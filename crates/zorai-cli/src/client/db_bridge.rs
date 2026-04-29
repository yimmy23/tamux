use anyhow::Result;
use futures::{SinkExt, StreamExt};
use tokio::io::{AsyncBufReadExt, BufReader};
use zorai_protocol::{ClientMessage, DaemonMessage};

use super::connection::connect;
use super::db_protocol::DbBridgeCommand;

fn emit_db_event(json: &str) -> Result<()> {
    println!("{json}");
    Ok(())
}

pub async fn run_db_bridge() -> Result<()> {
    let mut framed = connect().await?;

    println!("{{\"type\":\"ready\"}}");

    let mut stdin_lines = BufReader::new(tokio::io::stdin()).lines();

    loop {
        tokio::select! {
            maybe_line = stdin_lines.next_line() => {
                match maybe_line? {
                    Some(line) => {
                        let command: DbBridgeCommand = match serde_json::from_str(&line) {
                            Ok(cmd) => cmd,
                            Err(error) => {
                                let err_json = serde_json::json!({"type":"error","message":format!("invalid command: {error}")});
                                emit_db_event(&err_json.to_string())?;
                                continue;
                            }
                        };

                        match command {
                            DbBridgeCommand::AppendCommandLog { entry_json } => {
                                framed.send(ClientMessage::AppendCommandLog { entry_json }).await?;
                            }
                            DbBridgeCommand::CompleteCommandLog { id, exit_code, duration_ms } => {
                                framed.send(ClientMessage::CompleteCommandLog { id, exit_code, duration_ms }).await?;
                            }
                            DbBridgeCommand::QueryCommandLog { workspace_id, pane_id, limit } => {
                                framed.send(ClientMessage::QueryCommandLog { workspace_id, pane_id, limit }).await?;
                            }
                            DbBridgeCommand::ClearCommandLog => {
                                framed.send(ClientMessage::ClearCommandLog).await?;
                            }
                            DbBridgeCommand::CreateAgentThread { thread_json } => {
                                framed.send(ClientMessage::CreateAgentThread { thread_json }).await?;
                            }
                            DbBridgeCommand::DeleteAgentThread { thread_id } => {
                                framed.send(ClientMessage::DeleteAgentThread { thread_id }).await?;
                            }
                            DbBridgeCommand::ListAgentThreads => {
                                framed.send(ClientMessage::ListAgentThreads).await?;
                            }
                            DbBridgeCommand::GetAgentThread {
                                thread_id,
                                include_deleted,
                            } => {
                                framed
                                    .send(ClientMessage::GetAgentThread {
                                        thread_id,
                                        include_deleted,
                                    })
                                    .await?;
                            }
                            DbBridgeCommand::AddAgentMessage { message_json } => {
                                framed.send(ClientMessage::AddAgentMessage { message_json }).await?;
                            }
                            DbBridgeCommand::DeleteAgentMessages { thread_id, message_ids } => {
                                framed.send(ClientMessage::DeleteAgentMessages { thread_id, message_ids }).await?;
                            }
                            DbBridgeCommand::RestoreAgentMessages { thread_id, message_ids } => {
                                framed
                                    .send(ClientMessage::RestoreAgentMessages {
                                        thread_id,
                                        message_ids,
                                    })
                                    .await?;
                            }
                            DbBridgeCommand::ListAgentMessages {
                                thread_id,
                                limit,
                                include_deleted,
                            } => {
                                framed
                                    .send(ClientMessage::ListAgentMessages {
                                        thread_id,
                                        limit,
                                        include_deleted,
                                    })
                                    .await?;
                            }
                            DbBridgeCommand::UpsertTranscriptIndex { entry_json } => {
                                framed.send(ClientMessage::UpsertTranscriptIndex { entry_json }).await?;
                            }
                            DbBridgeCommand::ListTranscriptIndex { workspace_id } => {
                                framed.send(ClientMessage::ListTranscriptIndex { workspace_id }).await?;
                            }
                            DbBridgeCommand::UpsertSnapshotIndex { entry_json } => {
                                framed.send(ClientMessage::UpsertSnapshotIndex { entry_json }).await?;
                            }
                            DbBridgeCommand::ListSnapshotIndex { workspace_id } => {
                                framed.send(ClientMessage::ListSnapshotIndex { workspace_id }).await?;
                            }
                            DbBridgeCommand::UpsertAgentEvent { event_json } => {
                                framed.send(ClientMessage::UpsertAgentEvent { event_json }).await?;
                            }
                            DbBridgeCommand::ListAgentEvents { category, pane_id, limit } => {
                                framed.send(ClientMessage::ListAgentEvents { category, pane_id, limit }).await?;
                            }
                            DbBridgeCommand::ListDatabaseTables => {
                                framed.send(ClientMessage::ListDatabaseTables).await?;
                            }
                            DbBridgeCommand::QueryDatabaseRows { table_name, offset, limit, sort_column, sort_direction } => {
                                framed.send(ClientMessage::QueryDatabaseRows { table_name, offset, limit, sort_column, sort_direction }).await?;
                            }
                            DbBridgeCommand::UpdateDatabaseRows { table_name, updates_json } => {
                                framed.send(ClientMessage::UpdateDatabaseRows { table_name, updates_json }).await?;
                            }
                            DbBridgeCommand::QueueSemanticBackfill { limit } => {
                                framed.send(ClientMessage::QueueSemanticBackfill { limit }).await?;
                            }
                            DbBridgeCommand::GetSemanticIndexStatus { embedding_model, dimensions } => {
                                framed.send(ClientMessage::GetSemanticIndexStatus { embedding_model, dimensions }).await?;
                            }
                            DbBridgeCommand::Shutdown => {
                                break;
                            }
                        }
                    }
                    None => break,
                }
            }
            maybe_message = framed.next() => {
                match maybe_message {
                    Some(Ok(DaemonMessage::CommandLogEntries { entries_json })) => {
                        let msg = serde_json::json!({"type":"command-log-entries","data":serde_json::from_str::<serde_json::Value>(&entries_json).unwrap_or_default()});
                        emit_db_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::CommandLogAck)) => {
                        let msg = serde_json::json!({"type":"ack"});
                        emit_db_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::AgentDbThreadList { threads_json })) => {
                        let msg = serde_json::json!({"type":"agent-thread-list","data":serde_json::from_str::<serde_json::Value>(&threads_json).unwrap_or_default()});
                        emit_db_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::AgentDbThreadDetail { thread_json, messages_json })) => {
                        let msg = serde_json::json!({
                            "type":"agent-thread-detail",
                            "thread": serde_json::from_str::<serde_json::Value>(&thread_json).unwrap_or(serde_json::Value::Null),
                            "messages": serde_json::from_str::<serde_json::Value>(&messages_json).unwrap_or_default(),
                        });
                        emit_db_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::AgentDbMessageAck)) => {
                        let msg = serde_json::json!({"type":"ack"});
                        emit_db_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::TranscriptIndexEntries { entries_json })) => {
                        let msg = serde_json::json!({"type":"transcript-index-entries","data":serde_json::from_str::<serde_json::Value>(&entries_json).unwrap_or_default()});
                        emit_db_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::SnapshotIndexEntries { entries_json })) => {
                        let msg = serde_json::json!({"type":"snapshot-index-entries","data":serde_json::from_str::<serde_json::Value>(&entries_json).unwrap_or_default()});
                        emit_db_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::AgentEventRows { events_json })) => {
                        let msg = serde_json::json!({"type":"agent-event-rows","data":serde_json::from_str::<serde_json::Value>(&events_json).unwrap_or_default()});
                        emit_db_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::DatabaseTables { tables_json })) => {
                        let msg = serde_json::json!({"type":"database-tables","data":serde_json::from_str::<serde_json::Value>(&tables_json).unwrap_or_default()});
                        emit_db_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::DatabaseRows { rows_json })) => {
                        let msg = serde_json::json!({"type":"database-rows","data":serde_json::from_str::<serde_json::Value>(&rows_json).unwrap_or_default()});
                        emit_db_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::DatabaseUpdateAck { updated_rows })) => {
                        let msg = serde_json::json!({"type":"database-update-ack","data":{"updatedRows":updated_rows}});
                        emit_db_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::SemanticBackfillQueued { result_json })) => {
                        let msg = serde_json::json!({"type":"semantic-backfill-queued","data":serde_json::from_str::<serde_json::Value>(&result_json).unwrap_or_default()});
                        emit_db_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::SemanticIndexStatus { status_json })) => {
                        let msg = serde_json::json!({"type":"semantic-index-status","data":serde_json::from_str::<serde_json::Value>(&status_json).unwrap_or_default()});
                        emit_db_event(&msg.to_string())?;
                    }
                    Some(Ok(DaemonMessage::Error { message })) => {
                        let msg = serde_json::json!({"type":"error","message":message});
                        emit_db_event(&msg.to_string())?;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(error)) => return Err(error.into()),
                    None => {
                        let msg = serde_json::json!({"type":"error","message":"daemon connection closed"});
                        emit_db_event(&msg.to_string())?;
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}
