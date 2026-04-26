use anyhow::{Context, Result};
use ring::hmac;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use super::*;

const WEBHOOK_MAX_REQUEST_BYTES: usize = 64 * 1024;
const WEBHOOK_SIGNATURE_HEADER: &str = "x-tamux-signature-256";
const WEBHOOK_TIMESTAMP_HEADER: &str = "x-tamux-timestamp-ms";

fn webhook_listener_enabled(config: &AgentConfig) -> bool {
    config
        .extra
        .get("webhook_listener_enabled")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}

fn webhook_listener_bind_addr(config: &AgentConfig) -> String {
    config
        .extra
        .get("webhook_listener_bind")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("127.0.0.1:8787")
        .to_string()
}

fn webhook_listener_secret(config: &AgentConfig) -> Option<String> {
    config
        .extra
        .get("webhook_listener_secret")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn webhook_listener_max_age_secs(config: &AgentConfig) -> u64 {
    config
        .extra
        .get("webhook_listener_max_age_secs")
        .and_then(|value| value.as_u64())
        .unwrap_or(300)
}

fn now_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub(crate) fn compute_webhook_signature(secret: &str, timestamp_ms: u64, body: &[u8]) -> String {
    let key = hmac::Key::new(hmac::HMAC_SHA256, secret.as_bytes());
    let mut message = timestamp_ms.to_string().into_bytes();
    message.push(b'.');
    message.extend_from_slice(body);
    let signature = hmac::sign(&key, &message);
    format!("sha256={}", encode_hex(signature.as_ref()))
}

fn encode_hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(&mut output, "{byte:02x}");
    }
    output
}

fn header_value<'a>(headers: &'a HashMap<String, String>, name: &str) -> Option<&'a str> {
    headers.get(&name.to_ascii_lowercase()).map(String::as_str)
}

fn parse_content_length(headers: &HashMap<String, String>) -> Result<usize> {
    header_value(headers, "content-length")
        .unwrap_or("0")
        .parse::<usize>()
        .context("invalid content-length header")
}

fn parse_timestamp_ms(headers: &HashMap<String, String>) -> Result<u64> {
    header_value(headers, WEBHOOK_TIMESTAMP_HEADER)
        .context("missing webhook timestamp")?
        .parse::<u64>()
        .context("invalid webhook timestamp header")
}

fn find_header_terminator(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

async fn read_http_request(
    stream: &mut TcpStream,
) -> Result<(String, String, HashMap<String, String>, Vec<u8>)> {
    let mut buffer = Vec::with_capacity(2048);
    let mut chunk = [0u8; 2048];
    let header_end;

    loop {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            anyhow::bail!("connection closed before request headers were complete");
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.len() > WEBHOOK_MAX_REQUEST_BYTES {
            anyhow::bail!("request too large");
        }
        if let Some(position) = find_header_terminator(&buffer) {
            header_end = position + 4;
            break;
        }
    }

    let header_text = String::from_utf8_lossy(&buffer[..header_end]);
    let mut lines = header_text.lines();
    let request_line = lines
        .next()
        .context("request line missing from webhook request")?;
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts
        .next()
        .context("webhook request method missing")?
        .to_string();
    let path = request_parts
        .next()
        .context("webhook request path missing")?
        .to_string();

    let mut headers = HashMap::new();
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }

    let content_length = parse_content_length(&headers)?;
    if content_length > WEBHOOK_MAX_REQUEST_BYTES {
        anyhow::bail!("request body too large");
    }

    while buffer.len() < header_end + content_length {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            anyhow::bail!("connection closed before request body was complete");
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.len() > WEBHOOK_MAX_REQUEST_BYTES {
            anyhow::bail!("request too large");
        }
    }

    let body = buffer[header_end..header_end + content_length].to_vec();
    Ok((method, path, headers, body))
}

async fn write_http_response(
    stream: &mut TcpStream,
    status_line: &str,
    content_type: &str,
    body: &[u8],
) -> Result<()> {
    let response = format!(
        "HTTP/1.1 {status_line}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream.write_all(response.as_bytes()).await?;
    stream.write_all(body).await?;
    stream.shutdown().await?;
    Ok(())
}

impl AgentEngine {
    pub(crate) async fn webhook_listener_addr(&self) -> Option<String> {
        self.webhook_listener_addr.read().await.clone()
    }

    pub(super) async fn run_webhook_listener(
        self: Arc<Self>,
        mut shutdown: tokio::sync::watch::Receiver<bool>,
    ) {
        let config = self.config.read().await.clone();
        if !webhook_listener_enabled(&config) {
            return;
        }

        let bind_addr = webhook_listener_bind_addr(&config);
        let secret = webhook_listener_secret(&config);
        let max_age_secs = webhook_listener_max_age_secs(&config);
        let listener = match TcpListener::bind(&bind_addr).await {
            Ok(listener) => listener,
            Err(error) => {
                tracing::warn!(addr = %bind_addr, error = %error, "failed to bind webhook listener");
                return;
            }
        };

        let resolved_addr = listener
            .local_addr()
            .map(|addr| addr.to_string())
            .unwrap_or(bind_addr);
        *self.webhook_listener_addr.write().await = Some(resolved_addr.clone());
        tracing::info!(addr = %resolved_addr, "webhook listener ready");

        loop {
            tokio::select! {
                _ = shutdown.changed() => break,
                accept_result = listener.accept() => {
                    match accept_result {
                        Ok((mut stream, _)) => {
                            if let Err(error) = self.handle_webhook_connection(&mut stream, secret.as_deref(), max_age_secs).await {
                                tracing::warn!(error = %error, "webhook listener request failed");
                            }
                        }
                        Err(error) => {
                            tracing::warn!(error = %error, "webhook listener accept failed");
                        }
                    }
                }
            }
        }

        self.webhook_listener_addr.write().await.take();
    }

    async fn handle_webhook_connection(
        &self,
        stream: &mut TcpStream,
        secret: Option<&str>,
        max_age_secs: u64,
    ) -> Result<()> {
        let (method, path, headers, body) = match read_http_request(stream).await {
            Ok(request) => request,
            Err(error) => {
                let _ = write_http_response(
                    stream,
                    "400 Bad Request",
                    "text/plain; charset=utf-8",
                    b"Invalid webhook request.",
                )
                .await;
                return Err(error);
            }
        };

        if method != "POST" {
            write_http_response(
                stream,
                "405 Method Not Allowed",
                "text/plain; charset=utf-8",
                b"Webhook endpoint only accepts POST.",
            )
            .await?;
            return Ok(());
        }

        if path != "/webhook/event" {
            write_http_response(
                stream,
                "404 Not Found",
                "text/plain; charset=utf-8",
                b"Webhook endpoint not found.",
            )
            .await?;
            return Ok(());
        }

        if let Some(secret) = secret {
            let timestamp_ms = match parse_timestamp_ms(&headers) {
                Ok(timestamp_ms) => timestamp_ms,
                Err(error) => {
                    write_http_response(
                        stream,
                        "401 Unauthorized",
                        "text/plain; charset=utf-8",
                        b"Missing or invalid webhook timestamp.",
                    )
                    .await?;
                    return Err(error);
                }
            };
            let now_ms = now_unix_ms();
            let age_ms = now_ms.abs_diff(timestamp_ms);
            if age_ms > max_age_secs.saturating_mul(1000) {
                write_http_response(
                    stream,
                    "401 Unauthorized",
                    "text/plain; charset=utf-8",
                    b"Webhook timestamp is outside the allowed window.",
                )
                .await?;
                return Ok(());
            }

            let Some(signature_header) = header_value(&headers, WEBHOOK_SIGNATURE_HEADER) else {
                write_http_response(
                    stream,
                    "401 Unauthorized",
                    "text/plain; charset=utf-8",
                    b"Missing webhook signature.",
                )
                .await?;
                return Ok(());
            };
            let expected_signature = compute_webhook_signature(secret, timestamp_ms, &body);
            if expected_signature != signature_header {
                write_http_response(
                    stream,
                    "401 Unauthorized",
                    "text/plain; charset=utf-8",
                    b"Invalid webhook signature.",
                )
                .await?;
                return Ok(());
            }
        }

        let args: serde_json::Value = match serde_json::from_slice(&body) {
            Ok(args) => args,
            Err(error) => {
                write_http_response(
                    stream,
                    "400 Bad Request",
                    "text/plain; charset=utf-8",
                    b"Webhook body must be valid JSON.",
                )
                .await?;
                return Err(error.into());
            }
        };

        let payload = match self.ingest_webhook_event_json(&args).await {
            Ok(payload) => payload,
            Err(error) => {
                write_http_response(
                    stream,
                    "400 Bad Request",
                    "text/plain; charset=utf-8",
                    b"Webhook payload did not validate.",
                )
                .await?;
                return Err(error);
            }
        };

        let body = serde_json::to_vec(&payload).context("serialize webhook response")?;
        write_http_response(stream, "202 Accepted", "application/json", &body).await?;
        Ok(())
    }
}
