//! Unit tests for the CTE query types in [`crate::cte`].

use super::*;
use crate::{
    builders::{self, RecursiveParts},
    test_support::normalise_debug_sql,
};
use diesel::{debug_query, dsl::sql, expression::SqlLiteral, sql_types::Integer, sqlite::Sqlite};
use rstest::{fixture, rstest};

enum Builder {
    All,
    Distinct,
}

#[fixture]
fn sample_parts() -> RecursiveParts<SqlLiteral<Integer>, SqlLiteral<Integer>, SqlLiteral<Integer>> {
    RecursiveParts::new(
        sql::<Integer>("SELECT 1"),
        sql::<Integer>("SELECT n + 1 FROM nums WHERE n < 2"),
        sql::<Integer>("SELECT n FROM nums"),
    )
}

#[test]
fn duplicate_column_names_are_rejected() {
    let names = &["id", "id"];
    match ensure_unique_columns(names) {
        Err(err) => {
            assert!(matches!(err, Error::QueryBuilderError(_)));
            assert!(err.to_string().contains("duplicate column name"));
        }
        Ok(()) => panic!("expected duplicate column error"),
    }
}

#[rstest]
#[case::all(Builder::All, "UNION ALL")]
#[case::distinct(Builder::Distinct, "UNION")]
fn with_recursive_renders_expected_sql(
    sample_parts: RecursiveParts<SqlLiteral<Integer>, SqlLiteral<Integer>, SqlLiteral<Integer>>,
    #[case] builder: Builder,
    #[case] union_op: &str,
) {
    let query = match builder {
        Builder::All => {
            builders::with_recursive::<Sqlite, _, _, _, _, _>("nums", &["n"], sample_parts)
        }
        Builder::Distinct => {
            builders::with_recursive_not_all::<Sqlite, _, _, _, _, _>("nums", &["n"], sample_parts)
        }
    };

    let sql = normalise_debug_sql(&debug_query::<Sqlite, _>(&query).to_string());
    assert_eq!(
        sql,
        format!(
            "WITH RECURSIVE \"nums\" (\"n\") AS (SELECT 1 {union_op} SELECT n + 1 FROM nums WHERE n < 2) SELECT n FROM nums"
        )
    );
}

#[test]
fn with_cte_renders_expected_sql() {
    let query = builders::with_cte::<Sqlite, _, _, _, _>(
        "seed",
        &["value"],
        builders::CteParts::new(
            sql::<Integer>("SELECT 42"),
            sql::<Integer>("SELECT value FROM seed"),
        ),
    );
    let sql = normalise_debug_sql(&debug_query::<Sqlite, _>(&query).to_string());
    assert_eq!(
        sql,
        "WITH \"seed\" (\"value\") AS (SELECT 42) SELECT value FROM seed"
    );
}

#[test]
fn with_recursive_skips_identifier_list_when_empty() {
    let query = builders::with_recursive::<Sqlite, _, _, _, _, _>(
        "nums",
        &[] as &[&str],
        RecursiveParts::new(
            sql::<Integer>("SELECT 1"),
            sql::<Integer>("SELECT n + 1 FROM nums WHERE n < 2"),
            sql::<Integer>("SELECT n FROM nums"),
        ),
    );
    let sql = normalise_debug_sql(&debug_query::<Sqlite, _>(&query).to_string());
    assert_eq!(
        sql,
        "WITH RECURSIVE \"nums\" AS (SELECT 1 UNION ALL SELECT n + 1 FROM nums WHERE n < 2) SELECT n FROM nums"
    );
}

#[test]
fn query_id_reflects_runtime_union_choice() {
    type RecursiveQuery =
        WithRecursive<Sqlite, (), SqlLiteral<Integer>, SqlLiteral<Integer>, SqlLiteral<Integer>>;
    type CteQuery = WithCte<Sqlite, (), SqlLiteral<Integer>, SqlLiteral<Integer>>;

    let recursive_has_static =
        std::hint::black_box(<RecursiveQuery as QueryId>::HAS_STATIC_QUERY_ID);
    let cte_has_static = std::hint::black_box(<CteQuery as QueryId>::HAS_STATIC_QUERY_ID);

    assert!(!recursive_has_static);
    assert!(cte_has_static);
}
