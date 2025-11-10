//! Shared helpers for integration tests.

use std::{
    env,
    ffi::OsString,
    fs,
    path::Path,
    sync::{Mutex, MutexGuard, OnceLock},
};

fn env_mutex() -> &'static Mutex<()> {
    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    ENV_LOCK.get_or_init(|| Mutex::new(()))
}

/// Serialises access to pg-embed environment variables and restores them on drop.
pub struct EnvVarGuard {
    _lock: MutexGuard<'static, ()>,
    previous_runtime: Option<OsString>,
    previous_data: Option<OsString>,
}

impl EnvVarGuard {
    /// Set `PG_RUNTIME_DIR` and `PG_DATA_DIR`, creating the backing directories first.
    ///
    /// # Panics
    ///
    /// Panics if the directories cannot be created or if the environment lock is poisoned.
    #[must_use]
    pub fn set_pg_paths(runtime_dir: &Path, data_dir: &Path) -> Self {
        fs::create_dir_all(runtime_dir).unwrap_or_else(|err| panic!("runtime directory: {err}"));
        fs::create_dir_all(data_dir).unwrap_or_else(|err| panic!("data directory: {err}"));

        let lock = env_mutex()
            .lock()
            .unwrap_or_else(|err| panic!("env mutex poisoned: {err}"));
        let previous_runtime = env::var_os("PG_RUNTIME_DIR");
        let previous_data = env::var_os("PG_DATA_DIR");

        unsafe {
            env::set_var("PG_RUNTIME_DIR", runtime_dir);
            env::set_var("PG_DATA_DIR", data_dir);
        }

        Self {
            _lock: lock,
            previous_runtime,
            previous_data,
        }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match self.previous_runtime.as_ref() {
            Some(value) => unsafe { env::set_var("PG_RUNTIME_DIR", value) },
            None => unsafe { env::remove_var("PG_RUNTIME_DIR") },
        }

        match self.previous_data.as_ref() {
            Some(value) => unsafe { env::set_var("PG_DATA_DIR", value) },
            None => unsafe { env::remove_var("PG_DATA_DIR") },
        }
    }
}
