use diesel::{dsl::sql, pg::PgConnection, sql_types::Integer};
use diesel_cte_ext::{RecursiveCTEExt, RecursiveParts, SearchStyle};

fn accepts_static_search_columns(conn: &PgConnection) {
    let _single_column = conn.with_recursive(
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
    .with_search(SearchStyle::BreadthFirst, "id", "ordercol");

    let _column_list = conn.with_recursive(
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
    .with_search(SearchStyle::DepthFirst, &["id", "parent_id"], "ordercol");
}

fn main() {}
