#![cfg(feature = "postgres")]
//! Property tests for recursive CTE search clause rendering on `PostgreSQL`.

use diesel::{debug_query, dsl::sql, expression::SqlLiteral, pg::Pg, sql_types::Integer};
use diesel_cte_ext::{RecursiveParts, SearchStyle, builders};
use proptest::{prelude::*, prop_oneof};

const ID: &[&str] = &["id"];
const PARENT_ID: &[&str] = &["parent_id"];
const ID_PARENT_ID: &[&str] = &["id", "parent_id"];
const PARENT_ID_ID: &[&str] = &["parent_id", "id"];

type TestParts = RecursiveParts<SqlLiteral<Integer>, SqlLiteral<Integer>, SqlLiteral<Integer>>;

fn sample_parts() -> TestParts {
    RecursiveParts::new(
        sql::<Integer>("SELECT id, parent_id FROM search_nodes WHERE parent_id IS NULL"),
        sql::<Integer>(concat!(
            "SELECT search_nodes.id, search_nodes.parent_id ",
            "FROM search_nodes INNER JOIN tree ON search_nodes.parent_id = tree.id"
        )),
        sql::<Integer>("SELECT id FROM tree ORDER BY ordercol"),
    )
}

fn search_styles() -> impl Strategy<Value = SearchStyle> {
    prop_oneof![
        Just(SearchStyle::BreadthFirst),
        Just(SearchStyle::DepthFirst),
    ]
}

fn search_column_lists() -> impl Strategy<Value = &'static [&'static str]> {
    proptest::sample::select(vec![ID, PARENT_ID, ID_PARENT_ID, PARENT_ID_ID])
}

const fn expected_style(style: SearchStyle) -> &'static str {
    match style {
        SearchStyle::BreadthFirst => "BREADTH FIRST",
        SearchStyle::DepthFirst => "DEPTH FIRST",
    }
}

fn quoted_columns(columns: &[&str]) -> String {
    columns
        .iter()
        .map(|column| format!("\"{column}\""))
        .collect::<Vec<_>>()
        .join(", ")
}

proptest! {
    #[test]
    fn search_clause_quotes_generated_column_lists(
        style in search_styles(),
        columns in search_column_lists(),
    ) {
        let query = builders::with_recursive::<Pg, _, _, _, _, _>(
            "tree",
            &["id", "parent_id"],
            sample_parts(),
        )
        .with_search(style, columns, "ordercol");

        let rendered = debug_query::<Pg, _>(&query).to_string();
        let expected_clause = format!(
            "SEARCH {} BY {} SET \"ordercol\"",
            expected_style(style),
            quoted_columns(columns)
        );

        prop_assert!(
            rendered.contains(&expected_clause),
            "expected SQL to contain {expected_clause:?}, rendered {rendered:?}",
        );
    }
}
