use anyhow::{Context, Result};
use base64::Engine;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;
use url::Url;
use uuid::Uuid;

use super::now_millis;
use super::storage::persist_stored_openai_codex_auth;
use super::{
    auth_runtime, error_status, exchange_client, metadata_from_auth, openai_codex_auth_status,
    pending_status, sanitized_auth_failure_message, OpenAICodexAuthStatus, OpenAICodexExchange,
    OpenAICodexTokenResponse, PendingOpenAICodexAuth, StoredOpenAICodexAuth, OPENAI_AUTH_MODE,
    OPENAI_CODEX_AUTH_AUTHORIZE_URL, OPENAI_CODEX_AUTH_CLIENT_ID, OPENAI_CODEX_AUTH_PROVIDER,
    OPENAI_CODEX_AUTH_REDIRECT_URI, OPENAI_CODEX_AUTH_SCOPE, OPENAI_CODEX_AUTH_TOKEN_URL,
};

const OPENAI_CODEX_CALLBACK_TIMEOUT: Duration = Duration::from_secs(60);
const OPENAI_CODEX_CALLBACK_POLL_INTERVAL: Duration = Duration::from_millis(10);

pub(super) fn build_pending_auth_flow() -> Result<PendingOpenAICodexAuth> {
    let (verifier, challenge) = generate_pkce_pair();
    let state = Uuid::new_v4().simple().to_string();
    let auth_url = build_auth_url(&state, &challenge)?;
    Ok(PendingOpenAICodexAuth {
        flow_id: Uuid::new_v4().simple().to_string(),
        auth_url,
        verifier,
        state,
        completion_started: false,
    })
}

fn generate_pkce_pair() -> (String, String) {
    let verifier = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(Sha256::digest(verifier.as_bytes()));
    (verifier, challenge)
}

fn build_auth_url(state: &str, challenge: &str) -> Result<String> {
    let mut auth_url = Url::parse(OPENAI_CODEX_AUTH_AUTHORIZE_URL)?;
    auth_url
        .query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", OPENAI_CODEX_AUTH_CLIENT_ID)
        .append_pair("redirect_uri", OPENAI_CODEX_AUTH_REDIRECT_URI)
        .append_pair("scope", OPENAI_CODEX_AUTH_SCOPE)
        .append_pair("code_challenge", challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", state)
        .append_pair("id_token_add_organizations", "true")
        .append_pair("codex_cli_simplified_flow", "true")
        .append_pair("originator", "tamux");
    Ok(auth_url.to_string())
}

pub(super) fn exchange_authorization_code(
    code: &str,
    verifier: &str,
) -> Result<StoredOpenAICodexAuth> {
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .build();
    let agent: ureq::Agent = config.into();
    let mut response = agent
        .post(OPENAI_CODEX_AUTH_TOKEN_URL)
        .content_type("application/x-www-form-urlencoded")
        .send_form([
            ("grant_type", "authorization_code"),
            ("client_id", OPENAI_CODEX_AUTH_CLIENT_ID),
            ("code", code),
            ("code_verifier", verifier),
            ("redirect_uri", OPENAI_CODEX_AUTH_REDIRECT_URI),
        ])?;

    let status = response.status();
    if !status.is_success() {
        let text = response.body_mut().read_to_string().unwrap_or_default();
        anyhow::bail!("OpenAI OAuth exchange failed: HTTP {status} {text}");
    }

    let payload: OpenAICodexTokenResponse = response.body_mut().read_json()?;
    stored_auth_from_token_response(payload)
}

fn stored_auth_from_token_response(
    payload: OpenAICodexTokenResponse,
) -> Result<StoredOpenAICodexAuth> {
    let account_id = super::extract_openai_codex_account_id(&payload.access_token)
        .context("OpenAI OAuth exchange returned no ChatGPT account id")?;
    let now = now_millis() as i64;
    Ok(StoredOpenAICodexAuth {
        provider: Some(OPENAI_CODEX_AUTH_PROVIDER.to_string()),
        auth_mode: Some(OPENAI_AUTH_MODE.to_string()),
        access_token: payload.access_token,
        refresh_token: payload.refresh_token,
        account_id: Some(account_id),
        expires_at: Some(now.saturating_add(payload.expires_in.saturating_mul(1000))),
        source: Some("tamux".to_string()),
        updated_at: Some(now),
        created_at: Some(now),
    })
}

fn set_runtime_error(message: String) -> OpenAICodexAuthStatus {
    let sanitized = sanitized_auth_failure_message(&message);
    let mut runtime = auth_runtime()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    runtime.last_error = Some(sanitized.clone());
    error_status(sanitized)
}

fn clear_runtime_error() {
    let mut runtime = auth_runtime()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    runtime.last_error = None;
}

fn complete_pending_flow_with_result(
    result: Result<StoredOpenAICodexAuth>,
    flow_id: &str,
) -> OpenAICodexAuthStatus {
    let stale_or_canceled = {
        let mut runtime = auth_runtime()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        match runtime.pending.as_ref() {
            None => true,
            Some(current) if current.flow_id != flow_id => return pending_status(current),
            Some(_) => {
                runtime.pending = None;
                false
            }
        }
    };

    if stale_or_canceled {
        return openai_codex_auth_status(false);
    }

    match result {
        Ok(auth) => match persist_stored_openai_codex_auth(&auth) {
            Ok(persisted) => {
                clear_runtime_error();
                metadata_from_auth(&persisted)
            }
            Err(error) => {
                set_runtime_error(format!("failed to persist OpenAI Codex auth: {error}"))
            }
        },
        Err(error) => set_runtime_error(error.to_string()),
    }
}

fn read_callback_request(stream: &mut TcpStream) -> Result<(String, String)> {
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut buffer = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];

    loop {
        let read = stream.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.windows(4).any(|window| window == b"\r\n\r\n") || buffer.len() >= 8192 {
            break;
        }
    }

    let request = String::from_utf8_lossy(&buffer);
    let target = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .context("received malformed OAuth callback request")?;
    let url = Url::parse(&format!("http://127.0.0.1{target}"))?;
    let callback_state = url
        .query_pairs()
        .find(|(key, _)| key == "state")
        .map(|(_, value)| value.to_string())
        .unwrap_or_default();
    let code = url
        .query_pairs()
        .find(|(key, _)| key == "code")
        .map(|(_, value)| value.to_string())
        .unwrap_or_default();
    Ok((callback_state, code))
}

fn write_callback_failure(stream: &mut TcpStream) {
    let _ = stream.write_all(
        b"HTTP/1.1 400 Bad Request\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\nInvalid OpenAI OAuth callback.",
    );
}

fn write_callback_success(stream: &mut TcpStream) {
    let _ = stream.write_all(
        b"HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n<!doctype html><html><body><p>Authentication successful. Return to tamux.</p></body></html>",
    );
}

fn pending_flow_matches(flow_id: &str) -> bool {
    let runtime = auth_runtime()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    matches!(runtime.pending.as_ref(), Some(current) if current.flow_id == flow_id)
}

fn bind_callback_listener(flow_id: &str, timeout: Duration) -> Result<TcpListener> {
    let deadline = std::time::Instant::now() + timeout;

    loop {
        if !pending_flow_matches(flow_id) {
            anyhow::bail!("OpenAI OAuth callback canceled");
        }

        match TcpListener::bind("127.0.0.1:1455") {
            Ok(listener) => return Ok(listener),
            Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
                if std::time::Instant::now() >= deadline {
                    return Err(error)
                        .context("failed to bind localhost callback listener on port 1455");
                }
                std::thread::sleep(OPENAI_CODEX_CALLBACK_POLL_INTERVAL);
            }
            Err(error) => {
                return Err(error)
                    .context("failed to bind localhost callback listener on port 1455")
            }
        }
    }
}

fn await_browser_callback(
    listener: &TcpListener,
    timeout: Duration,
    flow_id: &str,
) -> Result<TcpStream> {
    listener
        .set_nonblocking(true)
        .context("failed to configure callback listener")?;
    let deadline = std::time::Instant::now() + timeout;

    loop {
        if !pending_flow_matches(flow_id) {
            anyhow::bail!("OpenAI OAuth callback canceled");
        }

        match listener.accept() {
            Ok((stream, _)) => return Ok(stream),
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if std::time::Instant::now() >= deadline {
                    anyhow::bail!("OpenAI OAuth callback timed out");
                }
                std::thread::sleep(OPENAI_CODEX_CALLBACK_POLL_INTERVAL);
            }
            Err(error) => return Err(error).context("failed to accept OpenAI OAuth callback"),
        }
    }
}

fn complete_browser_auth_with_timeout_sync(
    exchange: &dyn OpenAICodexExchange,
    timeout: Duration,
    ready_signal: Option<std::sync::mpsc::Sender<()>>,
) -> OpenAICodexAuthStatus {
    let pending = {
        let runtime = auth_runtime()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        runtime.pending.clone()
    };
    let Some(pending) = pending else {
        return openai_codex_auth_status(false);
    };

    let result = (|| -> Result<StoredOpenAICodexAuth> {
        let listener = bind_callback_listener(&pending.flow_id, timeout)?;
        if let Some(signal) = ready_signal {
            let _ = signal.send(());
        }
        let mut stream = await_browser_callback(&listener, timeout, &pending.flow_id)?;
        let (callback_state, code) = read_callback_request(&mut stream)?;

        if callback_state != pending.state || code.is_empty() {
            write_callback_failure(&mut stream);
            anyhow::bail!("invalid OpenAI OAuth callback");
        }

        write_callback_success(&mut stream);
        exchange.exchange_authorization_code(&code, &pending.verifier)
    })();

    complete_pending_flow_with_result(result, &pending.flow_id)
}

fn complete_browser_auth_with_timeout(
    exchange: &'static dyn OpenAICodexExchange,
    timeout: Duration,
) -> OpenAICodexAuthStatus {
    let run_blocking = move || complete_browser_auth_with_timeout_sync(exchange, timeout, None);

    match tokio::runtime::Handle::try_current() {
        Ok(handle) if handle.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread => {
            tokio::task::block_in_place(|| {
                handle
                    .block_on(tokio::task::spawn_blocking(run_blocking))
                    .unwrap_or_else(|error| {
                        set_runtime_error(format!("OpenAI authentication failed: {error}"))
                    })
            })
        }
        Ok(_) => std::thread::spawn(run_blocking)
            .join()
            .unwrap_or_else(|_| set_runtime_error("OpenAI authentication failed".to_string())),
        Err(_) => run_blocking(),
    }
}

pub(crate) fn complete_browser_auth() -> OpenAICodexAuthStatus {
    complete_browser_auth_with_timeout(exchange_client(), OPENAI_CODEX_CALLBACK_TIMEOUT)
}

#[cfg(test)]
pub(crate) fn complete_browser_auth_with(
    exchange: &dyn OpenAICodexExchange,
) -> OpenAICodexAuthStatus {
    complete_browser_auth_with_timeout_sync(exchange, OPENAI_CODEX_CALLBACK_TIMEOUT, None)
}

#[cfg(test)]
pub(crate) fn current_pending_openai_codex_flow_id_for_tests() -> Option<String> {
    let runtime = auth_runtime()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    runtime
        .pending
        .as_ref()
        .map(|pending| pending.flow_id.clone())
}

#[cfg(test)]
pub(crate) fn mark_openai_codex_auth_timeout_for_tests() -> OpenAICodexAuthStatus {
    let flow_id = current_pending_openai_codex_flow_id_for_tests();
    match flow_id {
        Some(flow_id) => complete_pending_flow_with_result(
            Err(anyhow::anyhow!("OpenAI OAuth callback timed out")),
            &flow_id,
        ),
        None => openai_codex_auth_status(false),
    }
}

#[cfg(test)]
pub(crate) fn complete_openai_codex_auth_with_code_for_tests(
    code: &str,
    exchange: &dyn OpenAICodexExchange,
) -> OpenAICodexAuthStatus {
    let pending = {
        let runtime = auth_runtime()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        runtime.pending.clone()
    };
    let Some(pending) = pending else {
        return openai_codex_auth_status(false);
    };
    let result = exchange.exchange_authorization_code(code, &pending.verifier);
    complete_pending_flow_with_result(result, &pending.flow_id)
}

#[cfg(test)]
pub(crate) fn complete_openai_codex_auth_flow_with_result_for_tests(
    flow_id: &str,
    result: Result<StoredOpenAICodexAuth>,
) -> OpenAICodexAuthStatus {
    complete_pending_flow_with_result(result, flow_id)
}

#[cfg(test)]
pub(crate) fn complete_browser_auth_with_timeout_for_tests(
    exchange: &dyn OpenAICodexExchange,
    timeout: Duration,
) -> OpenAICodexAuthStatus {
    complete_browser_auth_with_timeout_sync(exchange, timeout, None)
}

#[cfg(test)]
pub(crate) fn complete_browser_auth_with_timeout_ready_signal_for_tests(
    exchange: &dyn OpenAICodexExchange,
    timeout: Duration,
    ready_signal: std::sync::mpsc::Sender<()>,
) -> OpenAICodexAuthStatus {
    complete_browser_auth_with_timeout_sync(exchange, timeout, Some(ready_signal))
}
