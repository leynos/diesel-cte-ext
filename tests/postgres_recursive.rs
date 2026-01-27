#![cfg(feature = "postgres")]
//! Behavioural tests for recursive CTE helpers on `PostgreSQL`.

#[path = "test_helpers.rs"]
mod test_helpers;

use diesel::RunQueryDsl as DieselRunQueryDsl;
use diesel::{dsl::sql, sql_types::Integer};
#[cfg(feature = "async")]
use diesel_async::{AsyncConnection, AsyncPgConnection, RunQueryDsl as AsyncRunQueryDsl};
use diesel_cte_ext::{CteParts, RecursiveCTEExt, RecursiveParts};
use pg_embedded_setup_unpriv::test_support::ensure_worker_env;
use pg_embedded_setup_unpriv::{
    BootstrapResult, ExecutionPrivileges, ScopedEnv, TemporaryDatabase, TestCluster,
    detect_execution_privileges,
};
use rstest::{fixture, rstest};
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

type TestResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync + 'static>>;

const TEMPLATE_DB_NAME: &str = "cte_ext_template";
const PG_EMBED_SETUP_UNPRIV_VERSION: &str = "0.4.0";
static DB_COUNTER: AtomicUsize = AtomicUsize::new(0);
static WORKER_SETUP: OnceLock<()> = OnceLock::new();
static WORKER_MANIFEST: OnceLock<PathBuf> = OnceLock::new();

fn configure_pg_embed_env() -> test_helpers::EnvVarGuard {
    use std::path::PathBuf;

    let base = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/pg-embed");
    let runtime = base.join("runtime");
    let data = base.join("data");
    test_helpers::EnvVarGuard::set_pg_paths(&runtime, &data)
}

type GuardedCluster = BootstrapResult<(test_helpers::EnvVarGuard, TestCluster)>;

fn target_dir() -> PathBuf {
    std::env::var_os("CARGO_TARGET_DIR").map_or_else(
        || PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target"),
        PathBuf::from,
    )
}

fn pg_worker_path() -> PathBuf {
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_owned());
    target_dir().join(profile).join("pg_worker")
}

fn pg_embed_manifest_path() -> PathBuf {
    WORKER_MANIFEST
        .get_or_init(|| {
            let cargo_home = std::env::var_os("CARGO_HOME").map_or_else(
                || {
                    let home = std::env::var_os("HOME")
                        .map_or_else(|| PathBuf::from("/root"), PathBuf::from);
                    home.join(".cargo")
                },
                PathBuf::from,
            );
            let src_dir = cargo_home.join("registry").join("src");
            let crate_dir = format!("pg-embed-setup-unpriv-{PG_EMBED_SETUP_UNPRIV_VERSION}");
            let entries = std::fs::read_dir(&src_dir)
                .unwrap_or_else(|err| panic!("failed to read cargo registry: {err}"));
            for entry in entries.flatten() {
                let candidate = entry.path().join(&crate_dir).join("Cargo.toml");
                if candidate.is_file() {
                    return candidate;
                }
            }
            panic!(
                "pg-embed-setup-unpriv manifest not found under {}",
                src_dir.display()
            );
        })
        .clone()
}

fn ensure_worker_binary() {
    WORKER_SETUP.get_or_init(|| {
        let worker_path = pg_worker_path();
        if worker_path.is_file() {
            return;
        }

        let manifest_path = pg_embed_manifest_path();
        let target_dir = target_dir();
        let status = Command::new("cargo")
            .arg("build")
            .arg("--manifest-path")
            .arg(&manifest_path)
            .arg("--bin")
            .arg("pg_worker")
            .arg("--target-dir")
            .arg(&target_dir)
            .status()
            .unwrap_or_else(|err| panic!("failed to invoke cargo to build pg_worker: {err}"));
        assert!(
            status.success(),
            "pg_worker build failed with status {status}"
        );
        assert!(
            worker_path.is_file(),
            "pg_worker binary missing at {}",
            worker_path.display()
        );
    });
}

fn worker_env_guard() -> Option<ScopedEnv> {
    if detect_execution_privileges() != ExecutionPrivileges::Root {
        return None;
    }

    if std::env::var_os("PG_EMBEDDED_WORKER").is_none() {
        ensure_worker_binary();
    }

    ensure_worker_env()
}

fn next_database_name() -> String {
    let id = DB_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("cte_ext_test_{id}")
}

fn templated_database(cluster: &TestCluster) -> BootstrapResult<TemporaryDatabase> {
    let connection = cluster.connection();
    connection.ensure_template_exists(TEMPLATE_DB_NAME, |_db_name| Ok(()))?;
    connection.temporary_database_from_template(next_database_name(), TEMPLATE_DB_NAME)
}

#[fixture]
fn embedded_cluster() -> GuardedCluster {
    let guard = configure_pg_embed_env();
    let worker_guard = worker_env_guard();
    TestCluster::new().map(|cluster| (guard, cluster.with_worker_guard(worker_guard)))
}

#[rstest]
fn recursive_sequence_via_sync_conn(embedded_cluster: GuardedCluster) -> TestResult<()> {
    let (_env_guard, cluster) = embedded_cluster?;
    let temp_db = templated_database(&cluster)?;
    let mut conn = cluster.connection().diesel_connection(temp_db.name())?;

    let rows: Vec<i32> = DieselRunQueryDsl::load(
        conn.with_recursive(
            "t",
            &["n"],
            RecursiveParts::new(
                sql::<Integer>("SELECT 1"),
                sql::<Integer>("SELECT n + 1 FROM t WHERE n < 5"),
                sql::<Integer>("SELECT n FROM t ORDER BY n"),
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
    let temp_db = templated_database(&cluster)?;
    let rt = Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("tokio runtime");

    let db_url = temp_db.url().to_owned();

    rt.block_on(async move {
        let mut conn = AsyncPgConnection::establish(&db_url).await?;

        let rows: Vec<i32> = AsyncRunQueryDsl::load(
            conn.with_recursive(
                "t",
                &["n"],
                RecursiveParts::new(
                    sql::<Integer>("SELECT 1"),
                    sql::<Integer>("SELECT n + 1 FROM t WHERE n < 5"),
                    sql::<Integer>("SELECT n FROM t ORDER BY n"),
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
    let temp_db = templated_database(&cluster)?;
    let mut conn = cluster.connection().diesel_connection(temp_db.name())?;

    let result: i32 = DieselRunQueryDsl::get_result(
        conn.with_cte(
            "seed",
            &["value"],
            CteParts::new(
                sql::<Integer>("SELECT 42"),
                sql::<Integer>("SELECT value FROM seed"),
            ),
        ),
        &mut conn,
    )?;

    if result != 42 {
        return Err("seed CTE did not round-trip 42".into());
    }
    Ok(())
}
