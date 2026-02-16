#![cfg(feature = "sqlite")]
//! Behavioural tests ensuring the `SQLite` implementations of `RecursiveCTEExt`
//! function across sync and async entry points.

use diesel::{Connection, dsl::sql, sql_types::Integer, sqlite::SqliteConnection};
use diesel_cte_ext::{RecursiveCTEExt, RecursiveParts};
use rstest::{fixture, rstest};

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
fn graph_conn() -> SqliteConnection {
    use diesel::{RunQueryDsl, sql_query};

    let mut conn = match SqliteConnection::establish(":memory:") {
        Ok(conn) => conn,
        Err(err) => panic!("in-memory sqlite: {err}"),
    };
    if let Err(err) =
        sql_query("CREATE TABLE edges (source INTEGER NOT NULL, target INTEGER NOT NULL)")
            .execute(&mut conn)
    {
        panic!("create edges table: {err}");
    }
    sql_query("INSERT INTO edges (source, target) VALUES (1, 2), (1, 3), (2, 3), (3, 2), (2, 4)")
        .execute(&mut conn)
        .unwrap_or_else(|err| panic!("insert graph edges: {err}"));
    conn
}

#[rstest]
fn sqlite_graph_cycle_duplicate_behaviour(mut graph_conn: SqliteConnection) {
    use diesel::RunQueryDsl;

    // UNION ALL requires explicit cycle limiting for this graph; duplicate rows
    // can still emerge and then need DISTINCT in the final body query.
    let union_all_rows: Vec<i32> = graph_conn
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
        .load(&mut graph_conn)
        .expect("load UNION ALL rows");
    assert_eq!(union_all_rows, vec![2, 3, 3, 4]);

    let union_all_distinct_rows: Vec<i32> = graph_conn
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
        .load(&mut graph_conn)
        .expect("load DISTINCT UNION ALL rows");

    let union_rows: Vec<i32> = graph_conn
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
        .load(&mut graph_conn)
        .expect("load UNION rows");

    assert_eq!(union_rows, vec![2, 3, 4]);
    assert_eq!(union_rows, union_all_distinct_rows);
}

#[rstest]
fn sqlite_prepared_statement_cache_isolation_between_union_modes(mut graph_conn: SqliteConnection) {
    use diesel::RunQueryDsl;

    let first_union_all: Vec<i32> = graph_conn
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
        .load(&mut graph_conn)
        .expect("load first UNION ALL rows");

    let union_rows: Vec<i32> = graph_conn
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
        .load(&mut graph_conn)
        .expect("load UNION rows");

    let second_union_all: Vec<i32> = graph_conn
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
        .load(&mut graph_conn)
        .expect("load second UNION ALL rows");

    assert_eq!(first_union_all, vec![2, 3, 3, 4]);
    assert_eq!(union_rows, vec![2, 3, 4]);
    assert_eq!(second_union_all, first_union_all);
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
