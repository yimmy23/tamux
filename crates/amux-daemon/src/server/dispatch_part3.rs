if matches!(
        &msg,
        ClientMessage::ListAgentMessages{ .. } |
        ClientMessage::UpsertTranscriptIndex{ .. } |
        ClientMessage::ListTranscriptIndex{ .. } |
        ClientMessage::UpsertSnapshotIndex{ .. } |
        ClientMessage::ListSnapshotIndex{ .. } |
        ClientMessage::UpsertAgentEvent{ .. } |
        ClientMessage::ListAgentEvents{ .. } |
        ClientMessage::GenerateSkill{ .. } |
        ClientMessage::FindSymbol{ .. } |
        ClientMessage::ListSnapshots{ .. } |
        ClientMessage::RestoreSnapshot{ .. } |
        ClientMessage::ListWorkspaceSessions{ .. } |
        ClientMessage::GetGitStatus{ .. } |
        ClientMessage::GetGitDiff{ .. } |
        ClientMessage::GetFilePreview{ .. } |
        ClientMessage::SubscribeNotifications |
        ClientMessage::ScrubSensitive{ .. } |
        ClientMessage::CheckpointSession{ .. } |
        ClientMessage::VerifyTelemetryIntegrity |
        ClientMessage::AgentSendMessage{ .. } |
        ClientMessage::AgentDirectMessage{ .. } |
        ClientMessage::AgentStopStream{ .. }
    ) {
        match msg {
                ClientMessage::ListAgentMessages { thread_id, limit } => {
                    match manager.list_agent_messages(&thread_id, limit).await {
                        Ok(messages) => {
                            let thread = manager.get_agent_thread(&thread_id).await?;
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

                ClientMessage::UpsertTranscriptIndex { entry_json } => {
                    match serde_json::from_str::<amux_protocol::TranscriptIndexEntry>(&entry_json) {
                        Ok(entry) => match manager.upsert_transcript_index(&entry).await {
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
                                    message: format!("invalid transcript index payload: {e}"),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::ListTranscriptIndex { workspace_id } => {
                    match manager.list_transcript_index(workspace_id.as_deref()).await {
                        Ok(entries) => {
                            let entries_json = serde_json::to_string(&entries).unwrap_or_default();
                            framed
                                .send(DaemonMessage::TranscriptIndexEntries { entries_json })
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

                ClientMessage::UpsertSnapshotIndex { entry_json } => {
                    match serde_json::from_str::<amux_protocol::SnapshotIndexEntry>(&entry_json) {
                        Ok(entry) => match manager.upsert_snapshot_index(&entry).await {
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
                                    message: format!("invalid snapshot index payload: {e}"),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::ListSnapshotIndex { workspace_id } => {
                    match manager.list_snapshot_index(workspace_id.as_deref()).await {
                        Ok(entries) => {
                            let entries_json = serde_json::to_string(&entries).unwrap_or_default();
                            framed
                                .send(DaemonMessage::SnapshotIndexEntries { entries_json })
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

                ClientMessage::UpsertAgentEvent { event_json } => {
                    match serde_json::from_str::<amux_protocol::AgentEventRow>(&event_json) {
                        Ok(event) => match manager.upsert_agent_event(&event).await {
                            Ok(()) => {
                                if let Some(notification) =
                                    crate::notifications::parse_notification_row(&event)
                                {
                                    let _ = agent
                                        .event_sender()
                                        .send(crate::agent::types::AgentEvent::NotificationInboxUpsert {
                                            notification,
                                        });
                                }
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
                                    message: format!("invalid agent event payload: {e}"),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::ListAgentEvents {
                    category,
                    pane_id,
                    limit,
                } => {
                    match manager
                        .list_agent_events(category.as_deref(), pane_id.as_deref(), limit)
                        .await
                    {
                        Ok(events) => {
                            let events_json = serde_json::to_string(&events).unwrap_or_default();
                            framed
                                .send(DaemonMessage::AgentEventRows { events_json })
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

                ClientMessage::GenerateSkill { query, title } => {
                    match manager
                        .generate_skill(query.as_deref(), title.as_deref())
                        .await
                    {
                        Ok((title, path)) => {
                            framed
                                .send(DaemonMessage::SkillGenerated { title, path })
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

                ClientMessage::FindSymbol {
                    workspace_root,
                    symbol,
                    limit,
                } => {
                    let matches = manager.find_symbol_matches(
                        &workspace_root,
                        &symbol,
                        limit.unwrap_or(16).max(1),
                    );
                    framed
                        .send(DaemonMessage::SymbolSearchResult { symbol, matches })
                        .await?;
                }

                ClientMessage::ListSnapshots { workspace_id } => {
                    match manager.list_snapshots(workspace_id.as_deref()).await {
                        Ok(snapshots) => {
                            framed
                                .send(DaemonMessage::SnapshotList { snapshots })
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

                ClientMessage::RestoreSnapshot { snapshot_id } => {
                    match manager.restore_snapshot(&snapshot_id).await {
                        Ok((ok, message)) => {
                            framed
                                .send(DaemonMessage::SnapshotRestored {
                                    snapshot_id,
                                    ok,
                                    message,
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

                ClientMessage::ListWorkspaceSessions { workspace_id } => {
                    let sessions = manager.list_workspace(&workspace_id).await;
                    framed.send(DaemonMessage::SessionList { sessions }).await?;
                }

                ClientMessage::GetGitStatus { path } => {
                    let info = crate::git::get_git_status(&path);
                    framed.send(DaemonMessage::GitStatus { path, info }).await?;
                }

                ClientMessage::GetGitDiff {
                    repo_path,
                    file_path,
                } => {
                    let diff = crate::git::git_diff(&repo_path, file_path.as_deref());
                    framed
                        .send(DaemonMessage::GitDiff {
                            repo_path,
                            file_path,
                            diff,
                        })
                        .await?;
                }

                ClientMessage::GetFilePreview { path, max_bytes } => {
                    let (content, truncated, is_text) =
                        crate::git::read_file_preview(&path, max_bytes.unwrap_or(65_536));
                    framed
                        .send(DaemonMessage::FilePreview {
                            path,
                            content,
                            truncated,
                            is_text,
                        })
                        .await?;
                }

                ClientMessage::SubscribeNotifications => {
                    // Acknowledged. The client will receive OscNotification
                    // messages via the output broadcast channel.
                    // No explicit state change needed here.
                }

                ClientMessage::ScrubSensitive { text } => {
                    let scrubbed = crate::scrub::scrub_sensitive(&text);
                    framed
                        .send(DaemonMessage::ScrubResult { text: scrubbed })
                        .await?;
                }

                ClientMessage::CheckpointSession { id } => {
                    let dump_dir = crate::criu::dump_dir_for_session(&id.to_string())
                        .unwrap_or_else(|_| std::path::PathBuf::from("/tmp/amux-criu"));

                    if !crate::criu::is_available() {
                        framed
                            .send(DaemonMessage::SessionCheckpointed {
                                id,
                                ok: false,
                                path: None,
                                message: "CRIU is not available on this system".to_string(),
                            })
                            .await?;
                    } else {
                        // Get the PID from the session - for now report unavailable
                        // as we'd need to track the child PID in PtySession
                        framed
                            .send(DaemonMessage::SessionCheckpointed {
                                id,
                                ok: false,
                                path: Some(dump_dir.to_string_lossy().into_owned()),
                                message: "CRIU checkpoint: session PID tracking not yet integrated"
                                    .to_string(),
                            })
                            .await?;
                    }
                }

                ClientMessage::VerifyTelemetryIntegrity => {
                    match manager.verify_telemetry_integrity() {
                        Ok(results) => {
                            framed
                                .send(DaemonMessage::TelemetryIntegrityResult { results })
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

                // -----------------------------------------------------------
                // Agent engine messages
                // -----------------------------------------------------------
                ClientMessage::AgentSendMessage {
                    thread_id,
                    content,
                    session_id,
                    context_messages_json,
                    client_surface,
                } => {
                    agent.mark_operator_present("send_message").await;
                    let effective_thread_id =
                        thread_id.or_else(|| Some(format!("thread_{}", uuid::Uuid::new_v4())));
                    if let Some(thread_id) = effective_thread_id.as_ref() {
                        client_agent_threads.insert(thread_id.clone());
                    }
                    let agent = agent.clone();
                    tokio::spawn(async move {
                        let has_context = context_messages_json.is_some();
                        tracing::info!(
                            thread_id = ?effective_thread_id,
                            content_len = content.len(),
                            has_context_json = has_context,
                            "AgentSendMessage received"
                        );
                        // Seed context messages into the thread before the LLM turn
                        if let Some(ref json) = context_messages_json {
                            match serde_json::from_str::<Vec<amux_protocol::AgentDbMessage>>(json) {
                                Ok(ctx) if !ctx.is_empty() => {
                                    tracing::info!(
                                        count = ctx.len(),
                                        "seeding thread with context messages"
                                    );
                                    agent
                                        .seed_thread_context(effective_thread_id.as_deref(), &ctx)
                                        .await;
                                }
                                Ok(_) => tracing::info!("context_messages_json was empty array"),
                                Err(e) => {
                                    tracing::warn!(error = %e, json_len = json.len(), "failed to parse context_messages_json")
                                }
                            }
                        }
                        if let Err(e) = Box::pin(agent.send_message_with_session_and_surface(
                            effective_thread_id.as_deref(),
                            session_id.as_deref(),
                            &content,
                            client_surface,
                        ))
                        .await
                        {
                            tracing::warn!(error = %e, "agent send_message_with_session failed");
                        }
                    });
                }

                ClientMessage::AgentDirectMessage {
                    target,
                    thread_id,
                    content,
                    session_id,
                } => {
                    agent.mark_operator_present("direct_message").await;
                    match Box::pin(agent.send_direct_message(
                        &target,
                        thread_id.as_deref(),
                        session_id.as_deref(),
                        &content,
                    ))
                    .await
                    {
                        Ok((thread_id, response)) => {
                            client_agent_threads.insert(thread_id.clone());
                            framed
                                .send(DaemonMessage::AgentDirectMessageResponse {
                                    target,
                                    thread_id,
                                    response,
                                    session_id,
                                })
                                .await?;
                        }
                        Err(error) => {
                            framed
                                .send(DaemonMessage::Error {
                                    message: error.to_string(),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::AgentStopStream { thread_id } => {
                    client_agent_threads.insert(thread_id.clone());
                    let _ = agent.stop_stream(&thread_id).await;
                }
                ClientMessage::AgentRetryStreamNow { thread_id } => {
                    client_agent_threads.insert(thread_id.clone());
                    let _ = agent.retry_stream_now(&thread_id).await;
                }

            _ => unreachable!("message chunk should be exhaustive"),
        }
        continue;
    }
