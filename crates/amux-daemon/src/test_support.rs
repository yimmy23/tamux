use std::ffi::OsString;
use std::sync::{Mutex, MutexGuard, OnceLock};

fn env_lock_cell() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

pub(crate) fn env_test_lock() -> MutexGuard<'static, ()> {
    env_lock_cell()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub(crate) struct EnvGuard {
    saved: Vec<(&'static str, Option<OsString>)>,
}

impl EnvGuard {
    pub(crate) fn new(keys: &[&'static str]) -> Self {
        Self {
            saved: keys
                .iter()
                .map(|key| (*key, std::env::var_os(key)))
                .collect(),
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, value) in &self.saved {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }
}
