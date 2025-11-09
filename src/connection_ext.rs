//! Extension trait exposing a `with_recursive` method on Diesel connections.
//!
//! This trait provides convenient access to [`builders::with_recursive`] with
//! backend inference from the connection type. Both synchronous and
//! asynchronous Diesel connections implement `RecursiveCTEExt`.

use diesel::query_builder::QueryFragment;

#[cfg(all(feature = "async", feature = "sqlite"))]
use diesel_async::sync_connection_wrapper::SyncConnectionWrapper;

use crate::{
    builders::{self, RecursiveParts},
    columns::Columns,
    cte::{RecursiveBackend, WithCte, WithRecursive},
};

/// Extension trait providing a convenient `with_recursive` method on
/// connection types.
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
    fn with_cte<Cols, Cte, Body, ColSpec>(
        cte_name: &'static str,
        columns: ColSpec,
        cte: Cte,
        body: Body,
    ) -> WithCte<Self::Backend, Cols, Cte, Body>
    where
        Cte: QueryFragment<Self::Backend>,
        Body: QueryFragment<Self::Backend>,
        ColSpec: Into<Columns<Cols>>,
    {
        builders::with_cte::<Self::Backend, Cols, _, _, _>(cte_name, columns, cte, body)
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

    #[cfg(feature = "sqlite")]
    #[test]
    fn sqlite_connection_exposes_with_recursive() {
        use diesel::sqlite::Sqlite;

        let query =
            diesel::sqlite::SqliteConnection::with_recursive("nums", &["n"], sample_parts());
        let sql = normalise_debug_sql(&debug_query::<Sqlite, _>(&query).to_string());
        assert_eq!(sql, expected_recursive_sql());
    }

    #[cfg(feature = "sqlite")]
    #[test]
    fn sqlite_connection_exposes_with_cte() {
        use diesel::sqlite::Sqlite;

        let query = diesel::sqlite::SqliteConnection::with_cte(
            "seed",
            &["value"],
            sql::<Integer>("SELECT 42"),
            sql::<Integer>("SELECT value FROM seed"),
        );
        let sql = normalise_debug_sql(&debug_query::<Sqlite, _>(&query).to_string());
        assert_eq!(
            sql,
            "WITH \"seed\" (\"value\") AS (SELECT 42) SELECT value FROM seed"
        );
    }

    #[cfg(feature = "postgres")]
    #[test]
    fn postgres_connection_exposes_with_recursive() {
        use diesel::pg::Pg;

        let query = diesel::pg::PgConnection::with_recursive("nums", &["n"], sample_parts());
        let sql = normalise_debug_sql(&debug_query::<Pg, _>(&query).to_string());
        assert_eq!(sql, expected_recursive_sql());
    }

    #[cfg(all(feature = "async", feature = "postgres"))]
    #[test]
    fn async_postgres_connection_exposes_with_recursive() {
        use diesel::pg::Pg;
        use diesel_async::AsyncPgConnection;

        let query = AsyncPgConnection::with_recursive("nums", &["n"], sample_parts());
        let sql = normalise_debug_sql(&debug_query::<Pg, _>(&query).to_string());
        assert_eq!(sql, expected_recursive_sql());
    }

    #[cfg(all(feature = "async", feature = "sqlite"))]
    #[test]
    fn async_sqlite_wrapper_exposes_with_recursive() {
        use diesel::sqlite::Sqlite;
        use diesel_async::sync_connection_wrapper::SyncConnectionWrapper;

        let query = SyncConnectionWrapper::<diesel::sqlite::SqliteConnection>::with_recursive(
            "nums",
            &["n"],
            sample_parts(),
        );
        let sql = normalise_debug_sql(&debug_query::<Sqlite, _>(&query).to_string());
        assert_eq!(sql, expected_recursive_sql());
    }

    fn sample_parts() -> RecursiveParts<SqlLiteral<Integer>, SqlLiteral<Integer>, SqlLiteral<Integer>> {
        RecursiveParts::new(
            sql::<Integer>("SELECT 1"),
            sql::<Integer>("SELECT n + 1 FROM nums WHERE n < 5"),
            sql::<Integer>("SELECT n FROM nums"),
        )
    }

    fn expected_recursive_sql() -> &'static str {
        "WITH RECURSIVE \"nums\" (\"n\") AS (SELECT 1 UNION ALL SELECT n + 1 FROM nums WHERE n < 5) SELECT n FROM nums"
    }
}
