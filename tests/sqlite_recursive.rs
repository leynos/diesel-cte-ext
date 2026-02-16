#![cfg(feature = "sqlite")]
//! Behavioural tests ensuring the `SQLite` implementations of `RecursiveCTEExt`
//! work across sync and async entry points.

use diesel::{Connection, dsl::sql, sql_types::Integer, sqlite::SqliteConnection};
use diesel_cte_ext::{RecursiveCTEExt, RecursiveParts};
use rstest::{fixture, rstest};
use std::{error::Error, io};

type TestResult<T> = Result<T, Box<dyn Error>>;

#[test]
fn sqlite_sync_recursive_sequence() {
    use diesel::RunQueryDsl;
    let mut conn = SqliteConnection::establish(":memory:").expect("in-memory sqlite");
    let rows: Vec<i32> = conn
        .with_recursive(
            "nums",
            &["n"],
            RecursiveParts::new(
                sql::<Integer>("SELECT 1"),
                sql::<Integer>("SELECT n + 1 FROM nums WHERE n < 4"),
                sql::<Integer>("SELECT n FROM nums"),
            ),
        )
        .load(&mut conn)
        .expect("load rows");
    assert_eq!(rows, vec![1, 2, 3, 4]);
}

#[fixture]
fn graph_conn() -> TestResult<SqliteConnection> {
    use diesel::{RunQueryDsl, sql_query};

    let mut conn = SqliteConnection::establish(":memory:")?;
    sql_query("CREATE TABLE edges (source INTEGER NOT NULL, target INTEGER NOT NULL)")
        .execute(&mut conn)?;
    sql_query("INSERT INTO edges (source, target) VALUES (1, 2), (1, 3), (2, 3), (3, 2), (2, 4)")
        .execute(&mut conn)?;
    Ok(conn)
}

fn ensure_equal<T>(context: &'static str, actual: &T, expected: &T) -> TestResult<()>
where
    T: PartialEq + std::fmt::Debug,
{
    if actual == expected {
        return Ok(());
    }

    Err(io::Error::other(format!(
        "{context} mismatch. expected: {expected:?}, actual: {actual:?}"
    ))
    .into())
}

#[rstest]
fn sqlite_graph_cycle_duplicate_behaviour(
    graph_conn: TestResult<SqliteConnection>,
) -> TestResult<()> {
    use diesel::RunQueryDsl;
    let mut conn = graph_conn?;

    // UNION ALL requires explicit cycle limiting for this graph; duplicate rows
    // can still emerge and then need DISTINCT in the final body query.
    let union_all_rows: Vec<i32> = conn
        .with_recursive(
            "walk",
            &["node"],
            RecursiveParts::new(
                sql::<Integer>("SELECT 1"),
                sql::<Integer>(concat!(
                    "SELECT edges.target ",
                    "FROM edges ",
                    "INNER JOIN walk ON edges.source = walk.node ",
                    "WHERE walk.node < 3"
                )),
                sql::<Integer>("SELECT node FROM walk WHERE node <> 1 ORDER BY node"),
            ),
        )
        .load(&mut conn)?;
    ensure_equal("UNION ALL rows", &union_all_rows, &vec![2, 3, 3, 4])?;

    let union_all_distinct_rows: Vec<i32> = conn
        .with_recursive(
            "walk",
            &["node"],
            RecursiveParts::new(
                sql::<Integer>("SELECT 1"),
                sql::<Integer>(concat!(
                    "SELECT edges.target ",
                    "FROM edges ",
                    "INNER JOIN walk ON edges.source = walk.node ",
                    "WHERE walk.node < 3"
                )),
                sql::<Integer>("SELECT DISTINCT node FROM walk WHERE node <> 1 ORDER BY node"),
            ),
        )
        .load(&mut conn)?;

    let union_rows: Vec<i32> = conn
        .with_recursive_not_all(
            "walk",
            &["node"],
            RecursiveParts::new(
                sql::<Integer>("SELECT 1"),
                sql::<Integer>(concat!(
                    "SELECT edges.target ",
                    "FROM edges ",
                    "INNER JOIN walk ON edges.source = walk.node"
                )),
                sql::<Integer>("SELECT node FROM walk WHERE node <> 1 ORDER BY node"),
            ),
        )
        .load(&mut conn)?;

    ensure_equal("UNION rows", &union_rows, &vec![2, 3, 4])?;
    ensure_equal(
        "UNION vs UNION ALL DISTINCT rows",
        &union_rows,
        &union_all_distinct_rows,
    )?;
    Ok(())
}

#[rstest]
fn sqlite_prepared_statement_cache_isolation_between_union_modes(
    graph_conn: TestResult<SqliteConnection>,
) -> TestResult<()> {
    use diesel::RunQueryDsl;
    let mut conn = graph_conn?;

    let first_union_all: Vec<i32> = conn
        .with_recursive(
            "walk",
            &["node"],
            RecursiveParts::new(
                sql::<Integer>("SELECT 1"),
                sql::<Integer>(concat!(
                    "SELECT edges.target ",
                    "FROM edges ",
                    "INNER JOIN walk ON edges.source = walk.node ",
                    "WHERE walk.node < 3"
                )),
                sql::<Integer>("SELECT node FROM walk WHERE node <> 1 ORDER BY node"),
            ),
        )
        .load(&mut conn)?;

    let union_rows: Vec<i32> = conn
        .with_recursive_not_all(
            "walk",
            &["node"],
            RecursiveParts::new(
                sql::<Integer>("SELECT 1"),
                sql::<Integer>(concat!(
                    "SELECT edges.target ",
                    "FROM edges ",
                    "INNER JOIN walk ON edges.source = walk.node ",
                    "WHERE walk.node < 3"
                )),
                sql::<Integer>("SELECT node FROM walk WHERE node <> 1 ORDER BY node"),
            ),
        )
        .load(&mut conn)?;

    let second_union_all: Vec<i32> = conn
        .with_recursive(
            "walk",
            &["node"],
            RecursiveParts::new(
                sql::<Integer>("SELECT 1"),
                sql::<Integer>(concat!(
                    "SELECT edges.target ",
                    "FROM edges ",
                    "INNER JOIN walk ON edges.source = walk.node ",
                    "WHERE walk.node < 3"
                )),
                sql::<Integer>("SELECT node FROM walk WHERE node <> 1 ORDER BY node"),
            ),
        )
        .load(&mut conn)?;

    ensure_equal("first UNION ALL rows", &first_union_all, &vec![2, 3, 3, 4])?;
    ensure_equal("UNION rows", &union_rows, &vec![2, 3, 4])?;
    ensure_equal("second UNION ALL rows", &second_union_all, &first_union_all)?;
    Ok(())
}

#[cfg(feature = "async")]
mod async_sqlite {
    use super::*;
    use diesel_async::{
        AsyncConnection, RunQueryDsl as AsyncRunQueryDsl,
        sync_connection_wrapper::SyncConnectionWrapper,
    };
    use diesel_cte_ext::RecursiveCTEExt;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn sqlite_async_recursive_sequence() {
        let mut conn = SyncConnectionWrapper::<SqliteConnection>::establish(":memory:")
            .await
            .expect("async sqlite wrapper");
        let rows: Vec<i32> = conn
            .with_recursive(
                "nums",
                &["n"],
                RecursiveParts::new(
                    sql::<Integer>("SELECT 1"),
                    sql::<Integer>("SELECT n + 1 FROM nums WHERE n < 4"),
                    sql::<Integer>("SELECT n FROM nums"),
                ),
            )
            .load(&mut conn)
            .await
            .expect("load rows");
        assert_eq!(rows, vec![1, 2, 3, 4]);
    }
}
