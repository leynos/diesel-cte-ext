#![cfg(feature = "sqlite")]
//! Demonstrates constructing a recursive `WITH` block using Diesel's typed DSL
//! instead of raw SQL fragments.

use diesel::{
    Connection, RunQueryDsl, allow_tables_to_appear_in_same_query, prelude::*,
    sqlite::SqliteConnection, table,
};
use diesel_cte_ext::{RecursiveCTEExt, RecursiveParts};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    run_dsl_recursive_example()?;
    Ok(())
}

/// Executes a recursive CTE built from typed Diesel query fragments.
///
/// # Errors
/// Returns an error if `SQLite` cannot create the fixture table, execute the
/// recursive CTE, or if the ancestry differs from the expected rows.
pub fn run_dsl_recursive_example() -> Result<Vec<i32>, Box<dyn std::error::Error>> {
    table! {
        categories (id) {
            id -> Integer,
            parent_category_id -> Nullable<Integer>,
        }
    }

    table! {
        parents (id) {
            id -> Nullable<Integer>,
        }
    }

    allow_tables_to_appear_in_same_query!(categories, parents);

    let mut conn = SqliteConnection::establish(":memory:")?;
    diesel::sql_query(
        "CREATE TABLE categories (
            id INTEGER PRIMARY KEY NOT NULL,
            parent_category_id INTEGER REFERENCES categories(id)
        )",
    )
    .execute(&mut conn)?;
    diesel::sql_query(
        "INSERT INTO categories (id, parent_category_id)
        VALUES (1, NULL), (2, 1), (3, 2), (4, 3)",
    )
    .execute(&mut conn)?;

    let ancestor_ids: Vec<i32> = conn
        .with_recursive_not_all(
            "parents",
            &["id"],
            RecursiveParts::new(
                categories::table
                    .select(categories::parent_category_id)
                    .filter(categories::id.eq(4)),
                categories::table
                    .select(categories::parent_category_id)
                    .inner_join(
                        parents::table.on(parents::id.assume_not_null().eq(categories::id)),
                    ),
                parents::table
                    .select(parents::id.assume_not_null())
                    .filter(parents::id.is_not_null())
                    .order(parents::id.desc()),
            ),
        )
        .load(&mut conn)?;

    let expected = vec![3, 2, 1];
    if ancestor_ids != expected {
        return Err(format!("expected {expected:?} but saw {ancestor_ids:?}").into());
    }
    Ok(ancestor_ids)
}

#[cfg(test)]
mod tests {
    //! Tests for the typed DSL recursive `SQLite` example.

    use super::*;

    #[test]
    fn returns_expected_ancestor_ids() {
        let ancestor_ids = run_dsl_recursive_example().expect("ancestor ids");
        assert_eq!(ancestor_ids, vec![3, 2, 1]);
    }
}
