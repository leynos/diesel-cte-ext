#![cfg(feature = "sqlite")]
//! Behavioural tests ensuring the `SQLite` implementations of `RecursiveCTEExt`
//! function across sync and async entry points.

use diesel::{Connection, dsl::sql, sql_types::Integer, sqlite::SqliteConnection};
use diesel_cte_ext::{RecursiveCTEExt, RecursiveParts};

#[test]
fn sqlite_sync_recursive_sequence() {
    use diesel::RunQueryDsl;
    let mut conn = SqliteConnection::establish(":memory:").expect("in-memory sqlite");
    let rows: Vec<i32> = SqliteConnection::with_recursive(
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
        let rows: Vec<i32> = SyncConnectionWrapper::<SqliteConnection>::with_recursive(
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
