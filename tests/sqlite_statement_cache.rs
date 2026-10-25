#![cfg(feature = "sqlite")]
//! Regression tests for Diesel's prepared-statement cache.
//!
//! Diesel caches prepared statements by the query's Rust type when that type
//! reports a static query identifier. The table-backed query below is the real
//! cache regression: it shares a Rust type across runtime column lists but must
//! not report a static identifier. The raw-SQL cases use `sql::<Integer>`
//! fragments, which Diesel intentionally marks as unsafe to cache; they only
//! assert that the builder renders different SQL and should not be read as cache
//! regression tests.

use diesel::{
    Connection, QueryDsl, RunQueryDsl, connection::SimpleConnection, dsl::sql, sql_types::Integer,
    sqlite::SqliteConnection,
};
use diesel_cte_ext::{CteParts, RecursiveCTEExt, RecursiveParts};
use std::error::Error;

type TestResult<T> = Result<T, Box<dyn Error>>;

fn ensure_equal<T>(context: &'static str, actual: &T, expected: &T) -> TestResult<()>
where
    T: PartialEq + std::fmt::Debug,
{
    if actual == expected {
        Ok(())
    } else {
        Err(format!("{context}: expected {expected:?}, got {actual:?}").into())
    }
}

/// Schemas for the typed-fragment regression test.
mod schema {
    diesel::table! {
        people (id) {
            id -> Integer,
            age -> Integer,
        }
    }

    // Describes the CTE so its body can be written with Diesel's typed DSL.
    diesel::table! {
        pairs (a) {
            a -> Integer,
            b -> Integer,
        }
    }
}

use schema::{pairs, people};

/// A CTE built from literal SQL fragments.
struct CteSpec {
    name: &'static str,
    columns: &'static [&'static str],
    cte: &'static str,
    body: &'static str,
}

fn load_cte(conn: &mut SqliteConnection, spec: &CteSpec) -> TestResult<Vec<i32>> {
    // Raw SQL fragments are intentionally not prepared-statement-cacheable, so
    // they are only used here to check SQL rendering rather than cache reuse.
    let rows = conn
        .with_cte(
            spec.name,
            spec.columns,
            CteParts::new(sql::<Integer>(spec.cte), sql::<Integer>(spec.body)),
        )
        .load(conn)?;
    Ok(rows)
}

fn assert_distinct_cte_rows(
    first: CteSpec,
    second: CteSpec,
    expected_first: &[i32],
    expected_second: &[i32],
) -> TestResult<()> {
    let mut conn = SqliteConnection::establish(":memory:")?;
    ensure_equal(
        "first CTE",
        &load_cte(&mut conn, &first)?,
        &expected_first.to_vec(),
    )?;
    ensure_equal(
        "second CTE",
        &load_cte(&mut conn, &second)?,
        &expected_second.to_vec(),
    )
}

fn load_recursive(conn: &mut SqliteConnection, limit: u8) -> TestResult<Vec<i32>> {
    let step = format!("SELECT n + 1 FROM nums WHERE n < {limit}");
    let rows = conn
        .with_recursive(
            "nums",
            &["n"],
            RecursiveParts::new(
                sql::<Integer>("SELECT 1"),
                sql::<Integer>(&step),
                sql::<Integer>("SELECT n FROM nums ORDER BY n"),
            ),
        )
        .load(conn)?;
    Ok(rows)
}

/// Typed fragments are safe to cache and carry static query IDs, so only the
/// CTE's runtime column list distinguishes queries built by this helper.
fn load_pairs(
    conn: &mut SqliteConnection,
    columns: &'static [&'static str],
) -> TestResult<Vec<(i32, i32)>> {
    let rows = conn
        .with_cte(
            "pairs",
            columns,
            CteParts::new(
                people::table.select((people::id, people::age)),
                pairs::table.select((pairs::a, pairs::b)),
            ),
        )
        .load(conn)?;
    Ok(rows)
}

#[test]
fn typed_with_cte_queries_honour_their_runtime_column_lists() -> TestResult<()> {
    let mut conn = SqliteConnection::establish(":memory:")?;
    conn.batch_execute(
        "CREATE TABLE people (id INTEGER PRIMARY KEY, age INTEGER NOT NULL);
         INSERT INTO people (id, age) VALUES (1, 30);",
    )?;

    // `pairs (a, b)` binds id -> a, age -> b; `pairs (b, a)` binds id -> b, age -> a.
    ensure_equal(
        "pairs (a, b)",
        &load_pairs(&mut conn, &["a", "b"])?,
        &vec![(1, 30)],
    )?;
    // Before the fix this returned (1, 30): the statement cached for `pairs (a, b)`.
    ensure_equal(
        "pairs (b, a)",
        &load_pairs(&mut conn, &["b", "a"])?,
        &vec![(30, 1)],
    )
}

#[test]
fn with_cte_queries_with_different_fragment_text_render_their_own_sql() -> TestResult<()> {
    let first = CteSpec {
        name: "seed",
        columns: &["n"],
        cte: "SELECT 1",
        body: "SELECT n FROM seed",
    };
    let second = CteSpec {
        cte: "SELECT 2",
        ..first
    };

    assert_distinct_cte_rows(first, second, &[1], &[2])
}

#[test]
fn with_cte_queries_with_different_names_and_columns_render_their_own_sql() -> TestResult<()> {
    let first = CteSpec {
        name: "a",
        columns: &["x"],
        cte: "SELECT 1",
        body: "SELECT x * 10 FROM a",
    };
    let second = CteSpec {
        name: "b",
        columns: &["y"],
        cte: "SELECT 2",
        body: "SELECT y * 100 FROM b",
    };

    assert_distinct_cte_rows(first, second, &[10], &[200])
}

#[test]
fn with_recursive_queries_with_different_fragment_text_render_their_own_sql() -> TestResult<()> {
    let mut conn = SqliteConnection::establish(":memory:")?;
    ensure_equal("count to 2", &load_recursive(&mut conn, 2)?, &vec![1, 2])?;
    ensure_equal(
        "count to 4",
        &load_recursive(&mut conn, 4)?,
        &vec![1, 2, 3, 4],
    )
}
