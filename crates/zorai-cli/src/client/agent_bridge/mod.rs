mod commands;
pub(super) mod events;

use anyhow::Result;
use futures::SinkExt;
use tokio::io::{AsyncBufReadExt, BufReader};
use zorai_protocol::ClientMessage;

use super::connection::connect;

fn emit_agent_event(json: &str) -> Result<()> {
    println!("{json}");
    Ok(())
}

pub(super) fn initial_bridge_messages() -> Vec<ClientMessage> {
    vec![
        ClientMessage::AgentSubscribe,
        ClientMessage::AgentDeclareAsyncCommandCapability {
            capability: zorai_protocol::AsyncCommandCapability {
                version: 1,
                supports_operation_acceptance: true,
            },
        },
    ]
}

#[cfg(test)]
pub(super) async fn handle_message_for_test<T>(
    framed: &mut tokio_util::codec::Framed<T, zorai_protocol::ZoraiCodec>,
) -> Result<bool>
where
    T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    let mut thread_detail_chunks = None;
    events::handle_message(framed, &mut thread_detail_chunks).await
}

pub async fn run_agent_bridge() -> Result<()> {
    let mut framed = connect().await?;

    for message in initial_bridge_messages() {
        framed.send(message).await?;
    }
    println!(r#"{{"type":"ready"}}"#);

    let mut stdin_lines = BufReader::new(tokio::io::stdin()).lines();
    let mut thread_detail_chunks = None;

    loop {
        let continue_running = tokio::select! {
            maybe_line = stdin_lines.next_line() => {
                match maybe_line? {
                    Some(line) => commands::handle_line(&mut framed, &line).await?,
                    None => false,
                }
            }
            continue_running = events::handle_message(&mut framed, &mut thread_detail_chunks) => continue_running?,
        };
        if !continue_running {
            break;
        }
    }

    Ok(())
}
