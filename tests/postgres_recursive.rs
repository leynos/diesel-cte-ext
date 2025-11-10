#![cfg(feature = "postgres")]
//! Behavioural tests for recursive CTE helpers on `PostgreSQL`.

#[path = "test_helpers.rs"]
mod test_helpers;

use diesel::RunQueryDsl as DieselRunQueryDsl;
use diesel::{dsl::sql, sql_types::Integer};
#[cfg(feature = "async")]
use diesel_async::{AsyncConnection, AsyncPgConnection, RunQueryDsl as AsyncRunQueryDsl};
use diesel_cte_ext::{RecursiveCTEExt, RecursiveParts};
use pg_embedded_setup_unpriv::{BootstrapResult, TestCluster};
use rstest::{fixture, rstest};

type TestResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync + 'static>>;

fn configure_pg_embed_env() -> test_helpers::EnvVarGuard {
    use std::path::PathBuf;

    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/pg-embed");
    let runtime = base.join("runtime");
    let data = base.join("data");
    test_helpers::EnvVarGuard::set_pg_paths(&runtime, &data)
}

type GuardedCluster = BootstrapResult<(test_helpers::EnvVarGuard, TestCluster)>;

#[fixture]
fn embedded_cluster() -> GuardedCluster {
    let guard = configure_pg_embed_env();
    TestCluster::new().map(|cluster| (guard, cluster))
}

#[rstest]
fn recursive_sequence_via_sync_conn(embedded_cluster: GuardedCluster) -> TestResult<()> {
    let (_env_guard, cluster) = embedded_cluster?;
    let mut conn = cluster.connection().diesel_connection("postgres")?;

    let rows: Vec<i32> = DieselRunQueryDsl::load(
        conn.with_recursive(
            "t",
            &["n"],
            RecursiveParts::new(
                sql::<Integer>("SELECT 1"),
                sql::<Integer>("SELECT n + 1 FROM t WHERE n < 5"),
                sql::<Integer>("SELECT n FROM t"),
            ),
        ),
        &mut conn,
    )?;

    let expected = [1, 2, 3, 4, 5];
    if rows != expected {
        return Err(format!("expected {expected:?} but saw {rows:?}").into());
    }
    Ok(())
}

#[cfg(feature = "async")]
#[rstest]
fn recursive_sequence_via_async_conn(embedded_cluster: GuardedCluster) -> TestResult<()> {
    use tokio::runtime::Builder;

    let (_env_guard, cluster) = embedded_cluster?;
    let rt = Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("tokio runtime");

    let db_url = cluster.connection().database_url("postgres");

    rt.block_on(async move {
        let mut conn = AsyncPgConnection::establish(&db_url).await?;

        let rows: Vec<i32> = AsyncRunQueryDsl::load(
            conn.with_recursive(
                "t",
                &["n"],
                RecursiveParts::new(
                    sql::<Integer>("SELECT 1"),
                    sql::<Integer>("SELECT n + 1 FROM t WHERE n < 5"),
                    sql::<Integer>("SELECT n FROM t"),
                ),
            ),
            &mut conn,
        )
        .await?;

        let expected = [1, 2, 3, 4, 5];
        if rows != expected {
            return Err(format!("expected {expected:?} but saw {rows:?}").into());
        }

        Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
    })?;

    Ok(())
}

#[rstest]
fn non_recursive_cte_returns_seed(embedded_cluster: GuardedCluster) -> TestResult<()> {
    let (_env_guard, cluster) = embedded_cluster?;
    let mut conn = cluster.connection().diesel_connection("postgres")?;

    let result: i32 = DieselRunQueryDsl::get_result(
        conn.with_cte(
            "seed",
            &["value"],
            sql::<Integer>("SELECT 42"),
            sql::<Integer>("SELECT value FROM seed"),
        ),
        &mut conn,
    )?;

    if result != 42 {
        return Err("seed CTE did not round-trip 42".into());
    }
    Ok(())
}
