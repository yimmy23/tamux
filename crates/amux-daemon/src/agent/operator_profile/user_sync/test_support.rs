use super::*;

fn test_guard() -> &'static std::sync::Mutex<()> {
    static GUARD: std::sync::OnceLock<std::sync::Mutex<()>> = std::sync::OnceLock::new();
    GUARD.get_or_init(|| std::sync::Mutex::new(()))
}

pub(in crate::agent) fn acquire_user_sync_test_guard() -> std::sync::MutexGuard<'static, ()> {
    match test_guard().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

pub(in crate::agent) fn set_user_sync_state_for_test(state: UserProfileSyncState) {
    *sync_state_guard()
        .lock()
        .expect("user profile sync state mutex poisoned") = state;
}
