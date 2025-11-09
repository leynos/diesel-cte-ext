//! Helper types for constructing recursive CTE queries.
//!
//! [`with_recursive`] builds a [`WithRecursive`] query from a name, column list
//! and the [`RecursiveParts`] struct bundling the seed, step and body fragments.
//! These helpers are used indirectly via
//! [`crate::connection_ext::RecursiveCTEExt::with_recursive`].

use diesel::{backend::Backend, query_builder::QueryFragment};

use crate::{
    columns::Columns,
    cte::{RecursiveBackend, WithCte, WithRecursive},
};

/// Query fragments used by a recursive CTE.
#[derive(Debug, Clone)]
pub struct RecursiveParts<Seed, Step, Body> {
    /// Seed query producing the first row(s) of the CTE.
    pub seed: Seed,
    /// Step query referencing the previous iteration's result.
    pub step: Step,
    /// Query consuming the CTE.
    pub body: Body,
}

impl<Seed, Step, Body> RecursiveParts<Seed, Step, Body> {
    /// Bundle the seed, step and body queries together.
    pub const fn new(seed: Seed, step: Step, body: Body) -> Self {
        Self { seed, step, body }
    }
}

/// Build a recursive CTE query.
pub fn with_recursive<DB, Cols, Seed, Step, Body, ColSpec>(
    cte_name: &'static str,
    columns: ColSpec,
    parts: RecursiveParts<Seed, Step, Body>,
) -> WithRecursive<DB, Cols, Seed, Step, Body>
where
    DB: RecursiveBackend,
    Seed: QueryFragment<DB>,
    Step: QueryFragment<DB>,
    Body: QueryFragment<DB>,
    ColSpec: Into<Columns<Cols>>,
{
    WithRecursive {
        cte_name,
        columns: columns.into(),
        seed: parts.seed,
        step: parts.step,
        body: parts.body,
        _marker: std::marker::PhantomData,
    }
}

/// Build a non-recursive CTE query.
pub fn with_cte<DB, Cols, Cte, Body, ColSpec>(
    cte_name: &'static str,
    columns: ColSpec,
    cte: Cte,
    body: Body,
) -> WithCte<DB, Cols, Cte, Body>
where
    DB: Backend,
    Cte: QueryFragment<DB>,
    Body: QueryFragment<DB>,
    ColSpec: Into<Columns<Cols>>,
{
    WithCte {
        cte_name,
        columns: columns.into(),
        cte,
        body,
        _marker: std::marker::PhantomData,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::normalise_debug_sql;
    use diesel::{debug_query, dsl::sql, sql_types::Integer, sqlite::Sqlite};

    #[test]
    fn recursive_builder_composes_fragments() {
        let query = with_recursive::<Sqlite, _, _, _, _, _>(
            "nums",
            &["n"],
            RecursiveParts::new(
                sql::<Integer>("SELECT 1"),
                sql::<Integer>("SELECT n + 1 FROM nums"),
                sql::<Integer>("SELECT n FROM nums"),
            ),
        );
        let sql = normalise_debug_sql(&debug_query::<Sqlite, _>(&query).to_string());
        assert_eq!(
            sql,
            "WITH RECURSIVE \"nums\" (\"n\") AS (SELECT 1 UNION ALL SELECT n + 1 FROM nums) SELECT n FROM nums"
        );
    }

    #[test]
    fn non_recursive_builder_composes_fragments() {
        let query = with_cte::<Sqlite, _, _, _, _>(
            "nums",
            &["n"],
            sql::<Integer>("SELECT 1"),
            sql::<Integer>("SELECT n FROM nums"),
        );
        let sql = normalise_debug_sql(&debug_query::<Sqlite, _>(&query).to_string());
        assert_eq!(
            sql,
            "WITH \"nums\" (\"n\") AS (SELECT 1) SELECT n FROM nums"
        );
    }
}
