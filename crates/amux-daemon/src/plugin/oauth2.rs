//! OAuth2 authorization code + PKCE flow for plugin authentication.
//!
//! Manages the full lifecycle: start flow (bind listener, build auth URL),
//! await callback (extract code + state), exchange code for tokens,
//! and refresh expired tokens. Tokens never appear in logs.

use anyhow::{Context, Result};
use oauth2::{
    AuthUrl, AuthorizationCode, ClientId, ClientSecret, CsrfToken, PkceCodeChallenge,
    PkceCodeVerifier, RedirectUrl, RefreshToken, Scope, TokenResponse, TokenUrl,
};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Configuration for starting an OAuth2 flow.
#[derive(Debug, Clone)]
pub struct OAuthFlowConfig {
    pub client_id: String,
    pub client_secret: Option<String>,
    pub authorization_url: String,
    pub token_url: String,
    pub scopes: Vec<String>,
    pub pkce: bool,
}

/// Result of a successful token exchange or refresh.
#[derive(Debug, Clone)]
pub struct OAuthFlowResult {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: Option<u64>,
}

/// Holds the in-progress OAuth flow state between start and callback.
pub struct OAuthFlowState {
    /// The authorization URL the user should open in their browser.
    pub auth_url: String,
    /// The redirect URI (http://127.0.0.1:{port}/callback).
    pub redirect_uri: String,
    listener: tokio::net::TcpListener,
    csrf_state: String,
    pkce_verifier: Option<PkceCodeVerifier>,
    /// Stashed config for use in exchange_code.
    config: OAuthFlowConfig,
}

/// Start the OAuth2 flow: bind an ephemeral port listener and build the authorization URL.
///
/// Returns an `OAuthFlowState` containing the auth URL to send to the user and
/// the listener to await the callback on.
pub async fn start_oauth_flow(config: &OAuthFlowConfig) -> Result<OAuthFlowState> {
    // Bind ephemeral port per D-04
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .context("failed to bind OAuth callback listener")?;
    let port = listener.local_addr()?.port();
    let redirect_uri = format!("http://127.0.0.1:{}/callback", port);

    let client_id = ClientId::new(config.client_id.clone());
    let auth_url =
        AuthUrl::new(config.authorization_url.clone()).context("invalid authorization_url")?;
    let redirect = RedirectUrl::new(redirect_uri.clone()).context("invalid redirect_uri")?;

    let mut client = oauth2::basic::BasicClient::new(client_id).set_auth_uri(auth_url);

    if let Some(ref secret) = config.client_secret {
        client = client.set_client_secret(ClientSecret::new(secret.clone()));
    }

    // Build authorization URL
    let mut auth_request = client.authorize_url(CsrfToken::new_random);
    auth_request = auth_request.set_redirect_uri(std::borrow::Cow::Owned(redirect));

    // Add scopes
    for scope in &config.scopes {
        auth_request = auth_request.add_scope(Scope::new(scope.clone()));
    }

    // PKCE support per D-01/AUTH-01
    let pkce_verifier = if config.pkce {
        let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
        auth_request = auth_request.set_pkce_challenge(challenge);
        Some(verifier)
    } else {
        None
    };

    let (url, csrf_state) = auth_request.url();

    Ok(OAuthFlowState {
        auth_url: url.to_string(),
        redirect_uri,
        listener,
        csrf_state: csrf_state.secret().to_string(),
        pkce_verifier,
        config: config.clone(),
    })
}

/// Parse a raw HTTP callback request, extracting the `code` and `state` query parameters.
///
/// Expects a GET request like: `GET /callback?code=XXX&state=YYY HTTP/1.1\r\n...`
/// Returns `(code, state)` on success.
fn parse_callback_request(raw: &str) -> Result<(String, String)> {
    // Extract the request line
    let request_line = raw
        .lines()
        .next()
        .ok_or_else(|| anyhow::anyhow!("empty HTTP request"))?;

    // Extract path from "GET /callback?code=XXX&state=YYY HTTP/1.1"
    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() < 2 {
        anyhow::bail!("malformed HTTP request line");
    }
    let path = parts[1];

    // Parse as URL to extract query params
    let full_url = format!("http://localhost{}", path);
    let parsed = url::Url::parse(&full_url).context("failed to parse callback URL")?;

    let mut code = None;
    let mut state = None;
    for (key, value) in parsed.query_pairs() {
        match key.as_ref() {
            "code" => code = Some(value.to_string()),
            "state" => state = Some(value.to_string()),
            _ => {}
        }
    }

    let code = code.ok_or_else(|| anyhow::anyhow!("missing 'code' parameter in callback"))?;
    let state = state.ok_or_else(|| anyhow::anyhow!("missing 'state' parameter in callback"))?;

    Ok((code, state))
}

/// Await the OAuth callback on the bound listener.
///
/// Accepts one TCP connection with a 5-minute timeout per D-06. Extracts the
/// authorization code, validates the CSRF state, sends a success HTML response,
/// and returns the authorization code.
pub async fn await_callback(state: &mut OAuthFlowState) -> Result<String> {
    // 5-minute timeout per D-06
    let (mut stream, _addr) =
        tokio::time::timeout(Duration::from_secs(300), state.listener.accept())
            .await
            .map_err(|_| anyhow::anyhow!("OAuth callback timed out after 5 minutes"))?
            .context("failed to accept OAuth callback connection")?;

    // Read the HTTP request (4096 bytes is plenty for a callback GET)
    let mut buf = vec![0u8; 4096];
    let n = stream
        .read(&mut buf)
        .await
        .context("failed to read OAuth callback request")?;
    let request_str = String::from_utf8_lossy(&buf[..n]);

    // Parse code and state from callback
    let (code, received_state) = parse_callback_request(&request_str)?;

    // Validate CSRF state
    if received_state != state.csrf_state {
        // Send error response
        let error_response = "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html\r\n\r\n<html><body><h2>Authentication failed</h2><p>State mismatch. Please try again.</p></body></html>";
        let _ = stream.write_all(error_response.as_bytes()).await;
        let _ = stream.shutdown().await;
        anyhow::bail!(
            "OAuth state mismatch: expected {}, got {}",
            state.csrf_state,
            received_state
        );
    }

    // Send success response
    let success_response = "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nConnection: close\r\n\r\n<html><body><h2>Authentication successful!</h2><p>You can close this tab.</p></body></html>";
    let _ = stream.write_all(success_response.as_bytes()).await;
    let _ = stream.shutdown().await;

    Ok(code)
}

/// Exchange an authorization code for access and refresh tokens.
///
/// Uses a no-redirect HTTP client for SSRF prevention per Pitfall 4.
pub async fn exchange_code(state: &OAuthFlowState, code: &str) -> Result<OAuthFlowResult> {
    let config = &state.config;
    let client_id = ClientId::new(config.client_id.clone());
    let token_url = TokenUrl::new(config.token_url.clone()).context("invalid token_url")?;
    let redirect = RedirectUrl::new(state.redirect_uri.clone()).context("invalid redirect_uri")?;

    let mut client = oauth2::basic::BasicClient::new(client_id).set_token_uri(token_url);

    if let Some(ref secret) = config.client_secret {
        client = client.set_client_secret(ClientSecret::new(secret.clone()));
    }

    // No-redirect HTTP client for SSRF prevention per Pitfall 4
    let http_client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(30))
        .build()
        .context("failed to build HTTP client for token exchange")?;

    let mut token_request = client.exchange_code(AuthorizationCode::new(code.to_string()));
    token_request = token_request.set_redirect_uri(std::borrow::Cow::Owned(redirect));

    // Attach PKCE verifier if applicable
    if let Some(ref verifier) = state.pkce_verifier {
        token_request =
            token_request.set_pkce_verifier(PkceCodeVerifier::new(verifier.secret().to_string()));
    }

    let token_response = token_request
        .request_async(&http_client)
        .await
        .context("OAuth token exchange failed")?;

    // IMPORTANT: Never log token values per Pitfall 2
    tracing::info!("OAuth token exchange successful");

    Ok(OAuthFlowResult {
        access_token: token_response.access_token().secret().to_string(),
        refresh_token: token_response
            .refresh_token()
            .map(|rt| rt.secret().to_string()),
        expires_in: token_response.expires_in().map(|d| d.as_secs()),
    })
}

/// Refresh an access token using a refresh token.
///
/// Uses a no-redirect HTTP client for SSRF prevention per Pitfall 4.
pub async fn refresh_access_token(
    config: &OAuthFlowConfig,
    refresh_token_str: &str,
) -> Result<OAuthFlowResult> {
    let client_id = ClientId::new(config.client_id.clone());
    let token_url = TokenUrl::new(config.token_url.clone()).context("invalid token_url")?;

    let mut client = oauth2::basic::BasicClient::new(client_id).set_token_uri(token_url);

    if let Some(ref secret) = config.client_secret {
        client = client.set_client_secret(ClientSecret::new(secret.clone()));
    }

    // No-redirect HTTP client for SSRF prevention per Pitfall 4
    let http_client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(30))
        .build()
        .context("failed to build HTTP client for token refresh")?;

    let token_response = client
        .exchange_refresh_token(&RefreshToken::new(refresh_token_str.to_string()))
        .request_async(&http_client)
        .await
        .context("OAuth token refresh failed")?;

    // IMPORTANT: Never log token values per Pitfall 2
    tracing::info!("OAuth token refresh successful");

    Ok(OAuthFlowResult {
        access_token: token_response.access_token().secret().to_string(),
        refresh_token: token_response
            .refresh_token()
            .map(|rt| rt.secret().to_string()),
        expires_in: token_response.expires_in().map(|d| d.as_secs()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_callback_request_extracts_code_and_state() {
        let raw = "GET /callback?code=abc123&state=xyz789 HTTP/1.1\r\nHost: localhost\r\n\r\n";
        let (code, state) = parse_callback_request(raw).unwrap();
        assert_eq!(code, "abc123");
        assert_eq!(state, "xyz789");
    }

    #[test]
    fn parse_callback_request_rejects_missing_code() {
        let raw = "GET /callback?state=xyz789 HTTP/1.1\r\nHost: localhost\r\n\r\n";
        let result = parse_callback_request(raw);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("code"), "error should mention 'code': {msg}");
    }

    #[test]
    fn parse_callback_request_rejects_missing_state() {
        let raw = "GET /callback?code=abc123 HTTP/1.1\r\nHost: localhost\r\n\r\n";
        let result = parse_callback_request(raw);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("state"), "error should mention 'state': {msg}");
    }

    #[test]
    fn parse_callback_request_handles_url_encoded_values() {
        let raw =
            "GET /callback?code=abc%20123&state=xyz%3D789 HTTP/1.1\r\nHost: localhost\r\n\r\n";
        let (code, state) = parse_callback_request(raw).unwrap();
        assert_eq!(code, "abc 123");
        assert_eq!(state, "xyz=789");
    }

    #[tokio::test]
    async fn start_oauth_flow_builds_url_with_pkce_and_state() {
        let config = OAuthFlowConfig {
            client_id: "test-client".to_string(),
            client_secret: None,
            authorization_url: "https://example.com/auth".to_string(),
            token_url: "https://example.com/token".to_string(),
            scopes: vec!["read".to_string(), "write".to_string()],
            pkce: true,
        };

        let state = start_oauth_flow(&config).await.unwrap();

        // Auth URL should contain expected parameters
        assert!(state.auth_url.starts_with("https://example.com/auth?"));
        assert!(state.auth_url.contains("client_id=test-client"));
        assert!(state.auth_url.contains("redirect_uri="));
        assert!(state.auth_url.contains("state="));
        assert!(
            state.auth_url.contains("code_challenge="),
            "PKCE challenge missing from URL: {}",
            state.auth_url
        );
        assert!(
            state.auth_url.contains("code_challenge_method=S256"),
            "PKCE method missing from URL: {}",
            state.auth_url
        );
        // Scopes should be present (space-separated and URL-encoded)
        assert!(
            state.auth_url.contains("scope="),
            "scopes missing from URL: {}",
            state.auth_url
        );

        // Redirect URI should be localhost with ephemeral port
        assert!(state.redirect_uri.starts_with("http://127.0.0.1:"));
        assert!(state.redirect_uri.ends_with("/callback"));

        // CSRF state should be non-empty
        assert!(!state.csrf_state.is_empty());

        // PKCE verifier should be present
        assert!(state.pkce_verifier.is_some());
    }

    #[tokio::test]
    async fn start_oauth_flow_without_pkce_has_no_challenge() {
        let config = OAuthFlowConfig {
            client_id: "test-client".to_string(),
            client_secret: Some("test-secret".to_string()),
            authorization_url: "https://example.com/auth".to_string(),
            token_url: "https://example.com/token".to_string(),
            scopes: vec![],
            pkce: false,
        };

        let state = start_oauth_flow(&config).await.unwrap();

        assert!(!state.auth_url.contains("code_challenge="));
        assert!(state.pkce_verifier.is_none());
    }

    #[tokio::test]
    async fn state_validation_rejects_mismatch() {
        let config = OAuthFlowConfig {
            client_id: "test-client".to_string(),
            client_secret: None,
            authorization_url: "https://example.com/auth".to_string(),
            token_url: "https://example.com/token".to_string(),
            scopes: vec![],
            pkce: false,
        };

        let mut state = start_oauth_flow(&config).await.unwrap();
        let port = state.listener.local_addr().unwrap().port();

        // Simulate a callback with wrong state
        let client_handle = tokio::spawn(async move {
            let mut stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{}", port))
                .await
                .unwrap();
            let request =
                "GET /callback?code=authcode123&state=WRONG_STATE HTTP/1.1\r\nHost: localhost\r\n\r\n";
            stream.write_all(request.as_bytes()).await.unwrap();
            // Read response
            let mut buf = vec![0u8; 4096];
            let _ = stream.read(&mut buf).await;
        });

        let result = await_callback(&mut state).await;
        assert!(result.is_err(), "should reject mismatched state");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("state mismatch"),
            "error should mention state mismatch: {err_msg}"
        );

        let _ = client_handle.await;
    }

    #[tokio::test]
    async fn await_callback_extracts_code_on_valid_state() {
        let config = OAuthFlowConfig {
            client_id: "test-client".to_string(),
            client_secret: None,
            authorization_url: "https://example.com/auth".to_string(),
            token_url: "https://example.com/token".to_string(),
            scopes: vec![],
            pkce: false,
        };

        let mut state = start_oauth_flow(&config).await.unwrap();
        let port = state.listener.local_addr().unwrap().port();
        let csrf = state.csrf_state.clone();

        // Simulate a callback with correct state
        let client_handle = tokio::spawn(async move {
            let mut stream = tokio::net::TcpStream::connect(format!("127.0.0.1:{}", port))
                .await
                .unwrap();
            let request = format!(
                "GET /callback?code=my_auth_code&state={} HTTP/1.1\r\nHost: localhost\r\n\r\n",
                csrf
            );
            stream.write_all(request.as_bytes()).await.unwrap();
            // Read response
            let mut buf = vec![0u8; 4096];
            let _ = stream.read(&mut buf).await;
        });

        let code = await_callback(&mut state).await.unwrap();
        assert_eq!(code, "my_auth_code");

        let _ = client_handle.await;
    }
}
