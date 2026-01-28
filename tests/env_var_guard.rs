//! Tests for `EnvVarGuard` environment handling.

#[path = "test_helpers.rs"]
mod test_helpers;

use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

static TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

fn test_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|err| panic!("env var guard test mutex: {err}"))
}

struct EnvRestore {
    runtime: Option<OsString>,
    data: Option<OsString>,
    password: Option<OsString>,
}

impl EnvRestore {
    fn capture() -> Self {
        Self {
            runtime: env::var_os("PG_RUNTIME_DIR"),
            data: env::var_os("PG_DATA_DIR"),
            password: env::var_os("PG_PASSWORD"),
        }
    }
}

impl Drop for EnvRestore {
    fn drop(&mut self) {
        match self.runtime.as_ref() {
            Some(value) => unsafe { env::set_var("PG_RUNTIME_DIR", value) },
            None => unsafe { env::remove_var("PG_RUNTIME_DIR") },
        }

        match self.data.as_ref() {
            Some(value) => unsafe { env::set_var("PG_DATA_DIR", value) },
            None => unsafe { env::remove_var("PG_DATA_DIR") },
        }

        match self.password.as_ref() {
            Some(value) => unsafe { env::set_var("PG_PASSWORD", value) },
            None => unsafe { env::remove_var("PG_PASSWORD") },
        }
    }
}

fn unique_temp_dir() -> PathBuf {
    let mut base = env::temp_dir();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_else(|err| panic!("system time: {err}"))
        .as_nanos();
    let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    base.push(format!(
        "diesel_cte_ext_env_guard_{}_{}_{}",
        nanos,
        std::process::id(),
        counter
    ));
    fs::create_dir_all(&base).unwrap_or_else(|err| panic!("create temp dir: {err}"));
    base
}

fn remove_temp_dir(path: &Path) {
    if path.exists() {
        fs::remove_dir_all(path).unwrap_or_else(|err| panic!("remove temp dir: {err}"));
    }
}

#[test]
fn set_pg_paths_resets_data_dir_and_pgpass() {
    let _lock = test_lock();
    let _restore = EnvRestore::capture();
    let base = unique_temp_dir();
    let runtime_dir = base.join("runtime");
    let data_dir = base.join("data");

    fs::create_dir_all(&runtime_dir).unwrap_or_else(|err| panic!("create runtime dir: {err}"));
    fs::create_dir_all(&data_dir).unwrap_or_else(|err| panic!("create data dir: {err}"));
    let pgpass = runtime_dir.join(".pgpass");
    fs::write(&pgpass, "should be removed").unwrap_or_else(|err| panic!("write pgpass: {err}"));
    let junk = data_dir.join("junk.dat");
    fs::write(&junk, "old data").unwrap_or_else(|err| panic!("write junk: {err}"));

    {
        let _guard = test_helpers::EnvVarGuard::set_pg_paths(&runtime_dir, &data_dir);

        assert!(base.exists(), "base directory should exist");
        assert!(
            !pgpass.exists(),
            ".pgpass should be removed by reset_pgpass via set_pg_paths"
        );
        assert!(
            !data_dir.exists(),
            "data directory should be removed by reset_data_dir"
        );
    }

    remove_temp_dir(&base);
}

#[test]
fn set_pg_paths_sets_and_restores_env_vars_when_pre_set() {
    let _lock = test_lock();
    let _restore = EnvRestore::capture();
    unsafe {
        env::set_var("PG_RUNTIME_DIR", "pre_existing_runtime");
        env::set_var("PG_DATA_DIR", "pre_existing_data");
        env::set_var("PG_PASSWORD", "pre_existing_password");
    }

    let base = unique_temp_dir();
    let runtime_dir = base.join("runtime");
    let data_dir = base.join("data");

    {
        let _guard = test_helpers::EnvVarGuard::set_pg_paths(&runtime_dir, &data_dir);

        assert_eq!(
            env::var_os("PG_RUNTIME_DIR"),
            Some(runtime_dir.clone().into_os_string())
        );
        assert_eq!(
            env::var_os("PG_DATA_DIR"),
            Some(data_dir.clone().into_os_string())
        );
        assert_eq!(
            env::var("PG_PASSWORD")
                .unwrap_or_else(|err| panic!("PG_PASSWORD should be set: {err}")),
            test_helpers::TEST_PG_PASSWORD
        );
    }

    assert_eq!(
        env::var("PG_RUNTIME_DIR").unwrap_or_else(|err| panic!("PG_RUNTIME_DIR restored: {err}")),
        "pre_existing_runtime"
    );
    assert_eq!(
        env::var("PG_DATA_DIR").unwrap_or_else(|err| panic!("PG_DATA_DIR restored: {err}")),
        "pre_existing_data"
    );
    assert_eq!(
        env::var("PG_PASSWORD").unwrap_or_else(|err| panic!("PG_PASSWORD restored: {err}")),
        "pre_existing_password"
    );

    remove_temp_dir(&base);
}

#[test]
fn set_pg_paths_sets_and_restores_env_vars_when_unset() {
    let _lock = test_lock();
    let _restore = EnvRestore::capture();
    unsafe {
        env::remove_var("PG_RUNTIME_DIR");
        env::remove_var("PG_DATA_DIR");
        env::remove_var("PG_PASSWORD");
    }

    let base = unique_temp_dir();
    let runtime_dir = base.join("runtime");
    let data_dir = base.join("data");

    {
        let _guard = test_helpers::EnvVarGuard::set_pg_paths(&runtime_dir, &data_dir);

        assert_eq!(
            env::var_os("PG_RUNTIME_DIR"),
            Some(runtime_dir.clone().into_os_string())
        );
        assert_eq!(
            env::var_os("PG_DATA_DIR"),
            Some(data_dir.clone().into_os_string())
        );
        assert_eq!(
            env::var("PG_PASSWORD")
                .unwrap_or_else(|err| panic!("PG_PASSWORD should be set: {err}")),
            test_helpers::TEST_PG_PASSWORD
        );
    }

    assert!(env::var_os("PG_RUNTIME_DIR").is_none());
    assert!(env::var_os("PG_DATA_DIR").is_none());
    assert!(env::var_os("PG_PASSWORD").is_none());

    remove_temp_dir(&base);
}

#[test]
fn set_pg_paths_allows_sequential_guards() {
    let _lock = test_lock();
    let _restore = EnvRestore::capture();
    unsafe {
        env::set_var("PG_RUNTIME_DIR", "baseline_runtime");
        env::set_var("PG_DATA_DIR", "baseline_data");
        env::set_var("PG_PASSWORD", "baseline_password");
    }

    let base1 = unique_temp_dir();
    let runtime1 = base1.join("runtime1");
    let data1 = base1.join("data1");
    {
        let _guard = test_helpers::EnvVarGuard::set_pg_paths(&runtime1, &data1);
        assert_eq!(
            env::var_os("PG_RUNTIME_DIR"),
            Some(runtime1.clone().into_os_string())
        );
        assert_eq!(
            env::var_os("PG_DATA_DIR"),
            Some(data1.clone().into_os_string())
        );
    }

    let base2 = unique_temp_dir();
    let runtime2 = base2.join("runtime2");
    let data2 = base2.join("data2");
    {
        let _guard = test_helpers::EnvVarGuard::set_pg_paths(&runtime2, &data2);
        assert_eq!(
            env::var_os("PG_RUNTIME_DIR"),
            Some(runtime2.clone().into_os_string())
        );
        assert_eq!(
            env::var_os("PG_DATA_DIR"),
            Some(data2.clone().into_os_string())
        );
    }

    assert_eq!(
        env::var("PG_RUNTIME_DIR").unwrap_or_else(|err| panic!("PG_RUNTIME_DIR restored: {err}")),
        "baseline_runtime"
    );
    assert_eq!(
        env::var("PG_DATA_DIR").unwrap_or_else(|err| panic!("PG_DATA_DIR restored: {err}")),
        "baseline_data"
    );
    assert_eq!(
        env::var("PG_PASSWORD").unwrap_or_else(|err| panic!("PG_PASSWORD restored: {err}")),
        "baseline_password"
    );

    remove_temp_dir(&base1);
    remove_temp_dir(&base2);
}
