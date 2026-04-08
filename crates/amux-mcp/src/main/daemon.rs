use amux_protocol::{AmuxCodec, ClientMessage, DaemonMessage};
use anyhow::{Context, Result};
use futures::{SinkExt, StreamExt};
use tokio_util::codec::Framed;

pub(super) async fn connect_daemon(
) -> Result<Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, AmuxCodec>> {
    #[cfg(unix)]
    {
        let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
        let path = std::path::PathBuf::from(runtime_dir).join("tamux-daemon.sock");
        let stream = tokio::net::UnixStream::connect(&path)
            .await
            .with_context(|| format!("cannot connect to tamux daemon at {}", path.display()))?;
        Ok(Framed::new(stream, AmuxCodec))
    }

    #[cfg(windows)]
    {
        let addr = amux_protocol::default_tcp_addr();
        let stream = tokio::net::TcpStream::connect(&addr)
            .await
            .with_context(|| format!("cannot connect to tamux daemon on {addr}"))?;
        Ok(Framed::new(stream, AmuxCodec))
    }
}

pub(super) async fn daemon_roundtrip(msg: ClientMessage) -> Result<DaemonMessage> {
    let mut framed = connect_daemon().await?;
    framed.send(msg).await.context("failed to send to daemon")?;
    let resp = framed
        .next()
        .await
        .ok_or_else(|| anyhow::anyhow!("daemon closed connection before responding"))??;
    Ok(resp)
}

pub(super) async fn daemon_roundtrip_until<T, F>(msg: ClientMessage, mut f: F) -> Result<T>
where
    F: FnMut(DaemonMessage) -> Option<Result<T>>,
{
    let mut framed = connect_daemon().await?;
    framed.send(msg).await.context("failed to send to daemon")?;

    loop {
        let resp = framed
            .next()
            .await
            .ok_or_else(|| anyhow::anyhow!("daemon closed connection before responding"))??;
        if let Some(result) = f(resp) {
            return result;
        }
    }
}
