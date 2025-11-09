#![cfg(feature = "postgres")]
//! Behavioural tests for recursive CTE helpers on `PostgreSQL`.

use diesel::RunQueryDsl as DieselRunQueryDsl;
use diesel::{dsl::sql, pg::PgConnection, sql_types::Integer};
#[cfg(feature = "async")]
use diesel_async::{AsyncConnection, AsyncPgConnection, RunQueryDsl as AsyncRunQueryDsl};
use diesel_cte_ext::{RecursiveCTEExt, RecursiveParts};
use pg_embedded_setup_unpriv::{BootstrapResult, TestCluster};
use rstest::{fixture, rstest};

type TestResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync + 'static>>;

fn configure_pg_embed_env() {
    use std::{env, fs, path::PathBuf, sync::Once};

    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/pg-embed");
        let runtime = base.join("runtime");
        let data = base.join("data");
        if let Err(err) = fs::create_dir_all(&runtime) {
            panic!("runtime directory: {err}");
        }
        if let Err(err) = fs::create_dir_all(&data) {
            panic!("data directory: {err}");
        }
        unsafe {
            env::set_var("PG_RUNTIME_DIR", runtime);
            env::set_var("PG_DATA_DIR", data);
        }
    });
}

#[fixture]
fn embedded_cluster() -> BootstrapResult<TestCluster> {
    configure_pg_embed_env();
    TestCluster::new()
}

#[rstest]
fn recursive_sequence_via_sync_conn(
    embedded_cluster: BootstrapResult<TestCluster>,
) -> TestResult<()> {
    let cluster = embedded_cluster?;
    let mut conn = cluster.connection().diesel_connection("postgres")?;

    let rows: Vec<i32> = DieselRunQueryDsl::load(
        PgConnection::with_recursive(
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
fn recursive_sequence_via_async_conn(
    embedded_cluster: BootstrapResult<TestCluster>,
) -> TestResult<()> {
    use tokio::runtime::Builder;

    let cluster = embedded_cluster?;
    let rt = Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("tokio runtime");

    let db_url = cluster.connection().database_url("postgres");

    rt.block_on(async move {
        let mut conn = AsyncPgConnection::establish(&db_url).await?;

        let rows: Vec<i32> = AsyncRunQueryDsl::load(
            AsyncPgConnection::with_recursive(
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
fn non_recursive_cte_returns_seed(
    embedded_cluster: BootstrapResult<TestCluster>,
) -> TestResult<()> {
    let cluster = embedded_cluster?;
    let mut conn = cluster.connection().diesel_connection("postgres")?;

    let result: i32 = DieselRunQueryDsl::get_result(
        PgConnection::with_cte(
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
