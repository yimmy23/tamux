use std::ffi::{OsStr, OsString};
use std::sync::MutexGuard;

pub(crate) const TAMUX_DATA_DIR_ENV: &str = "TAMUX_DATA_DIR";

pub(crate) fn env_var_lock() -> MutexGuard<'static, ()> {
    crate::auth::auth_test_env_lock()
        .lock()
        .expect("env var lock")
}

pub(crate) struct EnvVarGuard {
    key: &'static str,
    original: Option<OsString>,
}

impl EnvVarGuard {
    pub(crate) fn set(key: &'static str, value: impl AsRef<OsStr>) -> Self {
        let original = std::env::var_os(key);
        unsafe {
            std::env::set_var(key, value);
        }
        Self { key, original }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match self.original.take() {
            Some(value) => unsafe {
                std::env::set_var(self.key, value);
            },
            None => unsafe {
                std::env::remove_var(self.key);
            },
        }
    }
}
