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

const TEST_PG_PASSWORD: &str = "postgres_pass";

fn env_mutex() -> &'static Mutex<()> {
    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    ENV_LOCK.get_or_init(|| Mutex::new(()))
}

fn reset_data_dir(data_dir: &Path) {
    if data_dir.exists() {
        fs::remove_dir_all(data_dir).unwrap_or_else(|err| panic!("data directory cleanup: {err}"));
    }
}

fn reset_pgpass(runtime_dir: &Path) {
    let pgpass = runtime_dir.join(".pgpass");
    match fs::remove_file(&pgpass) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => panic!("pgpass cleanup: {err}"),
    }
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
        let mut perms = fs::metadata(&base_dir)
            .unwrap_or_else(|err| panic!("base directory metadata: {err}"))
            .permissions();
        perms.set_mode(0o777);
        fs::set_permissions(&base_dir, perms)
            .unwrap_or_else(|err| panic!("base directory permissions: {err}"));
    }
}

/// Serialises access to pg-embed environment variables and restores them on drop.
pub struct EnvVarGuard {
    _lock: MutexGuard<'static, ()>,
    previous_runtime: Option<OsString>,
    previous_data: Option<OsString>,
    previous_password: Option<OsString>,
}

impl EnvVarGuard {
    /// Set `PG_RUNTIME_DIR` and `PG_DATA_DIR`, creating the backing directories first.
    ///
    /// # Panics
    ///
    /// Panics if the directories cannot be created or if the environment lock is poisoned.
    #[must_use]
    pub fn set_pg_paths(runtime_dir: &Path, data_dir: &Path) -> Self {
        reset_data_dir(data_dir);
        ensure_pg_embed_base(runtime_dir, data_dir);
        reset_pgpass(runtime_dir);

        let lock = env_mutex()
            .lock()
            .unwrap_or_else(|err| panic!("env mutex poisoned: {err}"));
        let previous_runtime = env::var_os("PG_RUNTIME_DIR");
        let previous_data = env::var_os("PG_DATA_DIR");
        let previous_password = env::var_os("PG_PASSWORD");

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

        match self.previous_password.as_ref() {
            Some(value) => unsafe { env::set_var("PG_PASSWORD", value) },
            None => unsafe { env::remove_var("PG_PASSWORD") },
        }
    }
}
