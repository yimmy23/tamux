impl DaemonClient {
    async fn handle_daemon_message_part1(
        message: DaemonMessage,
        event_tx: &mpsc::Sender<ClientEvent>,
        thread_detail_chunks: &mut Option<ThreadDetailChunkBuffer>,
    ) {
        match message {
            DaemonMessage::AgentEvent { event_json } => {
                match serde_json::from_str::<Value>(&event_json) {
                    Ok(event) => Self::dispatch_agent_event(event, event_tx).await,
                    Err(err) => warn!("Failed to parse agent event: {}", err),
                }
            }
            DaemonMessage::AgentThreadList { threads_json } => {
                match serde_json::from_str::<Vec<AgentThread>>(&threads_json) {
                    Ok(threads) => {
                        let _ = event_tx.send(ClientEvent::ThreadList(threads)).await;
                    }
                    Err(err) => warn!("Failed to parse thread list: {}", err),
                }
            }
            DaemonMessage::AgentThreadDetail { thread_json } => {
                match serde_json::from_str::<Option<AgentThread>>(&thread_json) {
                    Ok(thread) => {
                        let _ = event_tx.send(ClientEvent::ThreadDetail(thread)).await;
                    }
                    Err(err) => warn!("Failed to parse thread detail: {}", err),
                }
            }
            DaemonMessage::AgentThreadDetailChunk {
                thread_id,
                thread_json_chunk,
                done,
            } => {
                let chunk_buffer =
                    thread_detail_chunks.get_or_insert_with(ThreadDetailChunkBuffer::default);
                if chunk_buffer.thread_id.as_deref() != Some(thread_id.as_str()) {
                    chunk_buffer.thread_id = Some(thread_id);
                    chunk_buffer.bytes.clear();
                }
                chunk_buffer.bytes.extend(thread_json_chunk);
                if done {
                    let bytes = std::mem::take(&mut chunk_buffer.bytes);
                    chunk_buffer.thread_id = None;
                    *thread_detail_chunks = None;
                    match String::from_utf8(bytes) {
                        Ok(thread_json) => {
                            match serde_json::from_str::<Option<AgentThread>>(&thread_json) {
                                Ok(thread) => {
                                    let _ = event_tx.send(ClientEvent::ThreadDetail(thread)).await;
                                }
                                Err(err) => {
                                    warn!("Failed to parse streamed thread detail: {}", err)
                                }
                            }
                        }
                        Err(err) => warn!("Failed to decode streamed thread detail: {}", err),
                    }
                }
            }
            DaemonMessage::AgentTaskList { tasks_json } => {
                match serde_json::from_str::<Vec<AgentTask>>(&tasks_json) {
                    Ok(tasks) => {
                        let _ = event_tx.send(ClientEvent::TaskList(tasks)).await;
                    }
                    Err(err) => warn!("Failed to parse task list: {}", err),
                }
            }
            DaemonMessage::AgentWorkspaceSettings { settings } => {
                let _ = event_tx.send(ClientEvent::WorkspaceSettings(settings)).await;
            }
            DaemonMessage::AgentWorkspaceSettingsList { settings } => {
                let _ = event_tx
                    .send(ClientEvent::WorkspaceSettingsList(settings))
                    .await;
            }
            DaemonMessage::AgentWorkspaceTaskList {
                workspace_id,
                tasks,
            } => {
                let _ = event_tx
                    .send(ClientEvent::WorkspaceTaskList {
                        workspace_id,
                        tasks,
                    })
                    .await;
            }
            DaemonMessage::AgentWorkspaceTaskUpdated { task } => {
                let _ = event_tx.send(ClientEvent::WorkspaceTaskUpdated(task)).await;
            }
            DaemonMessage::AgentWorkspaceTaskDeleted {
                task_id,
                deleted_at,
            } => {
                let _ = event_tx
                    .send(ClientEvent::WorkspaceTaskDeleted {
                        task_id,
                        deleted_at,
                    })
                    .await;
            }
            DaemonMessage::AgentWorkspaceNoticeList {
                workspace_id,
                notices,
            } => {
                let _ = event_tx
                    .send(ClientEvent::WorkspaceNotices {
                        workspace_id,
                        notices,
                    })
                    .await;
            }
            DaemonMessage::AgentWorkspaceError { message } => {
                let _ = event_tx.send(ClientEvent::Error(message)).await;
            }
            DaemonMessage::AgentGoalRunList { goal_runs_json } => {
                match serde_json::from_str::<Vec<GoalRun>>(&goal_runs_json) {
                    Ok(goal_runs) => {
                        let _ = event_tx.send(ClientEvent::GoalRunList(goal_runs)).await;
                    }
                    Err(err) => warn!("Failed to parse goal run list: {}", err),
                }
            }
            DaemonMessage::AgentGoalRunStarted { goal_run_json } => {
                match serde_json::from_str::<GoalRun>(&goal_run_json) {
                    Ok(goal_run) => {
                        let _ = event_tx.send(ClientEvent::GoalRunStarted(goal_run)).await;
                    }
                    Err(err) => warn!("Failed to parse goal run started payload: {}", err),
                }
            }
            DaemonMessage::AgentGoalRunDetail { goal_run_json } => {
                match serde_json::from_str::<Option<GoalRun>>(&goal_run_json) {
                    Ok(goal_run) => {
                        let _ = event_tx.send(ClientEvent::GoalRunDetail(goal_run)).await;
                    }
                    Err(err) => warn!("Failed to parse goal run detail: {}", err),
                }
            }
            DaemonMessage::AgentGoalRunDeleted {
                goal_run_id,
                deleted,
            } => {
                let _ = event_tx
                    .send(ClientEvent::GoalRunDeleted {
                        goal_run_id,
                        deleted,
                    })
                    .await;
            }
            DaemonMessage::AgentCheckpointList {
                goal_run_id,
                checkpoints_json,
            } => {
                match serde_json::from_str::<Vec<CheckpointSummary>>(&checkpoints_json) {
                    Ok(checkpoints) => {
                        let _ = event_tx
                            .send(ClientEvent::GoalRunCheckpoints {
                                goal_run_id,
                                checkpoints,
                            })
                            .await;
                    }
                    Err(err) => warn!("Failed to parse checkpoint list: {}", err),
                }
            }
            DaemonMessage::AgentCheckpointRestored { outcome_json } => {
                match serde_json::from_str::<RestoreOutcome>(&outcome_json) {
                    Ok(outcome) => {
                        let details = format!(
                            "goal {} at step {} • restored {} task(s)",
                            outcome.goal_run_id,
                            outcome.restored_step_index + 1,
                            outcome.tasks_restored
                        );
                        let _ = event_tx
                            .send(ClientEvent::WorkflowNotice {
                                thread_id: None,
                                kind: "checkpoint-restored".to_string(),
                                message: "Checkpoint restored".to_string(),
                                details: Some(details),
                            })
                            .await;
                    }
                    Err(err) => warn!("Failed to parse checkpoint restore outcome: {}", err),
                }
            }
            DaemonMessage::AgentTodoDetail {
                thread_id,
                todos_json,
            } => match serde_json::from_str::<Vec<crate::wire::TodoItem>>(&todos_json) {
                Ok(items) => {
                    let _ = event_tx
                        .send(ClientEvent::ThreadTodos {
                            thread_id,
                            goal_run_id: None,
                            step_index: None,
                            items,
                        })
                        .await;
                }
                Err(err) => warn!("Failed to parse todo detail: {}", err),
            },
            DaemonMessage::AgentWorkContextDetail {
                thread_id: _,
                context_json,
            } => match serde_json::from_str::<ThreadWorkContext>(&context_json) {
                Ok(context) => {
                    let _ = event_tx.send(ClientEvent::WorkContext(context)).await;
                }
                Err(err) => warn!("Failed to parse work context detail: {}", err),
            },
            DaemonMessage::GitDiff {
                repo_path,
                file_path,
                diff,
            } => {
                let _ = event_tx
                    .send(ClientEvent::GitDiff {
                        repo_path,
                        file_path,
                        diff,
                    })
                    .await;
            }
            DaemonMessage::FilePreview {
                path,
                content,
                truncated,
                is_text,
            } => {
                let _ = event_tx
                    .send(ClientEvent::FilePreview {
                        path,
                        content,
                        truncated,
                        is_text,
                    })
                    .await;
            }
            DaemonMessage::AgentConfigResponse { config_json } => {
                match serde_json::from_str::<Value>(&config_json) {
                    Ok(raw) => {
                        if let Ok(config) =
                            serde_json::from_value::<AgentConfigSnapshot>(raw.clone())
                        {
                            let _ = event_tx.send(ClientEvent::AgentConfig(config)).await;
                        }
                        let _ = event_tx.send(ClientEvent::AgentConfigRaw(raw)).await;
                    }
                    Err(err) => warn!("Failed to parse agent config response: {}", err),
                }
            }
            DaemonMessage::AgentThreadDeleted { thread_id, deleted } => {
                let _ = event_tx
                    .send(ClientEvent::ThreadDeleted { thread_id, deleted })
                    .await;
            }
            DaemonMessage::AgentModelsResponse {
                operation_id: _,
                models_json,
            } => match serde_json::from_str::<Vec<FetchedModel>>(&models_json) {
                Ok(models) => {
                    let _ = event_tx.send(ClientEvent::ModelsFetched(models)).await;
                }
                Err(err) => warn!("Failed to parse models response: {}", err),
            },
            DaemonMessage::AgentHeartbeatItems { items_json } => {
                match serde_json::from_str::<Vec<HeartbeatItem>>(&items_json) {
                    Ok(items) => {
                        let _ = event_tx.send(ClientEvent::HeartbeatItems(items)).await;
                    }
                    Err(err) => warn!("Failed to parse heartbeat items: {}", err),
                }
            }
            DaemonMessage::AgentEventRows { events_json } => {
                match serde_json::from_str::<Vec<amux_protocol::AgentEventRow>>(&events_json) {
                    Ok(rows) => {
                        let notifications = rows
                            .into_iter()
                            .filter_map(|row| {
                                if row.category != "notification" {
                                    return None;
                                }
                                serde_json::from_str::<amux_protocol::InboxNotification>(
                                    &row.payload_json,
                                )
                                .ok()
                            })
                            .collect::<Vec<_>>();
                        let _ = event_tx
                            .send(ClientEvent::NotificationSnapshot(notifications))
                            .await;
                    }
                    Err(err) => warn!("Failed to parse agent event rows: {}", err),
                }
            }
            DaemonMessage::AgentDbMessageAck => {}
            DaemonMessage::SessionSpawned { id } => {
                let _ = event_tx
                    .send(ClientEvent::SessionSpawned {
                        session_id: id.to_string(),
                    })
                    .await;
            }
            DaemonMessage::ApprovalRequired { approval, .. } => {
                let _ = event_tx
                    .send(ClientEvent::ApprovalRequired {
                        approval_id: approval.approval_id,
                        command: approval.command,
                        rationale: Some(approval.rationale),
                        reasons: approval.reasons,
                        risk_level: approval.risk_level,
                        blast_radius: approval.blast_radius,
                    })
                    .await;
            }
            DaemonMessage::ApprovalResolved {
                approval_id,
                decision,
                ..
            } => {
                let _ = event_tx
                    .send(ClientEvent::ApprovalResolved {
                        approval_id,
                        decision: format!("{decision:?}").to_lowercase(),
                    })
                    .await;
            }
            DaemonMessage::AgentTaskApprovalRules { rules } => {
                let _ = event_tx.send(ClientEvent::TaskApprovalRules(rules)).await;
            }
            _ => unreachable!("daemon message part1 should be exhaustive"),
        }
    }

    async fn handle_daemon_message_part2(
        message: DaemonMessage,
        event_tx: &mpsc::Sender<ClientEvent>,
    ) {
        match message {
            DaemonMessage::AgentProviderAuthStates { states_json } => {
                let states: Vec<serde_json::Value> =
                    serde_json::from_str(&states_json).unwrap_or_default();
                let entries = states
                    .iter()
                    .filter_map(|v| {
                        Some(crate::state::ProviderAuthEntry {
                            provider_id: v.get("provider_id")?.as_str()?.to_string(),
                            provider_name: v.get("provider_name")?.as_str()?.to_string(),
                            authenticated: v.get("authenticated")?.as_bool()?,
                            auth_source: v
                                .get("auth_source")
                                .and_then(|s| s.as_str())
                                .unwrap_or("api_key")
                                .to_string(),
                            model: v
                                .get("model")
                                .and_then(|s| s.as_str())
                                .unwrap_or("")
                                .to_string(),
                        })
                    })
                    .collect();
                let _ = event_tx
                    .send(ClientEvent::ProviderAuthStates(entries))
                    .await;
            }
            DaemonMessage::AgentProviderCatalog { catalog_json } => {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&catalog_json) {
                    if let Some(diagnostics) = value
                        .get("custom_provider_report")
                        .and_then(|report| report.get("diagnostics"))
                        .and_then(|diagnostics| diagnostics.as_array())
                    {
                        for diagnostic in diagnostics {
                            warn!("Custom provider configuration issue: {}", diagnostic);
                        }
                    }
                }
            }
            DaemonMessage::AgentOpenAICodexAuthStatus { status_json } => {
                match serde_json::from_str::<OpenAICodexAuthStatusVm>(&status_json) {
                    Ok(status) => {
                        let _ = event_tx
                            .send(ClientEvent::OpenAICodexAuthStatus(status))
                            .await;
                    }
                    Err(err) => warn!("Failed to parse OpenAI Codex auth status: {}", err),
                }
            }
            DaemonMessage::AgentOpenAICodexAuthLoginResult { result_json } => {
                match serde_json::from_str::<OpenAICodexAuthStatusVm>(&result_json) {
                    Ok(status) => {
                        let _ = event_tx
                            .send(ClientEvent::OpenAICodexAuthLoginResult(status))
                            .await;
                    }
                    Err(err) => warn!("Failed to parse OpenAI Codex auth login result: {}", err),
                }
            }
            DaemonMessage::AgentOpenAICodexAuthLogoutResult { ok, error } => {
                let _ = event_tx
                    .send(ClientEvent::OpenAICodexAuthLogoutResult { ok, error })
                    .await;
            }
            DaemonMessage::AgentProviderValidation {
                operation_id: _,
                provider_id,
                valid,
                error,
                ..
            } => {
                let _ = event_tx
                    .send(ClientEvent::ProviderValidation {
                        provider_id,
                        valid,
                        error,
                    })
                    .await;
            }
            DaemonMessage::AgentSubAgentList { sub_agents_json } => {
                let items: Vec<serde_json::Value> =
                    serde_json::from_str(&sub_agents_json).unwrap_or_default();
                let entries = items
                    .iter()
                    .filter_map(|v| {
                        Some(crate::state::SubAgentEntry {
                            id: v.get("id")?.as_str()?.to_string(),
                            name: v.get("name")?.as_str()?.to_string(),
                            provider: v.get("provider")?.as_str()?.to_string(),
                            model: v.get("model")?.as_str()?.to_string(),
                            role: v.get("role").and_then(|s| s.as_str()).map(String::from),
                            enabled: v.get("enabled").and_then(|b| b.as_bool()).unwrap_or(true),
                            builtin: v.get("builtin").and_then(|b| b.as_bool()).unwrap_or(false),
                            immutable_identity: v
                                .get("immutable_identity")
                                .and_then(|b| b.as_bool())
                                .unwrap_or(false),
                            disable_allowed: v
                                .get("disable_allowed")
                                .and_then(|b| b.as_bool())
                                .unwrap_or(true),
                            delete_allowed: v
                                .get("delete_allowed")
                                .and_then(|b| b.as_bool())
                                .unwrap_or(true),
                            protected_reason: v
                                .get("protected_reason")
                                .and_then(|s| s.as_str())
                                .map(String::from),
                            reasoning_effort: v
                                .get("reasoning_effort")
                                .and_then(|s| s.as_str())
                                .map(String::from),
                            raw_json: Some(v.clone()),
                        })
                    })
                    .collect();
                let _ = event_tx.send(ClientEvent::SubAgentList(entries)).await;
            }
            DaemonMessage::AgentSubAgentUpdated { sub_agent_json } => {
                let v: serde_json::Value =
                    serde_json::from_str(&sub_agent_json).unwrap_or_default();
                let entry = crate::state::SubAgentEntry {
                    id: v
                        .get("id")
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .to_string(),
                    name: v
                        .get("name")
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .to_string(),
                    provider: v
                        .get("provider")
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .to_string(),
                    model: v
                        .get("model")
                        .and_then(|s| s.as_str())
                        .unwrap_or("")
                        .to_string(),
                    role: v.get("role").and_then(|s| s.as_str()).map(String::from),
                    enabled: v.get("enabled").and_then(|b| b.as_bool()).unwrap_or(true),
                    builtin: v.get("builtin").and_then(|b| b.as_bool()).unwrap_or(false),
                    immutable_identity: v
                        .get("immutable_identity")
                        .and_then(|b| b.as_bool())
                        .unwrap_or(false),
                    disable_allowed: v
                        .get("disable_allowed")
                        .and_then(|b| b.as_bool())
                        .unwrap_or(true),
                    delete_allowed: v
                        .get("delete_allowed")
                        .and_then(|b| b.as_bool())
                        .unwrap_or(true),
                    protected_reason: v
                        .get("protected_reason")
                        .and_then(|s| s.as_str())
                        .map(String::from),
                    reasoning_effort: v
                        .get("reasoning_effort")
                        .and_then(|s| s.as_str())
                        .map(String::from),
                    raw_json: Some(v),
                };
                let _ = event_tx.send(ClientEvent::SubAgentUpdated(entry)).await;
            }
            DaemonMessage::AgentSubAgentRemoved { sub_agent_id } => {
                let _ = event_tx
                    .send(ClientEvent::SubAgentRemoved { sub_agent_id })
                    .await;
            }
            DaemonMessage::AgentConciergeConfig { config_json } => {
                match serde_json::from_str::<Value>(&config_json) {
                    Ok(raw) => {
                        let _ = event_tx.send(ClientEvent::ConciergeConfig(raw)).await;
                    }
                    Err(err) => warn!("Failed to parse concierge config response: {}", err),
                }
            }
            // Plugin response handlers (Plan 16-03)
            DaemonMessage::PluginListResult { plugins } => {
                let _ = event_tx.send(ClientEvent::PluginList(plugins)).await;
            }
            DaemonMessage::PluginGetResult {
                plugin,
                settings_schema,
            } => {
                let _ = event_tx
                    .send(ClientEvent::PluginGet {
                        plugin,
                        settings_schema,
                    })
                    .await;
            }
            DaemonMessage::PluginSettingsResult {
                plugin_name,
                settings,
            } => {
                let _ = event_tx
                    .send(ClientEvent::PluginSettings {
                        plugin_name,
                        settings,
                    })
                    .await;
            }
            DaemonMessage::PluginTestConnectionResult {
                plugin_name,
                success,
                message,
            } => {
                let _ = event_tx
                    .send(ClientEvent::PluginTestConnection {
                        plugin_name,
                        success,
                        message,
                    })
                    .await;
            }
            DaemonMessage::PluginActionResult { success, message } => {
                let _ = event_tx
                    .send(ClientEvent::PluginAction { success, message })
                    .await;
            }
            DaemonMessage::PluginCommandsResult { commands } => {
                let _ = event_tx.send(ClientEvent::PluginCommands(commands)).await;
            }
            DaemonMessage::PluginOAuthUrl { name, url } => {
                let _ = event_tx
                    .send(ClientEvent::PluginOAuthUrl { name, url })
                    .await;
            }
            DaemonMessage::PluginOAuthComplete {
                operation_id: _,
                name,
                success,
                error,
            } => {
                let _ = event_tx
                    .send(ClientEvent::PluginOAuthComplete {
                        name,
                        success,
                        error,
                    })
                    .await;
            }
            _ => unreachable!("daemon message part2 should be exhaustive"),
        }
    }
}
