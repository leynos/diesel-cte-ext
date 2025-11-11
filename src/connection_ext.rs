//! Extension trait exposing `with_recursive` and `with_cte` on Diesel connections.
//!
//! Both helpers delegate to the builders module whilst inferring the backend
//! from the connection type, so callers never pass the backend explicitly.

use diesel::query_builder::QueryFragment;

#[cfg(all(feature = "async", feature = "sqlite"))]
use diesel_async::sync_connection_wrapper::SyncConnectionWrapper;

use crate::{
    builders::{self, CteParts, RecursiveParts},
    columns::Columns,
    cte::{RecursiveBackend, WithCte, WithRecursive},
};

/// Extension trait providing convenient `with_recursive` and `with_cte` methods
/// on connection types.
///
/// The backend is inferred from the connection, so callers do not need to
/// specify it explicitly.
pub trait RecursiveCTEExt {
    /// Backend associated with the connection.
    type Backend: RecursiveBackend;

    /// Create a [`WithRecursive`] builder for this connection's backend.
    ///
    /// See [`builders::with_recursive`] for parameter details.
    #[doc(alias = "builders::with_recursive")]
    fn with_recursive<Cols, Seed, Step, Body, ColSpec>(
        &self,
        cte_name: &'static str,
        columns: ColSpec,
        parts: RecursiveParts<Seed, Step, Body>,
    ) -> WithRecursive<Self::Backend, Cols, Seed, Step, Body>
    where
        Seed: QueryFragment<Self::Backend>,
        Step: QueryFragment<Self::Backend>,
        Body: QueryFragment<Self::Backend>,
        ColSpec: Into<Columns<Cols>>,
    {
        builders::with_recursive::<Self::Backend, Cols, _, _, _, _>(cte_name, columns, parts)
    }

    /// Create a [`WithCte`] builder for this connection's backend.
    #[doc(alias = "builders::with_cte")]
    fn with_cte<Cols, Cte, Body, ColSpec>(
        &self,
        cte_name: &'static str,
        columns: ColSpec,
        parts: CteParts<Cte, Body>,
    ) -> WithCte<Self::Backend, Cols, Cte, Body>
    where
        Cte: QueryFragment<Self::Backend>,
        Body: QueryFragment<Self::Backend>,
        ColSpec: Into<Columns<Cols>>,
    {
        builders::with_cte::<Self::Backend, Cols, _, _, _>(cte_name, columns, parts)
    }
}

/// Implementation of [`RecursiveCTEExt`] for synchronous `PostgreSQL` connections.
#[cfg(feature = "postgres")]
impl RecursiveCTEExt for diesel::pg::PgConnection {
    type Backend = diesel::pg::Pg;
}

/// Implementation of [`RecursiveCTEExt`] for synchronous `SQLite` connections.
#[cfg(feature = "sqlite")]
impl RecursiveCTEExt for diesel::sqlite::SqliteConnection {
    type Backend = diesel::sqlite::Sqlite;
}

/// Implementation of [`RecursiveCTEExt`] for `diesel_async` `PostgreSQL` connections.
#[cfg(all(feature = "async", feature = "postgres"))]
impl RecursiveCTEExt for diesel_async::AsyncPgConnection {
    type Backend = diesel::pg::Pg;
}

/// Implementation of [`RecursiveCTEExt`] for Diesel's async `SQLite` wrapper.
///
/// `diesel_async` exposes `SQLite` via [`SyncConnectionWrapper`], so we forward the
/// helper to that type instead of an `AsyncSqliteConnection` newtype.
#[cfg(all(feature = "async", feature = "sqlite"))]
impl<B> RecursiveCTEExt for SyncConnectionWrapper<diesel::sqlite::SqliteConnection, B> {
    type Backend = diesel::sqlite::Sqlite;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{builders::RecursiveParts, test_support::normalise_debug_sql};
    use diesel::{debug_query, dsl::sql, expression::SqlLiteral, sql_types::Integer};
    use std::marker::PhantomData;

    #[cfg(feature = "sqlite")]
    #[test]
    fn sqlite_backend_builds_recursive_sql() {
        use diesel::sqlite::Sqlite;

        let conn = DummyConn::<Sqlite>::default();
        let query = conn.with_recursive("nums", &["n"], sample_parts());
        let sql = normalise_debug_sql(&debug_query::<Sqlite, _>(&query).to_string());
        assert_eq!(sql, expected_recursive_sql());
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn sqlite_backend_builds_cte_sql() {
        use diesel::sqlite::Sqlite;

        let conn = DummyConn::<Sqlite>::default();
        let query = conn.with_cte(
            "seed",
            &["value"],
            CteParts::new(
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

    #[cfg(feature = "postgres")]
    #[test]
    fn postgres_backend_builds_recursive_sql() {
        use diesel::pg::Pg;

        let conn = DummyConn::<Pg>::default();
        let query = conn.with_recursive("nums", &["n"], sample_parts());
        let sql = normalise_debug_sql(&debug_query::<Pg, _>(&query).to_string());
        assert_eq!(sql, expected_recursive_sql());
    }

    #[test]
    fn connection_types_implement_recursive_ext() {
        fn assert_impl<T: RecursiveCTEExt>() {}

        #[cfg(feature = "sqlite")]
        {
            assert_impl::<diesel::sqlite::SqliteConnection>();
            #[cfg(feature = "async")]
            assert_impl::<
                diesel_async::sync_connection_wrapper::SyncConnectionWrapper<
                    diesel::sqlite::SqliteConnection,
                >,
            >();
        }

        #[cfg(feature = "postgres")]
        {
            assert_impl::<diesel::pg::PgConnection>();
            #[cfg(feature = "async")]
            assert_impl::<diesel_async::AsyncPgConnection>();
        }
    }

    fn sample_parts()
    -> RecursiveParts<SqlLiteral<Integer>, SqlLiteral<Integer>, SqlLiteral<Integer>> {
        RecursiveParts::new(
            sql::<Integer>("SELECT 1"),
            sql::<Integer>("SELECT n + 1 FROM nums WHERE n < 5"),
            sql::<Integer>("SELECT n FROM nums"),
        )
    }

    fn expected_recursive_sql() -> &'static str {
        "WITH RECURSIVE \"nums\" (\"n\") AS (SELECT 1 UNION ALL SELECT n + 1 FROM nums WHERE n < 5) SELECT n FROM nums"
    }

    #[derive(Default)]
    struct DummyConn<DB>(PhantomData<DB>);

    impl<DB> RecursiveCTEExt for DummyConn<DB>
    where
        DB: RecursiveBackend,
    {
        type Backend = DB;
    }
}
