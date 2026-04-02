use amux_protocol::{AmuxCodec, ClientMessage, DaemonMessage};
use anyhow::{Context, Result};
use futures::{SinkExt, StreamExt};
use tokio_util::codec::Framed;

/// Connect to the daemon and return a framed stream.
pub(super) async fn connect(
) -> Result<Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, AmuxCodec>> {
    #[cfg(unix)]
    {
        let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
        let path = std::path::PathBuf::from(runtime_dir).join("tamux-daemon.sock");
        let stream = tokio::net::UnixStream::connect(&path)
            .await
            .with_context(|| format!("cannot connect to daemon at {}", path.display()))?;
        Ok(Framed::new(stream, AmuxCodec))
    }

    #[cfg(windows)]
    {
        let addr = amux_protocol::default_tcp_addr();
        let stream = tokio::net::TcpStream::connect(&addr)
            .await
            .with_context(|| format!("cannot connect to daemon on {addr}"))?;
        Ok(Framed::new(stream, AmuxCodec))
    }
}

/// Send a message and receive exactly one response.
pub(super) async fn roundtrip(msg: ClientMessage) -> Result<DaemonMessage> {
    let mut framed = connect().await?;
    framed.send(msg).await?;
    let resp = framed
        .next()
        .await
        .ok_or_else(|| anyhow::anyhow!("daemon closed connection"))??;
    Ok(resp)
}

pub(super) async fn roundtrip_async_until<T, F>(msg: ClientMessage, mut f: F) -> Result<T>
where
    F: FnMut(DaemonMessage) -> Option<Result<T>>,
{
    let mut framed = connect().await?;
    framed
        .send(ClientMessage::AgentDeclareAsyncCommandCapability {
            capability: amux_protocol::AsyncCommandCapability {
                version: 1,
                supports_operation_acceptance: true,
            },
        })
        .await?;
    framed.send(msg).await?;

    loop {
        let resp = framed
            .next()
            .await
            .ok_or_else(|| anyhow::anyhow!("daemon closed connection"))??;
        if let Some(result) = f(resp) {
            return result;
        }
    }
}
