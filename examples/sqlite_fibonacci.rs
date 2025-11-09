#![cfg(feature = "sqlite")]
//! Demonstrates constructing a recursive `WITH` block using `SQLite`.

use diesel::{Connection, RunQueryDsl, dsl::sql, sql_types::Integer, sqlite::SqliteConnection};
use diesel_cte_ext::{RecursiveCTEExt, RecursiveParts};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    run_fibonacci_example()?;
    Ok(())
}

/// Executes the recursive CTE and returns the generated series.
///
/// # Errors
/// Returns an error if `SQLite` cannot execute the CTE or if the sequence
/// length deviates from the expected ten rows.
pub fn run_fibonacci_example() -> Result<Vec<i32>, Box<dyn std::error::Error>> {
    let mut conn = SqliteConnection::establish(":memory:")?;
    let values: Vec<i32> = conn
        .with_recursive(
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
    Ok(values)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_expected_series() {
        let values = run_fibonacci_example().expect("series");
        assert_eq!(values, (1..=10).collect::<Vec<_>>());
    }
}
