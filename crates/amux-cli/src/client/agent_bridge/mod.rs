mod commands;
mod events;

use amux_protocol::ClientMessage;
use anyhow::Result;
use futures::SinkExt;
use tokio::io::{AsyncBufReadExt, BufReader};

use super::connection::connect;

fn emit_agent_event(json: &str) -> Result<()> {
    println!("{json}");
    Ok(())
}

pub async fn run_agent_bridge() -> Result<()> {
    let mut framed = connect().await?;

    framed.send(ClientMessage::AgentSubscribe).await?;
    println!(r#"{{"type":"ready"}}"#);

    let mut stdin_lines = BufReader::new(tokio::io::stdin()).lines();

    loop {
        let continue_running = tokio::select! {
            maybe_line = stdin_lines.next_line() => {
                match maybe_line? {
                    Some(line) => commands::handle_line(&mut framed, &line).await?,
                    None => false,
                }
            }
            continue_running = events::handle_message(&mut framed) => continue_running?,
        };
        if !continue_running {
            break;
        }
    }

    Ok(())
}
