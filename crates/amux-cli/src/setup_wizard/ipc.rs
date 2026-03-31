use super::*;

#[cfg(unix)]
pub(super) async fn wizard_connect(
) -> Result<Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, AmuxCodec>> {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    let path = std::path::PathBuf::from(runtime_dir).join("tamux-daemon.sock");
    let stream = tokio::net::UnixStream::connect(&path)
        .await
        .with_context(|| format!("cannot connect to daemon at {}", path.display()))?;
    Ok(Framed::new(stream, AmuxCodec))
}

#[cfg(windows)]
pub(super) async fn wizard_connect(
) -> Result<Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, AmuxCodec>> {
    let addr = amux_protocol::default_tcp_addr();
    let stream = tokio::net::TcpStream::connect(&addr)
        .await
        .with_context(|| format!("cannot connect to daemon on {addr}"))?;
    Ok(Framed::new(stream, AmuxCodec))
}

pub(super) async fn wizard_send(
    framed: &mut Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, AmuxCodec>,
    msg: ClientMessage,
) -> Result<()> {
    framed.send(msg).await.map_err(Into::into)
}

pub(super) async fn wizard_recv(
    framed: &mut Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, AmuxCodec>,
) -> Result<DaemonMessage> {
    framed
        .next()
        .await
        .ok_or_else(|| anyhow::anyhow!("daemon closed connection"))?
        .map_err(Into::into)
}

pub(super) async fn validate_provider_on_stream(
    framed: &mut Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, AmuxCodec>,
    provider_id: &str,
    auth_source: &str,
) -> Result<bool> {
    wizard_send(
        framed,
        ClientMessage::AgentValidateProvider {
            provider_id: provider_id.to_string(),
            base_url: String::new(),
            api_key: String::new(),
            auth_source: auth_source.to_string(),
        },
    )
    .await?;

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        match tokio::time::timeout_at(deadline, wizard_recv(framed)).await {
            Ok(Ok(DaemonMessage::AgentProviderValidation { valid, error, .. })) => {
                if !valid {
                    if let Some(err) = error {
                        println!("Validation error: {err}");
                    }
                }
                return Ok(valid);
            }
            Ok(Ok(DaemonMessage::Error { message })) => {
                tracing::debug!("skipping daemon error during validate: {message}");
            }
            Ok(Ok(
                DaemonMessage::GatewayBootstrap { .. }
                | DaemonMessage::GatewaySendRequest { .. }
                | DaemonMessage::GatewayReloadCommand { .. }
                | DaemonMessage::GatewayShutdownCommand { .. }
                | DaemonMessage::AgentEvent { .. }
                | DaemonMessage::AgentConfigResponse { .. },
            )) => {}
            Ok(Ok(other)) => tracing::debug!("unexpected message during validate: {other:?}"),
            Ok(Err(e)) => anyhow::bail!("Connection error: {e}"),
            Err(_) => anyhow::bail!("Timed out (30s)"),
        }
    }
}

pub(super) async fn read_config_key(key: &str) -> Option<String> {
    let mut conn = wizard_connect().await.ok()?;
    wizard_send(&mut conn, ClientMessage::AgentGetConfig)
        .await
        .ok()?;
    match wizard_recv(&mut conn).await.ok()? {
        DaemonMessage::AgentConfigResponse { config_json } => {
            let val: serde_json::Value = serde_json::from_str(&config_json).ok()?;
            let mut current = &val;
            for part in key.split('.') {
                current = current.get(part)?;
            }
            match current {
                serde_json::Value::String(s) if !s.is_empty() => Some(s.clone()),
                serde_json::Value::Bool(b) => Some(b.to_string()),
                serde_json::Value::Number(n) => Some(n.to_string()),
                _ => None,
            }
        }
        _ => None,
    }
}

pub(super) async fn ensure_daemon_running() -> Result<()> {
    if wizard_connect().await.is_ok() {
        return Ok(());
    }

    println!("Starting daemon...");
    let mut cmd = std::process::Command::new("tamux-daemon");
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }

    if let Err(e) = cmd.spawn() {
        anyhow::bail!("Could not start daemon: {e}\nPlease start it manually with: tamux-daemon");
    }

    for _ in 0..10 {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        if wizard_connect().await.is_ok() {
            println!("Daemon started.");
            return Ok(());
        }
    }

    anyhow::bail!(
        "Daemon did not become reachable within 5 seconds.\n\
         Please start it manually with: tamux-daemon"
    )
}
