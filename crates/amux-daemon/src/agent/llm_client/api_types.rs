pub(crate) use super::openai_codex_auth::{
    begin_openai_codex_auth_login, clear_openai_codex_auth_test_state,
    extract_openai_codex_account_id, import_codex_cli_auth_if_present,
    read_stored_openai_codex_auth, reset_openai_codex_auth_runtime_for_tests,
    write_stored_openai_codex_auth, StoredOpenAICodexAuth,
};

fn classify_http_failure(
    status: reqwest::StatusCode,
    provider: &str,
    body_text: &str,
) -> anyhow::Error {
    let raw_message = raw_upstream_message(body_text);
    let lower = raw_message.to_ascii_lowercase();
    let request_invalid_like = lower.contains("invalid '")
        || lower.contains("invalid request")
        || lower.contains("request body")
        || lower.contains("malformed")
        || lower.contains("empty string")
        || lower.contains("tool")
        || lower.contains("required")
        || lower.contains("missing");
    let transport_incompatible_like = lower.contains("not supported")
        || lower.contains("does not support")
        || lower.contains("incompatible");
    let class = if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        UpstreamFailureClass::RateLimit
    } else if matches!(
        status,
        reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN
    ) {
        UpstreamFailureClass::AuthConfiguration
    } else if (matches!(
        status,
        reqwest::StatusCode::BAD_REQUEST | reqwest::StatusCode::UNPROCESSABLE_ENTITY
    ) && request_invalid_like)
        || (status == reqwest::StatusCode::UNPROCESSABLE_ENTITY && !transport_incompatible_like)
    {
        UpstreamFailureClass::RequestInvalid
    } else if matches!(
        status,
        reqwest::StatusCode::NOT_FOUND
            | reqwest::StatusCode::METHOD_NOT_ALLOWED
            | reqwest::StatusCode::UNSUPPORTED_MEDIA_TYPE
    ) || lower.contains("not supported")
        || lower.contains("does not support")
        || lower.contains("incompatible")
    {
        UpstreamFailureClass::TransportIncompatible
    } else if matches!(
        status,
        reqwest::StatusCode::REQUEST_TIMEOUT
            | reqwest::StatusCode::CONFLICT
            | reqwest::StatusCode::TOO_EARLY
            | reqwest::StatusCode::INTERNAL_SERVER_ERROR
            | reqwest::StatusCode::BAD_GATEWAY
            | reqwest::StatusCode::SERVICE_UNAVAILABLE
            | reqwest::StatusCode::GATEWAY_TIMEOUT
    ) || lower.contains("service unavailable")
        || lower.contains("temporarily unavailable")
        || lower.contains("overloaded")
        || lower.contains("try again later")
    {
        UpstreamFailureClass::TemporaryUpstream
    } else {
        UpstreamFailureClass::Unknown
    };

    let summary = match class {
        UpstreamFailureClass::RequestInvalid => {
            format!("{provider} rejected the daemon request as invalid: {raw_message}")
        }
        UpstreamFailureClass::AuthConfiguration => {
            format!("{provider} rejected the request because authentication or provider configuration is invalid: {raw_message}")
        }
        UpstreamFailureClass::TransportIncompatible => {
            format!("The selected provider/transport combination is incompatible for {provider}: {raw_message}")
        }
        UpstreamFailureClass::TemporaryUpstream => {
            format!("{provider} is temporarily unavailable upstream: {raw_message}")
        }
        UpstreamFailureClass::RateLimit => format!("{provider} API returned 429: {raw_message}"),
        UpstreamFailureClass::TransientTransport => {
            format!("{provider} transport error: {raw_message}")
        }
        UpstreamFailureClass::Unknown => format!("{provider} API returned {status}: {raw_message}"),
    };

    UpstreamFailureError::new(
        class,
        summary,
        serde_json::json!({
            "provider": provider,
            "status": status.as_u16(),
            "raw_message": raw_message,
            "body": summarize_upstream_body(body_text),
        }),
    )
    .into()
}

fn transport_incompatibility_error(provider: &str, details: impl Into<String>) -> anyhow::Error {
    let details = details.into();
    UpstreamFailureError::new(
        UpstreamFailureClass::TransportIncompatible,
        format!(
            "The selected provider/transport combination is incompatible for {provider}: {details}"
        ),
        serde_json::json!({
            "provider": provider,
            "details": details,
        }),
    )
    .into()
}

fn upstream_failure_error(err: &anyhow::Error) -> Option<&UpstreamFailureError> {
    err.chain()
        .find_map(|cause| cause.downcast_ref::<UpstreamFailureError>())
}

fn is_timeout_error(err: &anyhow::Error) -> bool {
    for cause in err.chain() {
        if let Some(reqwest_error) = cause.downcast_ref::<reqwest::Error>() {
            if reqwest_error.is_timeout() {
                return true;
            }
        }

        if let Some(io_error) = cause.downcast_ref::<std::io::Error>() {
            if io_error.kind() == std::io::ErrorKind::TimedOut {
                return true;
            }
        }
    }

    err.to_string().to_ascii_lowercase().contains("timed out")
}

fn is_transient_transport_error(err: &anyhow::Error) -> bool {
    for cause in err.chain() {
        if let Some(reqwest_error) = cause.downcast_ref::<reqwest::Error>() {
            if reqwest_error.is_timeout()
                || reqwest_error.is_connect()
                || reqwest_error.is_request()
                || reqwest_error.is_body()
                || reqwest_error.is_decode()
            {
                return true;
            }
        }

        if let Some(io_error) = cause.downcast_ref::<std::io::Error>() {
            use std::io::ErrorKind;

            if matches!(
                io_error.kind(),
                ErrorKind::TimedOut
                    | ErrorKind::Interrupted
                    | ErrorKind::ConnectionReset
                    | ErrorKind::ConnectionAborted
                    | ErrorKind::ConnectionRefused
                    | ErrorKind::BrokenPipe
                    | ErrorKind::UnexpectedEof
                    | ErrorKind::WouldBlock
            ) {
                return true;
            }
        }
    }

    let message = err.to_string().to_ascii_lowercase();
    message.contains("error sending request for url")
        || message.contains("connection reset")
        || message.contains("connection refused")
        || message.contains("timed out")
        || message.contains("unexpected eof")
}

fn summarize_transport_error(err: &anyhow::Error) -> String {
    let chain = err.chain().map(ToString::to_string).collect::<Vec<_>>();
    if chain.is_empty() {
        "unknown transport error".to_string()
    } else {
        chain.join(": ")
    }
}

fn is_temporary_upstream_error(err: &anyhow::Error) -> bool {
    if upstream_failure_error(err)
        .map(|failure| failure.class == UpstreamFailureClass::TemporaryUpstream)
        .unwrap_or(false)
    {
        return true;
    }
    let message = err.to_string().to_ascii_lowercase();
    message.contains(" 408")
        || message.contains(" 409")
        || message.contains(" 425")
        || message.contains(" 500")
        || message.contains(" 502")
        || message.contains(" 503")
        || message.contains(" 504")
        || message.contains("service unavailable")
        || message.contains("temporarily unavailable")
        || message.contains("server overloaded")
        || message.contains("overloaded")
        || message.contains("try again later")
}

impl Stream for CompletionStream {
    type Item = Result<CompletionChunk>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        self.rx.poll_recv(cx)
    }
}

pub(crate) fn has_openai_chatgpt_subscription_auth() -> bool {
    super::openai_codex_auth::has_openai_chatgpt_subscription_auth()
}

async fn refresh_openai_codex_auth(
    client: &reqwest::Client,
    auth: &StoredOpenAICodexAuth,
) -> Result<StoredOpenAICodexAuth> {
    let response = client
        .post("https://auth.openai.com/oauth/token")
        .form(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", auth.refresh_token.as_str()),
            ("client_id", "app_EMoamEEZ73f0CkXaXp7hrann"),
        ])
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        anyhow::bail!("OpenAI token refresh failed: HTTP {status} {text}");
    }

    let payload: serde_json::Value = response.json().await?;
    let access_token = payload
        .get("access_token")
        .and_then(|value| value.as_str())
        .context("OpenAI token refresh returned no access_token")?
        .to_string();
    let refresh_token = payload
        .get("refresh_token")
        .and_then(|value| value.as_str())
        .context("OpenAI token refresh returned no refresh_token")?
        .to_string();
    let account_id = extract_openai_codex_account_id(&access_token)
        .context("OpenAI token refresh returned no ChatGPT account id")?;
    let expires_in_ms = payload
        .get("expires_in")
        .and_then(|value| value.as_i64())
        .unwrap_or(3600)
        .saturating_mul(1000);
    let now = now_millis() as i64;
    let refreshed = StoredOpenAICodexAuth {
        provider: Some("openai-codex".to_string()),
        auth_mode: Some("chatgpt_subscription".to_string()),
        access_token,
        refresh_token,
        account_id: Some(account_id),
        expires_at: Some(now.saturating_add(expires_in_ms)),
        source: auth.source.clone().or_else(|| Some("tamux".to_string())),
        updated_at: Some(now),
        created_at: auth.created_at.or(Some(now)),
    };
    write_stored_openai_codex_auth(&refreshed)?;
    Ok(refreshed)
}

async fn resolve_openai_codex_request_auth(
    client: &reqwest::Client,
    provider: &str,
    config: &ProviderConfig,
) -> Result<Option<OpenAICodexRequestAuth>> {
    if provider != "openai" || config.auth_source != AuthSource::ChatgptSubscription {
        return Ok(None);
    }

    let auth = match read_stored_openai_codex_auth() {
        Some(auth) => Some(auth),
        None => import_codex_cli_auth_if_present()?,
    };
    let mut auth = auth.context(
        "No ChatGPT subscription auth found. Authenticate in the frontend or import ~/.codex/auth.json.",
    )?;
    let now = now_millis() as i64;
    if auth.expires_at.unwrap_or(0) <= now.saturating_add(60_000) {
        auth = refresh_openai_codex_auth(client, &auth).await?;
    }

    let account_id = auth
        .account_id
        .clone()
        .or_else(|| extract_openai_codex_account_id(&auth.access_token))
        .context("ChatGPT subscription auth is missing chatgpt_account_id")?;

    Ok(Some(OpenAICodexRequestAuth {
        access_token: auth.access_token,
        account_id,
    }))
}
