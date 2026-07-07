//! Compile-pass fixture proving recursive CTE parts accept typed Diesel DSL.

use diesel::{
    Connection, allow_tables_to_appear_in_same_query, prelude::*, sqlite::SqliteConnection, table,
};
use diesel_cte_ext::{RecursiveCTEExt, RecursiveParts};

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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let conn = SqliteConnection::establish(":memory:")?;
    let _query = conn.with_recursive_not_all(
        "parents",
        &["id"],
        RecursiveParts::new(
            categories::table
                .select(categories::parent_category_id)
                .filter(categories::id.eq(4)),
            categories::table
                .select(categories::parent_category_id)
                .inner_join(parents::table.on(parents::id.assume_not_null().eq(categories::id))),
            parents::table
                .select(parents::id.assume_not_null())
                .filter(parents::id.is_not_null())
                .order(parents::id.desc()),
        ),
    );

    Ok(())
}
