use anyhow::{Context, Result};
use base64::Engine;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::Duration;
use url::Url;
use uuid::Uuid;

const OPENAI_CODEX_AUTH_CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const OPENAI_CODEX_AUTH_AUTHORIZE_URL: &str = "https://auth.openai.com/oauth/authorize";
const OPENAI_CODEX_AUTH_TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
const OPENAI_CODEX_AUTH_REDIRECT_URI: &str = "http://localhost:1455/auth/callback";
const OPENAI_CODEX_AUTH_SCOPE: &str = "openid profile email offline_access";
const PROVIDER_AUTH_DB_PATH_ENV: &str = "TAMUX_PROVIDER_AUTH_DB_PATH";
const GITHUB_CLI_PATH_ENV: &str = "TAMUX_GH_CLI_PATH";

static AUTH_FLOW_ACTIVE: OnceLock<Mutex<bool>> = OnceLock::new();

#[path = "auth/openai_codex_flow.rs"]
mod openai_codex_flow;

#[cfg(test)]
pub(crate) fn auth_test_env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[derive(Debug)]
pub enum OpenAICodexAuthFlowResult {
    AlreadyAvailable,
    ImportedFromCodexCli,
    Started { url: String },
}

#[derive(Debug, Clone, Default)]
pub struct OpenAICodexAuthStatus {
    pub available: bool,
    pub source: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredOpenAICodexAuth {
    provider: String,
    auth_mode: String,
    access_token: String,
    refresh_token: String,
    account_id: String,
    expires_at: i64,
    source: String,
    updated_at: i64,
    created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredGithubCopilotAuth {
    auth_mode: String,
    access_token: String,
    source: String,
    updated_at: i64,
    created_at: i64,
}

#[derive(Debug, Deserialize)]
struct CodexCliAuthFile {
    tokens: Option<CodexCliTokens>,
}

#[derive(Debug, Deserialize)]
struct CodexCliTokens {
    access_token: Option<String>,
    refresh_token: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: String,
    expires_in: i64,
}

fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as i64)
        .unwrap_or(0)
}

fn auth_flag() -> &'static Mutex<bool> {
    AUTH_FLOW_ACTIVE.get_or_init(|| Mutex::new(false))
}

fn provider_auth_db_path() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os(PROVIDER_AUTH_DB_PATH_ENV) {
        return Ok(PathBuf::from(path));
    }
    Ok(amux_protocol::ensure_amux_data_dir()?
        .join("history")
        .join("command-history.db"))
}

fn codex_cli_auth_path() -> Option<std::path::PathBuf> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .ok()?;
    Some(
        std::path::PathBuf::from(home)
            .join(".codex")
            .join("auth.json"),
    )
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
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(&db_path)
        .with_context(|| format!("failed to open auth db '{}'", db_path.display()))?;
    ensure_provider_auth_schema(&conn)?;
    Ok(conn)
}

fn read_stored_openai_codex_auth() -> Option<StoredOpenAICodexAuth> {
    let conn = open_provider_auth_db().ok()?;
    let raw = conn
        .query_row(
            "SELECT state_json FROM provider_auth_state WHERE provider_id = ?1 AND auth_mode = ?2",
            params!["openai", "chatgpt_subscription"],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .ok()??;
    serde_json::from_str(&raw).ok()
}

pub fn openai_codex_auth_status() -> OpenAICodexAuthStatus {
    if let Some(auth) = read_stored_openai_codex_auth() {
        return OpenAICodexAuthStatus {
            available: true,
            source: Some(auth.source),
        };
    }

    match import_codex_cli_auth_if_present() {
        Ok(true) => read_stored_openai_codex_auth()
            .map(|auth| OpenAICodexAuthStatus {
                available: true,
                source: Some(auth.source),
            })
            .unwrap_or_default(),
        _ => OpenAICodexAuthStatus::default(),
    }
}

fn write_stored_openai_codex_auth(auth: &StoredOpenAICodexAuth) -> Result<()> {
    let conn = open_provider_auth_db()?;
    conn.execute(
        "INSERT OR REPLACE INTO provider_auth_state (provider_id, auth_mode, state_json, updated_at)
         VALUES (?1, ?2, ?3, ?4)",
        params![
            "openai",
            "chatgpt_subscription",
            serde_json::to_string(auth)?,
            now_millis(),
        ],
    )?;
    Ok(())
}

pub fn clear_openai_codex_auth() -> Result<()> {
    let conn = open_provider_auth_db()?;
    conn.execute(
        "DELETE FROM provider_auth_state WHERE provider_id = ?1 AND auth_mode = ?2",
        params!["openai", "chatgpt_subscription"],
    )?;
    Ok(())
}

pub fn clear_github_copilot_auth() -> Result<()> {
    let conn = open_provider_auth_db()?;
    conn.execute(
        "DELETE FROM provider_auth_state WHERE provider_id = ?1 AND auth_mode = ?2",
        params!["github-copilot", "github_copilot"],
    )?;
    Ok(())
}

fn read_stored_github_copilot_auth() -> Option<StoredGithubCopilotAuth> {
    let conn = open_provider_auth_db().ok()?;
    let raw = conn
        .query_row(
            "SELECT state_json FROM provider_auth_state WHERE provider_id = ?1 AND auth_mode = ?2",
            params!["github-copilot", "github_copilot"],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .ok()??;
    serde_json::from_str(&raw).ok()
}

fn write_stored_github_copilot_auth(auth: &StoredGithubCopilotAuth) -> Result<()> {
    let conn = open_provider_auth_db()?;
    conn.execute(
        "INSERT OR REPLACE INTO provider_auth_state (provider_id, auth_mode, state_json, updated_at)
         VALUES (?1, ?2, ?3, ?4)",
        params![
            "github-copilot",
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

fn decode_jwt_payload(access_token: &str) -> Option<serde_json::Value> {
    let payload = access_token.split('.').nth(1)?;
    let decoded = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    serde_json::from_slice::<serde_json::Value>(&decoded).ok()
}

fn extract_openai_codex_account_id(access_token: &str) -> Option<String> {
    decode_jwt_payload(access_token)?
        .get("https://api.openai.com/auth")
        .and_then(|value| value.get("chatgpt_account_id"))
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned)
}

fn extract_jwt_expiry(access_token: &str) -> Option<i64> {
    decode_jwt_payload(access_token)?
        .get("exp")
        .and_then(|value| value.as_i64())
        .map(|seconds| seconds.saturating_mul(1000))
}

fn import_codex_cli_auth_if_present() -> Result<bool> {
    if read_stored_openai_codex_auth().is_some() {
        return Ok(false);
    }

    let path = match codex_cli_auth_path() {
        Some(path) => path,
        None => return Ok(false),
    };
    let raw = match std::fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(_) => return Ok(false),
    };
    let parsed: CodexCliAuthFile = serde_json::from_str(&raw)?;
    let tokens = match parsed.tokens {
        Some(tokens) => tokens,
        None => return Ok(false),
    };
    let access_token = match tokens.access_token {
        Some(token) if !token.trim().is_empty() => token,
        _ => return Ok(false),
    };
    let refresh_token = match tokens.refresh_token {
        Some(token) if !token.trim().is_empty() => token,
        _ => return Ok(false),
    };
    let account_id = match extract_openai_codex_account_id(&access_token) {
        Some(account_id) => account_id,
        None => return Ok(false),
    };
    let expires_at = match extract_jwt_expiry(&access_token) {
        Some(expires_at) => expires_at,
        None => return Ok(false),
    };
    let now = now_millis();
    write_stored_openai_codex_auth(&StoredOpenAICodexAuth {
        provider: "openai-codex".to_string(),
        auth_mode: "chatgpt_subscription".to_string(),
        access_token,
        refresh_token,
        account_id,
        expires_at,
        source: "codex_import".to_string(),
        updated_at: now,
        created_at: now,
    })?;
    Ok(true)
}

pub fn begin_openai_codex_auth_flow() -> Result<OpenAICodexAuthFlowResult> {
    if read_stored_openai_codex_auth().is_some() {
        return Ok(OpenAICodexAuthFlowResult::AlreadyAvailable);
    }
    if import_codex_cli_auth_if_present()? {
        return Ok(OpenAICodexAuthFlowResult::ImportedFromCodexCli);
    }

    let mut guard = auth_flag()
        .lock()
        .map_err(|_| anyhow::anyhow!("failed to acquire auth flow lock"))?;
    if *guard {
        anyhow::bail!("an OpenAI auth flow is already running");
    }
    *guard = true;
    drop(guard);

    let (verifier, challenge) = openai_codex_flow::generate_pkce_pair();
    let state = Uuid::new_v4().simple().to_string();
    let mut auth_url = Url::parse(OPENAI_CODEX_AUTH_AUTHORIZE_URL)?;
    auth_url
        .query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", OPENAI_CODEX_AUTH_CLIENT_ID)
        .append_pair("redirect_uri", OPENAI_CODEX_AUTH_REDIRECT_URI)
        .append_pair("scope", OPENAI_CODEX_AUTH_SCOPE)
        .append_pair("code_challenge", &challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", &state)
        .append_pair("id_token_add_organizations", "true")
        .append_pair("codex_cli_simplified_flow", "true")
        .append_pair("originator", "tamux");
    let url = auth_url.to_string();

    std::thread::spawn(move || {
        let result = openai_codex_flow::complete_browser_auth(state, verifier);
        if let Err(error) = result {
            tracing::warn!("OpenAI auth flow failed: {error}");
        }
        if let Ok(mut active) = auth_flag().lock() {
            *active = false;
        }
    });

    Ok(OpenAICodexAuthFlowResult::Started { url })
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
