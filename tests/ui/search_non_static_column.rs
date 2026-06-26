//! Compile-fail UI test for non-static recursive search-column inputs.

use diesel::{dsl::sql, pg::PgConnection, sql_types::Integer};
use diesel_cte_ext::{RecursiveCTEExt, RecursiveParts, SearchStyle};

fn rejects_non_static_search_column(conn: &PgConnection) {
    let search_column = String::from("id");
    let _query = conn.with_recursive(
        "tree",
        &["id", "parent_id"],
        RecursiveParts::new(
            sql::<Integer>("SELECT id, parent_id FROM search_nodes WHERE parent_id IS NULL"),
            sql::<Integer>(concat!(
                "SELECT search_nodes.id, search_nodes.parent_id ",
                "FROM search_nodes INNER JOIN tree ON search_nodes.parent_id = tree.id"
            )),
            sql::<Integer>("SELECT id FROM tree ORDER BY ordercol"),
        ),
    )
    .with_search(SearchStyle::BreadthFirst, search_column.as_str(), "ordercol");
}

fn main() {}
