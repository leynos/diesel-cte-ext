//! Core types modelling CTE queries.
//!
//! [`WithRecursive`] represents recursive CTEs and [`WithCte`] represents
//! non-recursive CTEs, both as raw query fragments. [`RecursiveBackend`] marks
//! Diesel backends that support recursive queries.

use std::collections::BTreeSet;

use diesel::{
    backend::Backend,
    query_builder::{AstPass, Query, QueryFragment, QueryId},
    result::{Error, QueryResult},
};

use crate::columns::Columns;

macro_rules! impl_cte_traits {
    ($name:ident<$($gen:ident),*>, $body_ty:ident) => {
        impl<DB, Cols, $($gen),*> QueryId for $name<DB, Cols, $($gen),*>
        where
            DB: Backend + 'static,
            Cols: 'static,
            $($gen: 'static),*
        {
            type QueryId = Self;
            const HAS_STATIC_QUERY_ID: bool = true;
        }

        impl<DB, Cols, $($gen),*> Query for $name<DB, Cols, $($gen),*>
        where
            DB: Backend,
            $body_ty: Query,
        {
            type SqlType = <$body_ty as Query>::SqlType;
        }

        impl<DB, Cols, $($gen),*, Conn> diesel::query_dsl::RunQueryDsl<Conn>
            for $name<DB, Cols, $($gen),*>
        where
            DB: Backend,
            Conn: diesel::connection::Connection<Backend = DB>,
            Self: QueryFragment<DB> + QueryId + Query,
        {}
    };
}

fn push_identifiers<DB, Cols>(
    out: &mut AstPass<'_, '_, DB>,
    cols: &Columns<Cols>,
) -> QueryResult<()>
where
    DB: Backend,
{
    let ids = cols.names;
    if ids.is_empty() {
        return Ok(());
    }
    ensure_unique_columns(ids)?;
    out.push_sql(" (");
    for (i, id) in ids.iter().enumerate() {
        if i > 0 {
            out.push_sql(", ");
        }
        out.push_identifier(id)?;
    }
    out.push_sql(")");
    Ok(())
}

fn ensure_unique_columns(names: &[&str]) -> QueryResult<()> {
    let mut seen = BTreeSet::new();
    for name in names {
        if !seen.insert(name) {
            return Err(Error::QueryBuilderError(
                format!("duplicate column name '{name}' in CTE").into(),
            ));
        }
    }
    Ok(())
}

/// Marker trait for backends that support `WITH RECURSIVE`.
pub trait RecursiveBackend: Backend {}

#[cfg(feature = "sqlite")]
impl RecursiveBackend for diesel::sqlite::Sqlite {}

#[cfg(feature = "postgres")]
impl RecursiveBackend for diesel::pg::Pg {}

/// Representation of a recursive CTE query.
#[derive(Debug, Clone)]
pub struct WithRecursive<DB: Backend, Cols, Seed, Step, Body> {
    pub(crate) cte_name: &'static str,
    pub(crate) columns: Columns<Cols>,
    pub(crate) seed: Seed,
    pub(crate) step: Step,
    pub(crate) body: Body,
    pub(crate) _marker: std::marker::PhantomData<DB>,
}

impl<DB, Cols, Seed, Step, Body> QueryFragment<DB> for WithRecursive<DB, Cols, Seed, Step, Body>
where
    DB: Backend,
    Seed: QueryFragment<DB>,
    Step: QueryFragment<DB>,
    Body: QueryFragment<DB>,
{
    fn walk_ast<'b>(&'b self, mut out: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        out.push_sql("WITH RECURSIVE ");
        out.push_identifier(self.cte_name)?;
        push_identifiers(&mut out, &self.columns)?;
        out.push_sql(" AS (");
        self.seed.walk_ast(out.reborrow())?;
        out.push_sql(" UNION ALL ");
        self.step.walk_ast(out.reborrow())?;
        out.push_sql(") ");
        self.body.walk_ast(out.reborrow())
    }
}

/// Representation of a non-recursive CTE query.
#[derive(Debug, Clone)]
pub struct WithCte<DB: Backend, Cols, Cte, Body> {
    pub(crate) cte_name: &'static str,
    pub(crate) columns: Columns<Cols>,
    pub(crate) cte: Cte,
    pub(crate) body: Body,
    pub(crate) _marker: std::marker::PhantomData<DB>,
}

impl<DB, Cols, Cte, Body> QueryFragment<DB> for WithCte<DB, Cols, Cte, Body>
where
    DB: Backend,
    Cte: QueryFragment<DB>,
    Body: QueryFragment<DB>,
{
    fn walk_ast<'b>(&'b self, mut out: AstPass<'_, 'b, DB>) -> QueryResult<()> {
        out.push_sql("WITH ");
        out.push_identifier(self.cte_name)?;
        push_identifiers(&mut out, &self.columns)?;
        out.push_sql(" AS (");
        self.cte.walk_ast(out.reborrow())?;
        out.push_sql(") ");
        self.body.walk_ast(out.reborrow())
    }
}

impl_cte_traits!(WithRecursive<Seed, Step, Body>, Body);
impl_cte_traits!(WithCte<Cte, Body>, Body);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        builders::{self, RecursiveParts},
        test_support::normalise_debug_sql,
    };
    use diesel::{debug_query, dsl::sql, sql_types::Integer, sqlite::Sqlite};

    #[test]
    fn duplicate_column_names_are_rejected() {
        let names = &["id", "id"];
        match ensure_unique_columns(names) {
            Err(err) => {
                assert!(matches!(err, Error::QueryBuilderError(_)));
                assert!(err.to_string().contains("duplicate column name"));
            }
            Ok(()) => panic!("expected duplicate column error"),
        }
    }

    #[test]
    fn with_recursive_renders_expected_sql() {
        let query = builders::with_recursive::<Sqlite, _, _, _, _, _>(
            "nums",
            &["n"],
            RecursiveParts::new(
                sql::<Integer>("SELECT 1"),
                sql::<Integer>("SELECT n + 1 FROM nums WHERE n < 2"),
                sql::<Integer>("SELECT n FROM nums"),
            ),
        );

        let sql = normalise_debug_sql(&debug_query::<Sqlite, _>(&query).to_string());
        assert_eq!(
            sql,
            "WITH RECURSIVE \"nums\" (\"n\") AS (SELECT 1 UNION ALL SELECT n + 1 FROM nums WHERE n < 2) SELECT n FROM nums"
        );
    }

    #[test]
    fn with_cte_renders_expected_sql() {
        let query = builders::with_cte::<Sqlite, _, _, _, _>(
            "seed",
            &["value"],
            crate::builders::CteParts::new(
                sql::<Integer>("SELECT 42"),
                sql::<Integer>("SELECT value FROM seed"),
            ),
        );
        let sql = normalise_debug_sql(&debug_query::<Sqlite, _>(&query).to_string());
        assert_eq!(
            sql,
            "WITH \"seed\" (\"value\") AS (SELECT 42) SELECT value FROM seed"
        );
    }

    #[test]
    fn with_recursive_skips_identifier_list_when_empty() {
        let query = builders::with_recursive::<Sqlite, _, _, _, _, _>(
            "nums",
            &[] as &[&str],
            RecursiveParts::new(
                sql::<Integer>("SELECT 1"),
                sql::<Integer>("SELECT n + 1 FROM nums WHERE n < 2"),
                sql::<Integer>("SELECT n FROM nums"),
            ),
        );
        let sql = normalise_debug_sql(&debug_query::<Sqlite, _>(&query).to_string());
        assert_eq!(
            sql,
            "WITH RECURSIVE \"nums\" AS (SELECT 1 UNION ALL SELECT n + 1 FROM nums WHERE n < 2) SELECT n FROM nums"
        );
    }
}
