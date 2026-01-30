//! Shared helpers for integration tests.

use std::{
    env,
    ffi::OsString,
    fs,
    path::Path,
    path::PathBuf,
    sync::{Mutex, MutexGuard, OnceLock},
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use pg_embedded_setup_unpriv::{ExecutionPrivileges, detect_execution_privileges};

/// Password used by embedded Postgres tests.
pub const TEST_PG_PASSWORD: &str = "postgres_pass";

fn env_mutex() -> &'static Mutex<()> {
    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    ENV_LOCK.get_or_init(|| Mutex::new(()))
}

fn lock_env_mutex() -> MutexGuard<'static, ()> {
    env_mutex()
        .lock()
        .unwrap_or_else(|err| panic!("env mutex poisoned: {err}"))
}

/// Execute a closure while holding the shared environment mutex.
///
/// # Panics
///
/// Panics if the environment mutex is poisoned.
pub fn with_env_lock<T>(f: impl FnOnce() -> T) -> T {
    let _lock = lock_env_mutex();
    f()
}

/// Acquire the shared environment mutex for a longer-lived critical section.
///
/// # Panics
///
/// Panics if the environment mutex is poisoned.
pub fn env_lock_guard() -> MutexGuard<'static, ()> {
    lock_env_mutex()
}

fn reset_data_dir(data_dir: &Path) {
    if data_dir.exists() {
        fs::remove_dir_all(data_dir).unwrap_or_else(|err| panic!("data directory cleanup: {err}"));
    }
}

fn reset_pgpass(runtime_dir: &Path) {
    let pgpass = runtime_dir.join(".pgpass");
    if let Err(err) = fs::remove_file(&pgpass) {
        assert!(is_not_found(&err), "pgpass cleanup: {err}");
    }
}

fn is_not_found(err: &std::io::Error) -> bool {
    err.kind() == std::io::ErrorKind::NotFound
}

fn ensure_pg_embed_base(runtime_dir: &Path, data_dir: &Path) {
    let base_dir = runtime_dir
        .parent()
        .map_or_else(|| runtime_dir.to_path_buf(), PathBuf::from);
    let data_parent = data_dir
        .parent()
        .map_or_else(|| data_dir.to_path_buf(), PathBuf::from);
    assert!(
        base_dir == data_parent,
        "runtime/data directories must share the same parent: {} vs {}",
        runtime_dir.display(),
        data_dir.display()
    );

    fs::create_dir_all(&base_dir).unwrap_or_else(|err| panic!("base directory: {err}"));
    #[cfg(unix)]
    {
        // Keep base directories writable when running as root so the worker
        // process can initialize Postgres under a different user.
        let desired_mode = match detect_execution_privileges() {
            ExecutionPrivileges::Root => 0o777,
            ExecutionPrivileges::Unprivileged => 0o700,
        };
        let mut perms = fs::metadata(&base_dir)
            .unwrap_or_else(|err| panic!("base directory metadata: {err}"))
            .permissions();
        if perms.mode() != desired_mode {
            perms.set_mode(desired_mode);
            fs::set_permissions(&base_dir, perms)
                .unwrap_or_else(|err| panic!("base directory permissions: {err}"));
        }
    }
}

/// Restore an environment variable from a captured value.
fn restore_env_var_unlocked(name: &str, value: Option<&OsString>) {
    match value {
        Some(stored) => unsafe { env::set_var(name, stored) },
        None => unsafe { env::remove_var(name) },
    }
}

/// Restore an environment variable while holding the shared mutex.
///
/// # Panics
///
/// Panics if the environment mutex is poisoned.
pub fn restore_env_var(name: &str, value: Option<&OsString>) {
    with_env_lock(|| restore_env_var_unlocked(name, value));
}

/// Restore an environment variable while already holding the shared mutex.
///
/// Callers must hold the env mutex via `env_lock_guard` or `with_env_lock`.
pub fn restore_env_var_locked(
    _lock: &MutexGuard<'static, ()>,
    name: &str,
    value: Option<&OsString>,
) {
    restore_env_var_unlocked(name, value);
}

/// Set or clear an environment variable while holding the shared mutex.
///
/// # Panics
///
/// Panics if the environment mutex is poisoned.
pub fn set_env_var(name: &str, new_value: Option<&str>) {
    with_env_lock(|| match new_value {
        Some(value) => unsafe { env::set_var(name, value) },
        None => unsafe { env::remove_var(name) },
    });
}

/// Set or clear an environment variable while already holding the shared mutex.
///
/// Callers must hold the env mutex via `env_lock_guard` or `with_env_lock`.
pub fn set_env_var_locked(_lock: &MutexGuard<'static, ()>, name: &str, new_value: Option<&str>) {
    match new_value {
        Some(value) => unsafe { env::set_var(name, value) },
        None => unsafe { env::remove_var(name) },
    }
}

/// Serializes access to pg-embed environment variables and restores them on drop.
pub struct EnvVarGuard {
    _lock: Option<MutexGuard<'static, ()>>,
    previous_runtime: Option<OsString>,
    previous_data: Option<OsString>,
    previous_password: Option<OsString>,
}

impl EnvVarGuard {
    fn set_pg_paths_inner(
        lock: Option<MutexGuard<'static, ()>>,
        runtime_dir: &Path,
        data_dir: &Path,
    ) -> Self {
        let previous_runtime = env::var_os("PG_RUNTIME_DIR");
        let previous_data = env::var_os("PG_DATA_DIR");
        let previous_password = env::var_os("PG_PASSWORD");

        reset_data_dir(data_dir);
        ensure_pg_embed_base(runtime_dir, data_dir);
        reset_pgpass(runtime_dir);

        unsafe {
            env::set_var("PG_RUNTIME_DIR", runtime_dir);
            env::set_var("PG_DATA_DIR", data_dir);
            env::set_var("PG_PASSWORD", TEST_PG_PASSWORD);
        }

        Self {
            _lock: lock,
            previous_runtime,
            previous_data,
            previous_password,
        }
    }

    /// Set `PG_RUNTIME_DIR` and `PG_DATA_DIR`, ensuring clean backing paths.
    ///
    /// # Panics
    ///
    /// Panics if the directories cannot be created or if the environment lock is poisoned.
    ///
    /// Coverage for env var restoration and filesystem cleanup lives in
    /// `tests/env_var_guard.rs`.
    #[must_use]
    pub fn set_pg_paths(runtime_dir: &Path, data_dir: &Path) -> Self {
        let lock = lock_env_mutex();
        Self::set_pg_paths_inner(Some(lock), runtime_dir, data_dir)
    }

    /// Set `PG_RUNTIME_DIR` and `PG_DATA_DIR` while the env mutex is already held.
    ///
    /// # Panics
    ///
    /// Panics if the directories cannot be created.
    ///
    /// Callers must hold the env mutex via `env_lock_guard` or `with_env_lock`.
    #[must_use]
    pub fn set_pg_paths_locked(
        _lock: &MutexGuard<'static, ()>,
        runtime_dir: &Path,
        data_dir: &Path,
    ) -> Self {
        Self::set_pg_paths_inner(None, runtime_dir, data_dir)
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        restore_env_var_unlocked("PG_RUNTIME_DIR", self.previous_runtime.as_ref());
        restore_env_var_unlocked("PG_DATA_DIR", self.previous_data.as_ref());
        restore_env_var_unlocked("PG_PASSWORD", self.previous_password.as_ref());
    }
}

#[cfg(test)]
mod with_env_lock_tests {
    use super::{
        EnvVarGuard, env_lock_guard, restore_env_var, restore_env_var_locked, set_env_var,
        set_env_var_locked, with_env_lock,
    };
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn with_env_lock_runs_closure() {
        let value = with_env_lock(|| 42);
        assert_eq!(value, 42);
    }

    #[test]
    fn set_env_var_updates_value() {
        let name = "DIESEL_CTE_EXT_TEST_HELPERS_TMP";
        let previous = env::var_os(name);

        set_env_var(name, Some("value"));
        assert_eq!(
            env::var(name).unwrap_or_else(|err| panic!("{name} should be set: {err}")),
            "value"
        );

        restore_env_var(name, previous.as_ref());
    }

    #[test]
    fn set_env_var_locked_updates_value() {
        let name = "DIESEL_CTE_EXT_TEST_HELPERS_LOCKED_TMP";
        let lock = env_lock_guard();
        let previous = env::var_os(name);

        set_env_var_locked(&lock, name, Some("value"));
        assert_eq!(
            env::var(name).unwrap_or_else(|err| panic!("{name} should be set: {err}")),
            "value"
        );

        restore_env_var_locked(&lock, name, previous.as_ref());
    }

    #[test]
    fn env_var_guard_sets_paths_locked() {
        let lock = env_lock_guard();
        let base = temp_dir("locked");
        let runtime = base.join("runtime");
        let data = base.join("data");

        let guard = EnvVarGuard::set_pg_paths_locked(&lock, &runtime, &data);
        assert_eq!(
            env::var_os("PG_RUNTIME_DIR"),
            Some(runtime.into_os_string())
        );
        assert_eq!(env::var_os("PG_DATA_DIR"), Some(data.into_os_string()));
        drop(guard);
        remove_dir_all(&base);
    }

    #[test]
    fn env_var_guard_sets_paths() {
        let base = temp_dir("unlocked");
        let runtime = base.join("runtime");
        let data = base.join("data");

        let guard = EnvVarGuard::set_pg_paths(&runtime, &data);
        assert_eq!(
            env::var_os("PG_RUNTIME_DIR"),
            Some(runtime.into_os_string())
        );
        assert_eq!(env::var_os("PG_DATA_DIR"), Some(data.into_os_string()));
        drop(guard);
        remove_dir_all(&base);
    }

    fn remove_dir_all(path: &PathBuf) {
        if path.exists() {
            fs::remove_dir_all(path).unwrap_or_else(|err| {
                panic!("remove temp dir {}: {err}", path.display());
            });
        }
    }

    fn temp_dir(suffix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|err| panic!("system time: {err}"))
            .as_nanos();
        let mut path = env::temp_dir();
        path.push(format!(
            "diesel_cte_ext_test_helpers_{}_{}_{}",
            suffix,
            std::process::id(),
            nanos
        ));
        path
    }
}
