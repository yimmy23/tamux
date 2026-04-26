use amux_protocol::{AmuxCodec, ClientMessage, DaemonMessage};
use anyhow::{Context, Result};
use futures::{SinkExt, StreamExt};
use tokio_util::codec::Framed;

async fn maybe_handle_roundtrip_gateway_control_message<T>(
    framed: &mut Framed<T, AmuxCodec>,
    message: &DaemonMessage,
) -> Result<bool>
where
    T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    match message {
        DaemonMessage::GatewayBootstrap { payload } => {
            framed
                .send(ClientMessage::GatewayAck {
                    ack: amux_protocol::GatewayAck {
                        correlation_id: payload.bootstrap_correlation_id.clone(),
                        accepted: true,
                        detail: Some("tamux-mcp bootstrap acknowledged".to_string()),
                    },
                })
                .await
                .context("failed to acknowledge gateway bootstrap")?;
            Ok(true)
        }
        _ => Ok(should_skip_roundtrip_message(message)),
    }
}

fn should_skip_roundtrip_message(message: &DaemonMessage) -> bool {
    matches!(
        message,
        DaemonMessage::GatewayBootstrap { .. }
            | DaemonMessage::GatewaySendRequest { .. }
            | DaemonMessage::GatewayReloadCommand { .. }
            | DaemonMessage::GatewayShutdownCommand { .. }
    )
}

async fn read_next_roundtrip_message<T>(framed: &mut Framed<T, AmuxCodec>) -> Result<DaemonMessage>
where
    T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    loop {
        let resp = framed
            .next()
            .await
            .ok_or_else(|| anyhow::anyhow!("daemon closed connection before responding"))??;
        if maybe_handle_roundtrip_gateway_control_message(framed, &resp).await? {
            continue;
        }
        return Ok(resp);
    }
}

async fn daemon_roundtrip_framed<T>(
    framed: &mut Framed<T, AmuxCodec>,
    msg: ClientMessage,
) -> Result<DaemonMessage>
where
    T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    framed.send(msg).await.context("failed to send to daemon")?;
    read_next_roundtrip_message(framed).await
}

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
    daemon_roundtrip_framed(&mut framed, msg).await
}

pub(super) async fn daemon_roundtrip_until<T, F>(msg: ClientMessage, mut f: F) -> Result<T>
where
    F: FnMut(DaemonMessage) -> Option<Result<T>>,
{
    let mut framed = connect_daemon().await?;
    framed.send(msg).await.context("failed to send to daemon")?;

    loop {
        let resp = read_next_roundtrip_message(&mut framed).await?;
        if let Some(result) = f(resp) {
            return result;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use amux_protocol::{
        DaemonCodec, GatewayAck, GatewayBootstrapPayload, GatewayContinuityState,
        GatewayReloadCommand, SessionInfo,
    };

    #[tokio::test]
    async fn daemon_roundtrip_framed_skips_gateway_control_messages() {
        let (client, server) = tokio::io::duplex(16 * 1024);
        let mut client = Framed::new(client, AmuxCodec);
        let mut server = Framed::new(server, DaemonCodec);

        let server_task = tokio::spawn(async move {
            let received = server
                .next()
                .await
                .expect("client request should arrive")
                .expect("request should decode");
            assert!(matches!(received, ClientMessage::ListSessions));

            server
                .send(DaemonMessage::GatewayBootstrap {
                    payload: GatewayBootstrapPayload {
                        bootstrap_correlation_id: "bootstrap-1".to_string(),
                        feature_flags: Vec::new(),
                        providers: Vec::new(),
                        continuity: GatewayContinuityState::default(),
                    },
                })
                .await
                .expect("send bootstrap");

            let ack = tokio::time::timeout(std::time::Duration::from_secs(2), server.next())
                .await
                .expect("bootstrap ack should arrive")
                .expect("client should send bootstrap ack")
                .expect("ack should decode");
            match ack {
                ClientMessage::GatewayAck {
                    ack:
                        GatewayAck {
                            correlation_id,
                            accepted,
                            ..
                        },
                } => {
                    assert_eq!(correlation_id, "bootstrap-1");
                    assert!(accepted, "bootstrap ack should accept payload");
                }
                other => panic!("expected GatewayAck, got {other:?}"),
            }

            server
                .send(DaemonMessage::GatewayReloadCommand {
                    command: GatewayReloadCommand {
                        correlation_id: "reload-1".to_string(),
                        reason: Some("test".to_string()),
                        requested_at_ms: 1,
                    },
                })
                .await
                .expect("send reload");
            server
                .send(DaemonMessage::SessionList {
                    sessions: Vec::<SessionInfo>::new(),
                })
                .await
                .expect("send session list");
        });

        let response = daemon_roundtrip_framed(&mut client, ClientMessage::ListSessions)
            .await
            .expect("roundtrip should succeed");
        assert!(matches!(response, DaemonMessage::SessionList { .. }));

        server_task.await.expect("server task should join");
    }

    #[tokio::test]
    async fn daemon_roundtrip_framed_acknowledges_gateway_bootstrap_before_returning_response() {
        let (client, server) = tokio::io::duplex(16 * 1024);
        let mut client = Framed::new(client, AmuxCodec);
        let mut server = Framed::new(server, DaemonCodec);

        let server_task = tokio::spawn(async move {
            let received = server
                .next()
                .await
                .expect("client request should arrive")
                .expect("request should decode");
            assert!(matches!(received, ClientMessage::ListSessions));

            server
                .send(DaemonMessage::GatewayBootstrap {
                    payload: GatewayBootstrapPayload {
                        bootstrap_correlation_id: "bootstrap-ack-1".to_string(),
                        feature_flags: Vec::new(),
                        providers: Vec::new(),
                        continuity: GatewayContinuityState::default(),
                    },
                })
                .await
                .expect("send bootstrap");

            let ack = tokio::time::timeout(std::time::Duration::from_secs(2), server.next())
                .await
                .expect("bootstrap ack should arrive")
                .expect("client should send bootstrap ack")
                .expect("ack should decode");

            match ack {
                ClientMessage::GatewayAck {
                    ack:
                        GatewayAck {
                            correlation_id,
                            accepted,
                            ..
                        },
                } => {
                    assert_eq!(correlation_id, "bootstrap-ack-1");
                    assert!(accepted, "bootstrap ack should accept payload");
                }
                other => panic!("expected GatewayAck, got {other:?}"),
            }

            server
                .send(DaemonMessage::SessionList {
                    sessions: Vec::<SessionInfo>::new(),
                })
                .await
                .expect("send session list");
        });

        let response = daemon_roundtrip_framed(&mut client, ClientMessage::ListSessions)
            .await
            .expect("roundtrip should succeed");
        assert!(matches!(response, DaemonMessage::SessionList { .. }));

        server_task.await.expect("server task should join");
    }

    #[tokio::test]
    async fn daemon_roundtrip_until_skips_gateway_control_messages() {
        let (client, server) = tokio::io::duplex(16 * 1024);
        let mut client = Framed::new(client, AmuxCodec);
        let mut server = Framed::new(server, DaemonCodec);

        let server_task = tokio::spawn(async move {
            let received = server
                .next()
                .await
                .expect("client request should arrive")
                .expect("request should decode");
            assert!(matches!(received, ClientMessage::Ping));

            server
                .send(DaemonMessage::GatewayBootstrap {
                    payload: GatewayBootstrapPayload {
                        bootstrap_correlation_id: "bootstrap-2".to_string(),
                        feature_flags: Vec::new(),
                        providers: Vec::new(),
                        continuity: GatewayContinuityState::default(),
                    },
                })
                .await
                .expect("send bootstrap");

            let ack = tokio::time::timeout(std::time::Duration::from_secs(2), server.next())
                .await
                .expect("bootstrap ack should arrive")
                .expect("client should send bootstrap ack")
                .expect("ack should decode");
            match ack {
                ClientMessage::GatewayAck {
                    ack:
                        GatewayAck {
                            correlation_id,
                            accepted,
                            ..
                        },
                } => {
                    assert_eq!(correlation_id, "bootstrap-2");
                    assert!(accepted, "bootstrap ack should accept payload");
                }
                other => panic!("expected GatewayAck, got {other:?}"),
            }

            server.send(DaemonMessage::Pong).await.expect("send pong");
        });

        client
            .send(ClientMessage::Ping)
            .await
            .expect("send ping request");
        let response = read_next_roundtrip_message(&mut client)
            .await
            .expect("response should arrive");
        assert!(matches!(response, DaemonMessage::Pong));

        server_task.await.expect("server task should join");
    }
}
