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
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

type TestResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync + 'static>>;

const TEMPLATE_DB_NAME: &str = "cte_ext_template";
static DB_COUNTER: AtomicUsize = AtomicUsize::new(0);
static WORKER_SETUP: OnceLock<()> = OnceLock::new();
static WORKER_MANIFEST: OnceLock<PathBuf> = OnceLock::new();
static WORKER_VERSION: OnceLock<String> = OnceLock::new();

struct WorkerProfile {
    cargo_profile: String,
    target_dir: String,
}

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

fn worker_profile() -> WorkerProfile {
    let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_owned());
    match profile.as_str() {
        "debug" => WorkerProfile {
            cargo_profile: "dev".to_owned(),
            target_dir: "debug".to_owned(),
        },
        "release" => WorkerProfile {
            cargo_profile: "release".to_owned(),
            target_dir: "release".to_owned(),
        },
        other => WorkerProfile {
            cargo_profile: other.to_owned(),
            target_dir: other.to_owned(),
        },
    }
}

fn pg_worker_path(profile: &WorkerProfile) -> PathBuf {
    target_dir().join(&profile.target_dir).join("pg_worker")
}

fn cargo_lock_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.lock")
}

fn pg_embed_setup_unpriv_version() -> String {
    WORKER_VERSION
        .get_or_init(|| {
            let lock_path = cargo_lock_path();
            let contents = fs::read_to_string(&lock_path).unwrap_or_else(|err| {
                panic!(
                    "failed to read Cargo.lock at {}: {err}",
                    lock_path.display()
                )
            });
            parse_cargo_lock_version(&contents, "pg-embed-setup-unpriv").unwrap_or_else(|| {
                panic!("pg-embed-setup-unpriv not found in {}", lock_path.display())
            })
        })
        .clone()
}

fn parse_cargo_lock_version(contents: &str, crate_name: &str) -> Option<String> {
    let lockfile: toml::Value =
        toml::from_str(contents).unwrap_or_else(|err| panic!("failed to parse Cargo.lock: {err}"));
    lockfile
        .get("package")
        .and_then(|packages| packages.as_array())
        .and_then(|packages| {
            packages.iter().find_map(|package| {
                let name = package.get("name")?.as_str()?;
                let version = package.get("version")?.as_str()?;
                if name == crate_name {
                    Some(version.to_owned())
                } else {
                    None
                }
            })
        })
}

fn pg_embed_manifest_path() -> PathBuf {
    WORKER_MANIFEST
        .get_or_init(|| {
            let metadata = fetch_cargo_metadata();
            find_pg_embed_manifest(&metadata, &pg_embed_setup_unpriv_version())
        })
        .clone()
}

/// Fetch and parse cargo metadata for the workspace.
fn fetch_cargo_metadata() -> Value {
    let output = Command::new("cargo")
        .arg("metadata")
        .arg("--format-version")
        .arg("1")
        .arg("--locked")
        .output()
        .unwrap_or_else(|err| panic!("failed to run cargo metadata: {err}"));
    assert!(
        output.status.success(),
        "cargo metadata failed with status {}",
        output.status
    );
    serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|err| panic!("failed to parse cargo metadata output: {err}"))
}

/// Locate the pg-embed-setup-unpriv manifest in cargo metadata.
fn find_pg_embed_manifest(metadata: &Value, desired_version: &str) -> PathBuf {
    metadata
        .get("packages")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|package| {
            package.get("name").and_then(Value::as_str) == Some("pg-embed-setup-unpriv")
        })
        .find(|package| package.get("version").and_then(Value::as_str) == Some(desired_version))
        .or_else(|| {
            metadata
                .get("packages")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .find(|package| {
                    package.get("name").and_then(Value::as_str) == Some("pg-embed-setup-unpriv")
                })
        })
        .map(|package| {
            package
                .get("manifest_path")
                .and_then(Value::as_str)
                .map_or_else(
                    || panic!("pg-embed-setup-unpriv manifest_path missing in metadata"),
                    PathBuf::from,
                )
        })
        .map_or_else(
            || panic!("pg-embed-setup-unpriv not found in cargo metadata packages"),
            |path| path,
        )
}

fn ensure_worker_binary() {
    WORKER_SETUP.get_or_init(|| {
        let profile = worker_profile();
        let worker_path = pg_worker_path(&profile);
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
            .arg("--profile")
            .arg(&profile.cargo_profile)
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
