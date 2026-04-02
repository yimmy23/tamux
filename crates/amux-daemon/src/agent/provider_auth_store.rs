use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
use std::path::PathBuf;
#[cfg(test)]
use std::sync::{Mutex, MutexGuard, OnceLock};

const PROVIDER_AUTH_DB_PATH_ENV: &str = "TAMUX_PROVIDER_AUTH_DB_PATH";

#[cfg(test)]
pub(crate) fn provider_auth_test_env_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn provider_auth_db_path() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os(PROVIDER_AUTH_DB_PATH_ENV) {
        return Ok(PathBuf::from(path));
    }
    Ok(amux_protocol::ensure_amux_data_dir()?
        .join("history")
        .join("command-history.db"))
}

fn ensure_provider_auth_schema(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS provider_auth_state (
            provider_id TEXT NOT NULL,
            auth_mode   TEXT NOT NULL,
            state_json  TEXT NOT NULL,
            updated_at  INTEGER NOT NULL,
            PRIMARY KEY (provider_id, auth_mode)
        );
        CREATE INDEX IF NOT EXISTS idx_provider_auth_state_updated
        ON provider_auth_state(updated_at DESC);
        ",
    )?;
    Ok(())
}

fn open_provider_auth_db() -> Result<Connection> {
    let db_path = provider_auth_db_path()?;
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!("failed to create auth db directory '{}'", parent.display())
        })?;
    }
    let conn = Connection::open(&db_path)
        .with_context(|| format!("failed to open auth db '{}'", db_path.display()))?;
    ensure_provider_auth_schema(&conn)?;
    Ok(conn)
}

pub(crate) fn load_provider_auth_state(
    provider_id: &str,
    auth_mode: &str,
) -> Result<Option<Value>> {
    let conn = open_provider_auth_db()?;
    let row = conn
        .query_row(
            "SELECT state_json FROM provider_auth_state WHERE provider_id = ?1 AND auth_mode = ?2",
            params![provider_id, auth_mode],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    row.map(|raw| {
        serde_json::from_str::<Value>(&raw).context("failed to parse provider auth state")
    })
    .transpose()
}

pub(crate) fn save_provider_auth_state(
    provider_id: &str,
    auth_mode: &str,
    state: &Value,
) -> Result<()> {
    let conn = open_provider_auth_db()?;
    conn.execute(
        "INSERT OR REPLACE INTO provider_auth_state (provider_id, auth_mode, state_json, updated_at)
         VALUES (?1, ?2, ?3, ?4)",
        params![
            provider_id,
            auth_mode,
            serde_json::to_string(state)?,
            crate::history::now_ts() as i64
        ],
    )?;
    Ok(())
}

pub(crate) fn delete_provider_auth_state(provider_id: &str, auth_mode: &str) -> Result<()> {
    let conn = open_provider_auth_db()?;
    conn.execute(
        "DELETE FROM provider_auth_state WHERE provider_id = ?1 AND auth_mode = ?2",
        params![provider_id, auth_mode],
    )?;
    Ok(())
}
