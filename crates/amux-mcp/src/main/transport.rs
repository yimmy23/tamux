use std::io::Write;

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, BufReader};

use super::rpc::JsonRpcResponse;

pub(super) async fn read_message(reader: &mut BufReader<tokio::io::Stdin>) -> Result<Option<String>> {
    loop {
        let mut line = String::new();
        let n = reader
            .read_line(&mut line)
            .await
            .context("failed to read from stdin")?;

        if n == 0 {
            return Ok(None);
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed.starts_with('{') {
            return Ok(Some(trimmed.to_string()));
        }

        if let Some(value) = trimmed.strip_prefix("Content-Length:") {
            let length: usize = value.trim().parse().context("invalid Content-Length value")?;

            loop {
                let mut header_line = String::new();
                let hn = reader
                    .read_line(&mut header_line)
                    .await
                    .context("failed to read header")?;
                if hn == 0 || header_line.trim().is_empty() {
                    break;
                }
            }

            let mut body = vec![0u8; length];
            tokio::io::AsyncReadExt::read_exact(reader, &mut body)
                .await
                .context("failed to read message body")?;

            let text = String::from_utf8(body).context("message body is not valid UTF-8")?;
            return Ok(Some(text));
        }
    }
}

pub(super) fn write_message(msg: &JsonRpcResponse) -> Result<()> {
    let body = serde_json::to_string(msg).context("failed to serialize response")?;

    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    if std::env::var("TAMUX_MCP_FRAMING").as_deref() == Ok("content-length") {
        let header = format!("Content-Length: {}\r\n\r\n", body.len());
        out.write_all(header.as_bytes())?;
        out.write_all(body.as_bytes())?;
    } else {
        out.write_all(body.as_bytes())?;
        out.write_all(b"\n")?;
    }

    out.flush()?;
    Ok(())
}
