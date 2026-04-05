use super::*;

include!("impl_part1.rs");
include!("impl_part2.rs");
include!("impl_part3.rs");
include!("impl_part4.rs");
include!("impl_part5.rs");
include!("impl_part6.rs");

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::{params, Connection};
    use std::ffi::OsString;
    use std::path::PathBuf;
    use tokio::sync::mpsc::unbounded_channel;

    fn make_model() -> (
        TuiModel,
        tokio::sync::mpsc::UnboundedReceiver<DaemonCommand>,
    ) {
        let (_event_tx, event_rx) = std::sync::mpsc::channel();
        let (daemon_tx, daemon_rx) = unbounded_channel();
        (TuiModel::new(event_rx, daemon_tx), daemon_rx)
    }

    fn auth_env_lock() -> std::sync::MutexGuard<'static, ()> {
        crate::auth::auth_test_env_lock().lock().unwrap()
    }

    struct EnvGuard {
        saved: Vec<(&'static str, Option<OsString>)>,
    }

    impl EnvGuard {
        fn new(keys: &[&'static str]) -> Self {
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

    fn init_provider_auth_db(path: &std::path::Path) {
        let conn = Connection::open(path).expect("open auth db");
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS provider_auth_state (
                provider_id TEXT NOT NULL,
                auth_mode   TEXT NOT NULL,
                state_json  TEXT NOT NULL,
                updated_at  INTEGER NOT NULL,
                PRIMARY KEY (provider_id, auth_mode)
            );
            ",
        )
        .expect("create auth schema");
    }

    fn write_provider_auth_row(path: &std::path::Path, provider_id: &str, auth_mode: &str) {
        init_provider_auth_db(path);
        let conn = Connection::open(path).expect("open auth db");
        conn.execute(
            "INSERT OR REPLACE INTO provider_auth_state (provider_id, auth_mode, state_json, updated_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![
                provider_id,
                auth_mode,
                "{\"token\":\"test\"}",
                1_i64
            ],
        )
        .expect("insert auth row");
    }

    fn has_provider_auth_row(path: &std::path::Path, provider_id: &str, auth_mode: &str) -> bool {
        init_provider_auth_db(path);
        let conn = Connection::open(path).expect("open auth db");
        conn.query_row(
            "SELECT 1 FROM provider_auth_state WHERE provider_id = ?1 AND auth_mode = ?2",
            params![provider_id, auth_mode],
            |_row| Ok(()),
        )
        .is_ok()
    }

    fn unique_test_db_path(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        std::env::temp_dir().join(format!("tamux-{name}-{nanos}.sqlite"))
    }

    include!("tests/tests_part1.rs");
    include!("tests/tests_part2.rs");
    include!("tests/tests_part3.rs");
}
