//! Protects the builder contract that recursive CTE seed, step and body
//! inputs must be Diesel query fragments for the selected backend.

use diesel::{dsl::sql, sql_types::Integer, sqlite::SqliteConnection};
use diesel_cte_ext::{RecursiveCTEExt, RecursiveParts};

/// Attempts to build a recursive CTE with a seed that is not a query fragment.
fn rejects_non_query_fragment_seed(conn: &SqliteConnection) {
    let _query = conn.with_recursive(
        "series",
        &["n"],
        RecursiveParts::new(
            String::from("SELECT 1"),
            sql::<Integer>("SELECT n + 1 FROM series WHERE n < 10"),
            sql::<Integer>("SELECT n FROM series ORDER BY n"),
        ),
    );
}

/// Runs the compile-fail fixture.
fn main() {}
