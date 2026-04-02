if matches!(
        &msg,
        ClientMessage::KillSession{ .. } |
        ClientMessage::Input{ .. } |
        ClientMessage::ExecuteManagedCommand{ .. } |
        ClientMessage::ResolveApproval{ .. } |
        ClientMessage::Resize{ .. } |
        ClientMessage::ListSessions |
        ClientMessage::GetScrollback{ .. } |
        ClientMessage::AnalyzeSession{ .. } |
        ClientMessage::SearchHistory{ .. } |
        ClientMessage::AppendCommandLog{ .. } |
        ClientMessage::CompleteCommandLog{ .. } |
        ClientMessage::QueryCommandLog{ .. } |
        ClientMessage::ClearCommandLog |
        ClientMessage::CreateAgentThread{ .. } |
        ClientMessage::DeleteAgentThread{ .. } |
        ClientMessage::ListAgentThreads |
        ClientMessage::GetAgentThread{ .. } |
        ClientMessage::AddAgentMessage{ .. } |
        ClientMessage::DeleteAgentMessages{ .. }
    ) {
        match msg {
                ClientMessage::KillSession { id } => {
                    attached_rxs.retain(|(sid, _)| *sid != id);
                    let session_info = manager
                        .list()
                        .await
                        .into_iter()
                        .find(|session| session.id == id);
                    let recent_output = manager.get_analysis_text(id, Some(40)).await.ok();
                    match manager.kill(id).await {
                        Ok(()) => {
                            // Record session-end episode (EPIS-08)
                            let session_id_str = id.to_string();
                            let (session_summary, entities) = build_session_end_episode_payload(
                                &session_id_str,
                                session_info.as_ref(),
                                recent_output.as_deref(),
                            );
                            if let Err(e) = agent
                                .record_session_end_episode(
                                    &session_id_str,
                                    &session_summary,
                                    entities,
                                )
                                .await
                            {
                                tracing::warn!(session_id = %session_id_str, error = %e, "failed to record session end episode");
                            }
                            framed.send(DaemonMessage::SessionKilled { id }).await?;
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::Error {
                                    message: e.to_string(),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::Input { id, data } => {
                    if let Err(e) = manager.write_input(id, &data).await {
                        framed
                            .send(DaemonMessage::Error {
                                message: e.to_string(),
                            })
                            .await?;
                    }
                }

                ClientMessage::ExecuteManagedCommand {
                    id,
                    request,
                    client_surface,
                } => {
                    if matches!(client_surface, Some(amux_protocol::ClientSurface::Tui)) {
                        framed
                            .send(DaemonMessage::Error {
                                message: "managed terminal execution is reserved for Electron clients".to_string(),
                            })
                            .await?;
                        return Ok(());
                    }
                    match manager.execute_managed_command(id, request).await {
                        Ok(message) => {
                            if let DaemonMessage::ApprovalRequired { approval, .. } = &message {
                                let pending = crate::agent::types::ToolPendingApproval {
                                    approval_id: approval.approval_id.clone(),
                                    execution_id: approval.execution_id.clone(),
                                    command: approval.command.clone(),
                                    rationale: approval.rationale.clone(),
                                    risk_level: approval.risk_level.clone(),
                                    blast_radius: approval.blast_radius.clone(),
                                    reasons: approval.reasons.clone(),
                                    session_id: Some(id.to_string()),
                                };
                                if let Err(error) =
                                    agent.record_operator_approval_requested(&pending).await
                                {
                                    tracing::warn!(
                                        approval_id = %approval.approval_id,
                                        "failed to record operator approval request: {error}"
                                    );
                                }
                                agent
                                    .record_provenance_event(
                                        "approval_requested",
                                        "managed command requested approval",
                                        serde_json::json!({
                                            "approval_id": approval.approval_id,
                                            "session_id": id.to_string(),
                                            "command": approval.command,
                                            "risk_level": approval.risk_level,
                                            "blast_radius": approval.blast_radius,
                                        }),
                                        None,
                                        None,
                                        None,
                                        Some(approval.approval_id.as_str()),
                                        None,
                                    )
                                    .await;
                            }
                            framed.send(message).await?;
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::Error {
                                    message: e.to_string(),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::ResolveApproval {
                    id,
                    approval_id,
                    decision,
                } => match manager.resolve_approval(id, &approval_id, decision).await {
                    Ok(messages) => {
                        let _ = agent
                            .record_operator_approval_resolution(&approval_id, decision)
                            .await;
                        let _ = agent
                            .handle_task_approval_resolution(&approval_id, decision)
                            .await;
                        agent
                            .record_provenance_event(
                                match decision {
                                    amux_protocol::ApprovalDecision::ApproveOnce
                                    | amux_protocol::ApprovalDecision::ApproveSession => {
                                        "approval_granted"
                                    }
                                    amux_protocol::ApprovalDecision::Deny => "approval_denied",
                                },
                                "operator resolved approval request",
                                serde_json::json!({
                                    "approval_id": approval_id,
                                    "session_id": id.to_string(),
                                    "decision": format!("{decision:?}").to_lowercase(),
                                }),
                                None,
                                None,
                                None,
                                Some(approval_id.as_str()),
                                None,
                            )
                            .await;
                        for message in messages {
                            framed.send(message).await?;
                        }
                    }
                    Err(e) => {
                        framed
                            .send(DaemonMessage::Error {
                                message: e.to_string(),
                            })
                            .await?;
                    }
                },

                ClientMessage::Resize { id, cols, rows } => {
                    if let Err(e) = manager.resize(id, cols, rows).await {
                        framed
                            .send(DaemonMessage::Error {
                                message: e.to_string(),
                            })
                            .await?;
                    }
                }

                ClientMessage::ListSessions => {
                    let sessions = manager.list().await;
                    framed.send(DaemonMessage::SessionList { sessions }).await?;
                }

                ClientMessage::GetScrollback { id, max_lines } => {
                    match manager.get_scrollback(id, max_lines).await {
                        Ok(data) => {
                            framed.send(DaemonMessage::Scrollback { id, data }).await?;
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::Error {
                                    message: e.to_string(),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::AnalyzeSession { id, max_lines } => {
                    match manager.get_analysis_text(id, max_lines).await {
                        Ok(text) => {
                            // TODO: Send to AI model. For now, return the raw text.
                            framed
                                .send(DaemonMessage::AnalysisResult { id, result: text })
                                .await?;
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::Error {
                                    message: e.to_string(),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::SearchHistory { query, limit } => {
                    match manager
                        .search_history(&query, limit.unwrap_or(8).max(1))
                        .await
                    {
                        Ok((summary, hits)) => {
                            framed
                                .send(DaemonMessage::HistorySearchResult {
                                    query,
                                    summary,
                                    hits,
                                })
                                .await?;
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::Error {
                                    message: e.to_string(),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::AppendCommandLog { entry_json } => {
                    match serde_json::from_str::<amux_protocol::CommandLogEntry>(&entry_json) {
                        Ok(entry) => match manager.append_command_log(&entry).await {
                            Ok(()) => {
                                framed.send(DaemonMessage::CommandLogAck).await?;
                            }
                            Err(e) => {
                                framed
                                    .send(DaemonMessage::Error {
                                        message: e.to_string(),
                                    })
                                    .await?;
                            }
                        },
                        Err(e) => {
                            framed
                                .send(DaemonMessage::Error {
                                    message: format!("invalid command log payload: {e}"),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::CompleteCommandLog {
                    id,
                    exit_code,
                    duration_ms,
                } => match manager
                    .complete_command_log(&id, exit_code, duration_ms)
                    .await
                {
                    Ok(()) => {
                        framed.send(DaemonMessage::CommandLogAck).await?;
                    }
                    Err(e) => {
                        framed
                            .send(DaemonMessage::Error {
                                message: e.to_string(),
                            })
                            .await?;
                    }
                },

                ClientMessage::QueryCommandLog {
                    workspace_id,
                    pane_id,
                    limit,
                } => match manager
                    .query_command_log(workspace_id.as_deref(), pane_id.as_deref(), limit)
                    .await
                {
                    Ok(entries) => {
                        let entries_json = serde_json::to_string(&entries).unwrap_or_default();
                        framed
                            .send(DaemonMessage::CommandLogEntries { entries_json })
                            .await?;
                    }
                    Err(e) => {
                        framed
                            .send(DaemonMessage::Error {
                                message: e.to_string(),
                            })
                            .await?;
                    }
                },

                ClientMessage::ClearCommandLog => match manager.clear_command_log().await {
                    Ok(()) => {
                        framed.send(DaemonMessage::CommandLogAck).await?;
                    }
                    Err(e) => {
                        framed
                            .send(DaemonMessage::Error {
                                message: e.to_string(),
                            })
                            .await?;
                    }
                },

                ClientMessage::CreateAgentThread { thread_json } => {
                    match serde_json::from_str::<amux_protocol::AgentDbThread>(&thread_json) {
                        Ok(thread) => match manager.create_agent_thread(&thread).await {
                            Ok(()) => {
                                framed.send(DaemonMessage::AgentDbMessageAck).await?;
                            }
                            Err(e) => {
                                framed
                                    .send(DaemonMessage::Error {
                                        message: e.to_string(),
                                    })
                                    .await?;
                            }
                        },
                        Err(e) => {
                            framed
                                .send(DaemonMessage::Error {
                                    message: format!("invalid agent thread payload: {e}"),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::DeleteAgentThread { thread_id } => {
                    match manager.delete_agent_thread(&thread_id).await {
                        Ok(()) => {
                            framed.send(DaemonMessage::AgentDbMessageAck).await?;
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::Error {
                                    message: e.to_string(),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::ListAgentThreads => match manager.list_agent_threads().await {
                    Ok(threads) => {
                        let threads_json = serde_json::to_string(&threads).unwrap_or_default();
                        framed
                            .send(DaemonMessage::AgentDbThreadList { threads_json })
                            .await?;
                    }
                    Err(e) => {
                        framed
                            .send(DaemonMessage::Error {
                                message: e.to_string(),
                            })
                            .await?;
                    }
                },

                ClientMessage::GetAgentThread { thread_id } => {
                    match manager.get_agent_thread(&thread_id).await {
                        Ok(thread) => {
                            let messages = manager.list_agent_messages(&thread_id, None).await?;
                            let thread_json = serde_json::to_string(&thread).unwrap_or_default();
                            let messages_json =
                                serde_json::to_string(&messages).unwrap_or_default();
                            framed
                                .send(DaemonMessage::AgentDbThreadDetail {
                                    thread_json,
                                    messages_json,
                                })
                                .await?;
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::Error {
                                    message: e.to_string(),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::AddAgentMessage { message_json } => {
                    match serde_json::from_str::<amux_protocol::AgentDbMessage>(&message_json) {
                        Ok(message) => match manager.add_agent_message(&message).await {
                            Ok(()) => {
                                framed.send(DaemonMessage::AgentDbMessageAck).await?;
                            }
                            Err(e) => {
                                framed
                                    .send(DaemonMessage::Error {
                                        message: e.to_string(),
                                    })
                                    .await?;
                            }
                        },
                        Err(e) => {
                            framed
                                .send(DaemonMessage::Error {
                                    message: format!("invalid agent message payload: {e}"),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::DeleteAgentMessages {
                    thread_id,
                    message_ids,
                } => match agent.delete_thread_messages(&thread_id, &message_ids).await {
                    Ok(deleted) => {
                        tracing::info!(
                            thread_id = %thread_id,
                            deleted,
                            "deleted agent messages"
                        );
                        framed.send(DaemonMessage::AgentDbMessageAck).await?;
                    }
                    Err(e) => {
                        framed
                            .send(DaemonMessage::Error {
                                message: e.to_string(),
                            })
                            .await?;
                    }
                },

            _ => unreachable!("message chunk should be exhaustive"),
        }
        continue;
    }
