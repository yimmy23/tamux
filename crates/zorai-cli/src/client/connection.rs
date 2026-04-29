use anyhow::{Context, Result};
use futures::{SinkExt, StreamExt};
use tokio_util::codec::Framed;
use zorai_protocol::{ClientMessage, DaemonMessage, ZoraiCodec};

pub(super) fn closed_connection_error() -> anyhow::Error {
    anyhow::anyhow!(
        "daemon closed connection; this often indicates a zorai CLI/daemon version mismatch or a daemon crash. restart zorai-daemon so it matches the current CLI version and try again"
    )
}

/// Connect to the daemon and return a framed stream.
pub(super) async fn connect(
) -> Result<Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, ZoraiCodec>> {
    #[cfg(unix)]
    {
        let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
        let path = std::path::PathBuf::from(runtime_dir).join("zorai-daemon.sock");
        let stream = tokio::net::UnixStream::connect(&path)
            .await
            .with_context(|| format!("cannot connect to daemon at {}", path.display()))?;
        Ok(Framed::new(stream, ZoraiCodec))
    }

    #[cfg(windows)]
    {
        let addr = zorai_protocol::default_tcp_addr();
        let stream = tokio::net::TcpStream::connect(&addr)
            .await
            .with_context(|| format!("cannot connect to daemon on {addr}"))?;
        Ok(Framed::new(stream, ZoraiCodec))
    }
}

/// Send a message and receive exactly one response.
pub(super) async fn roundtrip(msg: ClientMessage) -> Result<DaemonMessage> {
    let mut framed = connect().await?;
    framed.send(msg).await?;
    let resp = framed.next().await.ok_or_else(closed_connection_error)??;
    Ok(resp)
}

pub(super) async fn roundtrip_until<T, F>(msg: ClientMessage, mut f: F) -> Result<T>
where
    F: FnMut(DaemonMessage) -> Option<Result<T>>,
{
    let mut framed = connect().await?;
    framed.send(msg).await?;

    loop {
        let resp = framed.next().await.ok_or_else(closed_connection_error)??;
        if let Some(result) = f(resp) {
            return result;
        }
    }
}

pub(super) async fn roundtrip_async_until<T, F>(msg: ClientMessage, mut f: F) -> Result<T>
where
    F: FnMut(DaemonMessage) -> Option<Result<T>>,
{
    let mut framed = connect().await?;
    framed
        .send(ClientMessage::AgentDeclareAsyncCommandCapability {
            capability: zorai_protocol::AsyncCommandCapability {
                version: 1,
                supports_operation_acceptance: true,
            },
        })
        .await?;
    framed.send(msg).await?;

    loop {
        let resp = framed.next().await.ok_or_else(closed_connection_error)??;
        if let Some(result) = f(resp) {
            return result;
        }
    }
}
