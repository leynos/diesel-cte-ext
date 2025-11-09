#![cfg(feature = "sqlite")]
//! Demonstrates non-recursive CTEs for seeding temporary data in `SQLite`.

use diesel::{Connection, RunQueryDsl, dsl::sql, sql_types::Text, sqlite::SqliteConnection};
use diesel_cte_ext::RecursiveCTEExt;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut conn = SqliteConnection::establish(":memory:")?;
    let names: Vec<String> = SqliteConnection::with_cte(
        "seed",
        &["message"],
        sql::<Text>("SELECT 'Hello' AS message UNION ALL SELECT 'Diesel'"),
        sql::<Text>("SELECT message FROM seed ORDER BY message DESC"),
    )
    .load(&mut conn)?;

    let expected = vec!["Hello".to_owned(), "Diesel".to_owned()];
    if names != expected {
        return Err("seeded names deviated from expectation".into());
    }
    Ok(())
}
