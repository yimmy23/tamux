use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::path::PathBuf;
#[cfg(test)]
use std::sync::{Mutex, OnceLock};

#[cfg(test)]
use uuid::Uuid;

const PROVIDER_AUTH_DB_PATH_ENV: &str = "ZORAI_PROVIDER_AUTH_DB_PATH";
const GITHUB_CLI_PATH_ENV: &str = "ZORAI_GH_CLI_PATH";

#[cfg(test)]
pub(crate) fn auth_test_env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredGithubCopilotAuth {
    auth_mode: String,
    access_token: String,
    source: String,
    updated_at: i64,
    created_at: i64,
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn provider_auth_db_path() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os(PROVIDER_AUTH_DB_PATH_ENV) {
        return Ok(PathBuf::from(path));
    }
    Ok(zorai_protocol::ensure_zorai_data_dir()?
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
            deleted_at  INTEGER,
            PRIMARY KEY (provider_id, auth_mode)
        );
        CREATE INDEX IF NOT EXISTS idx_provider_auth_state_updated
        ON provider_auth_state(deleted_at, updated_at DESC);
        ",
    )?;
    conn.execute(
        "ALTER TABLE provider_auth_state ADD COLUMN deleted_at INTEGER",
        [],
    )
    .ok();
    Ok(())
}

fn open_provider_auth_db() -> Result<Connection> {
    let db_path = provider_auth_db_path()?;
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(&db_path)
        .with_context(|| format!("failed to open auth db '{}'", db_path.display()))?;
    ensure_provider_auth_schema(&conn)?;

    Ok(conn)
}

pub fn clear_github_copilot_auth() -> Result<()> {
    let conn = open_provider_auth_db()?;
    conn.execute(
        "UPDATE provider_auth_state SET deleted_at = ?3 WHERE provider_id = ?1 AND auth_mode = ?2 AND deleted_at IS NULL",
        params![
            zorai_shared::providers::PROVIDER_ID_GITHUB_COPILOT,
            "github_copilot",
            now_millis()
        ],
    )?;
    Ok(())
}

fn read_stored_github_copilot_auth() -> Option<StoredGithubCopilotAuth> {
    let conn = open_provider_auth_db().ok()?;
    let raw = conn
        .query_row(
            "SELECT state_json FROM provider_auth_state WHERE provider_id = ?1 AND auth_mode = ?2 AND deleted_at IS NULL",
            params![
                zorai_shared::providers::PROVIDER_ID_GITHUB_COPILOT,
                "github_copilot"
            ],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .ok()??;
    serde_json::from_str(&raw).ok()
}

fn write_stored_github_copilot_auth(auth: &StoredGithubCopilotAuth) -> Result<()> {
    let conn = open_provider_auth_db()?;
    conn.execute(
        "INSERT OR REPLACE INTO provider_auth_state (provider_id, auth_mode, state_json, updated_at, deleted_at)
         VALUES (?1, ?2, ?3, ?4, NULL)",
        params![
            zorai_shared::providers::PROVIDER_ID_GITHUB_COPILOT,
            "github_copilot",
            serde_json::to_string(auth)?,
            now_millis(),
        ],
    )?;
    Ok(())
}

fn github_cli_program() -> OsString {
    std::env::var_os(GITHUB_CLI_PATH_ENV).unwrap_or_else(|| OsString::from("gh"))
}

fn gh_cli_token_quiet() -> Option<String> {
    let output = std::process::Command::new(github_cli_program())
        .args(["auth", "token"])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let token = String::from_utf8(output.stdout).ok()?.trim().to_string();
    if token.is_empty() {
        None
    } else {
        Some(token)
    }
}

fn store_github_copilot_token(token: String, source: &str) -> Result<()> {
    let now = now_millis();
    write_stored_github_copilot_auth(&StoredGithubCopilotAuth {
        auth_mode: "github_copilot".to_string(),
        access_token: token,
        source: source.to_string(),
        updated_at: now,
        created_at: now,
    })
}

#[cfg(target_os = "windows")]
pub fn open_external_url(url: &str) -> Result<()> {
    std::process::Command::new("cmd")
        .args(["/C", "start", "", url])
        .spawn()
        .context("failed to open browser")?;
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn open_external_url(url: &str) -> Result<()> {
    std::process::Command::new("open")
        .arg(url)
        .spawn()
        .context("failed to open browser")?;
    Ok(())
}

#[cfg(all(unix, not(target_os = "macos")))]
pub fn open_external_url(url: &str) -> Result<()> {
    // Redirect stdout/stderr to /dev/null so browser process output
    // (DBus errors, Chromium warnings) doesn't corrupt the TUI alternate screen.
    std::process::Command::new("xdg-open")
        .arg(url)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .stdin(std::process::Stdio::null())
        .spawn()
        .context("failed to open browser")?;
    Ok(())
}

#[cfg(not(any(target_os = "windows", target_os = "macos", unix)))]
pub fn open_external_url(_url: &str) -> Result<()> {
    Err(anyhow::anyhow!(
        "opening URLs is not supported on this platform"
    ))
}

#[derive(Debug)]
pub enum GithubCopilotAuthFlowResult {
    AlreadyAvailable,
    ImportedFromGhCli,
    Started,
}

pub fn begin_github_copilot_auth_flow() -> Result<GithubCopilotAuthFlowResult> {
    if read_stored_github_copilot_auth().is_some() {
        return Ok(GithubCopilotAuthFlowResult::AlreadyAvailable);
    }

    if let Some(token) = gh_cli_token_quiet() {
        store_github_copilot_token(token, "gh_cli_import")?;
        return Ok(GithubCopilotAuthFlowResult::ImportedFromGhCli);
    }

    std::process::Command::new(github_cli_program())
        .args([
            "auth",
            "login",
            "--web",
            "--hostname",
            "github.com",
            "--git-protocol",
            "ssh",
            "--skip-ssh-key",
            "--scopes",
            "read:org,models:read",
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .context("failed to start GitHub CLI login flow")?;

    Ok(GithubCopilotAuthFlowResult::Started)
}

#[cfg(test)]
#[path = "auth/tests.rs"]
mod tests;
