use super::*;

pub(super) fn generate_pkce_pair() -> (String, String) {
    let verifier = format!("{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(Sha256::digest(verifier.as_bytes()));
    (verifier, challenge)
}

fn exchange_authorization_code(code: &str, verifier: &str) -> Result<StoredOpenAICodexAuth> {
    let mut response = ureq::post(OPENAI_CODEX_AUTH_TOKEN_URL)
        .content_type("application/x-www-form-urlencoded")
        .send_form([
            ("grant_type", "authorization_code"),
            ("client_id", OPENAI_CODEX_AUTH_CLIENT_ID),
            ("code", code),
            ("code_verifier", verifier),
            ("redirect_uri", OPENAI_CODEX_AUTH_REDIRECT_URI),
        ])?;
    let payload: TokenResponse = response.body_mut().read_json()?;
    let account_id = extract_openai_codex_account_id(&payload.access_token)
        .context("OpenAI OAuth exchange returned no ChatGPT account id")?;
    let now = now_millis();
    Ok(StoredOpenAICodexAuth {
        provider: "openai-codex".to_string(),
        auth_mode: "chatgpt_subscription".to_string(),
        access_token: payload.access_token,
        refresh_token: payload.refresh_token,
        account_id,
        expires_at: now.saturating_add(payload.expires_in.saturating_mul(1000)),
        source: "tamux".to_string(),
        updated_at: now,
        created_at: now,
    })
}

pub(super) fn complete_browser_auth(state: String, verifier: String) -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:1455")
        .context("failed to bind localhost callback listener on port 1455")?;
    listener
        .set_nonblocking(false)
        .context("failed to configure callback listener")?;
    let (mut stream, _) = listener.accept()?;
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut buffer = [0u8; 8192];
    let read = stream.read(&mut buffer)?;
    let request = String::from_utf8_lossy(&buffer[..read]);
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

    if callback_state != state || code.is_empty() {
        let _ = stream.write_all(
            b"HTTP/1.1 400 Bad Request\r\nContent-Type: text/plain\r\nConnection: close\r\n\r\nInvalid OpenAI OAuth callback.",
        );
        anyhow::bail!("invalid OpenAI OAuth callback");
    }

    let _ = stream.write_all(
        b"HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nConnection: close\r\n\r\n<!doctype html><html><body><p>Authentication successful. Return to tamux.</p></body></html>",
    );
    let auth = exchange_authorization_code(&code, &verifier)?;
    write_stored_openai_codex_auth(&auth)?;
    Ok(())
}
