use amux_protocol::{
    AmuxCodec, ClientMessage, DaemonMessage, GatewayBootstrapPayload, GatewayRegistration,
    GATEWAY_IPC_PROTOCOL_VERSION,
};
use anyhow::{Context, Result};
use futures::{SinkExt, StreamExt};
use tokio::sync::{mpsc, Mutex};
use tokio_util::codec::Framed;

use crate::runtime::DaemonConnection;

trait DaemonIo: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send {}
impl<T> DaemonIo for T where T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send {}

type DaemonFramed = Framed<Box<dyn DaemonIo>, AmuxCodec>;
type DaemonSink = futures::stream::SplitSink<DaemonFramed, ClientMessage>;

pub struct GatewayIpcClient {
    sink: Mutex<DaemonSink>,
    incoming: mpsc::UnboundedReceiver<DaemonMessage>,
}

impl GatewayIpcClient {
    async fn connect_framed() -> Result<DaemonFramed> {
        #[cfg(unix)]
        {
            let runtime_dir =
                std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
            let path = std::path::PathBuf::from(runtime_dir).join("tamux-daemon.sock");
            let stream = tokio::net::UnixStream::connect(&path)
                .await
                .with_context(|| format!("cannot connect to daemon at {}", path.display()))?;
            let stream: Box<dyn DaemonIo> = Box::new(stream);
            return Ok(Framed::new(stream, AmuxCodec));
        }

        #[cfg(windows)]
        {
            let addr = amux_protocol::default_tcp_addr();
            let stream = tokio::net::TcpStream::connect(&addr)
                .await
                .with_context(|| format!("cannot connect to daemon on {addr}"))?;
            let stream: Box<dyn DaemonIo> = Box::new(stream);
            return Ok(Framed::new(stream, AmuxCodec));
        }
    }

    fn from_framed(framed: DaemonFramed) -> Self {
        let (sink, mut stream) = framed.split();
        let (tx, rx) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            while let Some(message) = stream.next().await {
                match message {
                    Ok(message) => {
                        if tx.send(message).is_err() {
                            break;
                        }
                    }
                    Err(error) => {
                        let _ = tx.send(DaemonMessage::Error {
                            message: format!("gateway IPC reader failed: {error}"),
                        });
                        break;
                    }
                }
            }
        });

        Self {
            sink: Mutex::new(sink),
            incoming: rx,
        }
    }

    pub async fn connect_and_register(
        registration: GatewayRegistration,
    ) -> Result<(Self, GatewayBootstrapPayload)> {
        let mut framed = Self::connect_framed().await?;
        framed
            .send(ClientMessage::GatewayRegister { registration })
            .await
            .context("send gateway registration")?;

        let bootstrap = match framed.next().await {
            Some(Ok(DaemonMessage::GatewayBootstrap { payload })) => payload,
            Some(Ok(DaemonMessage::Error { message })) => {
                anyhow::bail!("daemon rejected gateway registration: {message}");
            }
            Some(Ok(other)) => anyhow::bail!("unexpected daemon bootstrap response: {other:?}"),
            Some(Err(error)) => return Err(error.into()),
            None => anyhow::bail!("daemon closed connection during gateway bootstrap"),
        };

        Ok((Self::from_framed(framed), bootstrap))
    }
}

pub async fn connect_and_bootstrap(
    gateway_id: &str,
    supported_platforms: Vec<String>,
) -> Result<(GatewayIpcClient, GatewayBootstrapPayload)> {
    let registration = GatewayRegistration {
        gateway_id: gateway_id.to_string(),
        instance_id: format!("{}-{}", gateway_id, uuid::Uuid::new_v4()),
        protocol_version: GATEWAY_IPC_PROTOCOL_VERSION,
        supported_platforms,
        process_id: Some(std::process::id()),
    };
    GatewayIpcClient::connect_and_register(registration).await
}

impl DaemonConnection for GatewayIpcClient {
    fn send(
        &mut self,
        msg: ClientMessage,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<()>> + Send + '_>> {
        Box::pin(async move {
            self.sink
                .lock()
                .await
                .send(msg)
                .await
                .context("send gateway IPC message")
        })
    }

    fn recv(
        &mut self,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<Option<DaemonMessage>>> + Send + '_>,
    > {
        Box::pin(async move { Ok(self.incoming.recv().await) })
    }
}
