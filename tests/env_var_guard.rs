//! Tests for `EnvVarGuard` environment handling.

#[path = "test_helpers.rs"]
mod test_helpers;

use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use rstest::{fixture, rstest};

static TEMP_COUNTER: AtomicUsize = AtomicUsize::new(0);

struct EnvTestGuard {
    lock: std::sync::MutexGuard<'static, ()>,
    runtime: Option<OsString>,
    data: Option<OsString>,
    password: Option<OsString>,
}

impl EnvTestGuard {
    fn capture() -> Self {
        let lock = test_helpers::env_lock_guard();
        Self {
            runtime: env::var_os("PG_RUNTIME_DIR"),
            data: env::var_os("PG_DATA_DIR"),
            password: env::var_os("PG_PASSWORD"),
            lock,
        }
    }

    const fn lock(&self) -> &std::sync::MutexGuard<'static, ()> {
        &self.lock
    }
}

impl Drop for EnvTestGuard {
    fn drop(&mut self) {
        test_helpers::restore_env_var_locked(self.lock(), "PG_RUNTIME_DIR", self.runtime.as_ref());
        test_helpers::restore_env_var_locked(self.lock(), "PG_DATA_DIR", self.data.as_ref());
        test_helpers::restore_env_var_locked(self.lock(), "PG_PASSWORD", self.password.as_ref());
    }
}

#[fixture]
fn env_setup() -> EnvTestGuard {
    EnvTestGuard::capture()
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

fn assert_env_var(name: &str, expected_value: Option<&str>) {
    match expected_value {
        Some(expected) => {
            let value = env::var(name).unwrap_or_else(|err| panic!("{name} should be set: {err}"));
            assert_eq!(value, expected, "{name} should be restored");
        }
        None => assert!(
            env::var_os(name).is_none(),
            "{name} should be unset after restore"
        ),
    }
}

#[rstest]
fn set_pg_paths_resets_data_dir_and_pgpass(env_setup: EnvTestGuard) {
    let env_lock = env_setup.lock();
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
        let _guard =
            test_helpers::EnvVarGuard::set_pg_paths_locked(env_lock, &runtime_dir, &data_dir);

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

#[rstest]
#[case::pre_set(
    Some("pre_existing_runtime"),
    Some("pre_existing_data"),
    Some("pre_existing_password")
)]
#[case::unset(None, None, None)]
fn set_pg_paths_sets_and_restores_env_vars(
    env_setup: EnvTestGuard,
    #[case] initial_runtime: Option<&'static str>,
    #[case] initial_data: Option<&'static str>,
    #[case] initial_password: Option<&'static str>,
) {
    let env_lock = env_setup.lock();
    test_helpers::set_env_var_locked(env_lock, "PG_RUNTIME_DIR", initial_runtime);
    test_helpers::set_env_var_locked(env_lock, "PG_DATA_DIR", initial_data);
    test_helpers::set_env_var_locked(env_lock, "PG_PASSWORD", initial_password);

    let base = unique_temp_dir();
    let runtime_dir = base.join("runtime");
    let data_dir = base.join("data");

    {
        let _guard =
            test_helpers::EnvVarGuard::set_pg_paths_locked(env_lock, &runtime_dir, &data_dir);

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

    assert_env_var("PG_RUNTIME_DIR", initial_runtime);
    assert_env_var("PG_DATA_DIR", initial_data);
    assert_env_var("PG_PASSWORD", initial_password);

    remove_temp_dir(&base);
}

#[rstest]
fn set_pg_paths_allows_sequential_guards(env_setup: EnvTestGuard) {
    let env_lock = env_setup.lock();
    test_helpers::set_env_var_locked(env_lock, "PG_RUNTIME_DIR", Some("baseline_runtime"));
    test_helpers::set_env_var_locked(env_lock, "PG_DATA_DIR", Some("baseline_data"));
    test_helpers::set_env_var_locked(env_lock, "PG_PASSWORD", Some("baseline_password"));

    let base1 = unique_temp_dir();
    let runtime1 = base1.join("runtime1");
    let data1 = base1.join("data1");
    {
        let _guard = test_helpers::EnvVarGuard::set_pg_paths_locked(env_lock, &runtime1, &data1);
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
        let _guard = test_helpers::EnvVarGuard::set_pg_paths_locked(env_lock, &runtime2, &data2);
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
