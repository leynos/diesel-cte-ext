#![cfg(feature = "sqlite")]
//! Demonstrates constructing a recursive `WITH` block using `SQLite`.

use diesel::{Connection, RunQueryDsl, dsl::sql, sql_types::Integer, sqlite::SqliteConnection};
use diesel_cte_ext::{RecursiveCTEExt, RecursiveParts};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut conn = SqliteConnection::establish(":memory:")?;
    let values: Vec<i32> = SqliteConnection::with_recursive(
        "series",
        &["n"],
        RecursiveParts::new(
            sql::<Integer>("SELECT 1"),
            sql::<Integer>("SELECT n + 1 FROM series WHERE n < 10"),
            sql::<Integer>("SELECT n FROM series"),
        ),
    )
    .load(&mut conn)?;

    if values.len() != 10 {
        return Err("expected ten rows in the series".into());
    }
    Ok(())
}
