if matches!(
        &msg,
        ClientMessage::AgentRecordAttention{ .. } |
        ClientMessage::AgentListThreads |
        ClientMessage::AgentGetThread{ .. } |
        ClientMessage::AgentDeleteThread{ .. } |
        ClientMessage::AgentAddTask{ .. } |
        ClientMessage::AgentCancelTask{ .. } |
        ClientMessage::AgentListTasks |
        ClientMessage::AgentListRuns |
        ClientMessage::AgentGetRun{ .. } |
        ClientMessage::AgentStartGoalRun{ .. } |
        ClientMessage::AgentListGoalRuns |
        ClientMessage::AgentGetGoalRun{ .. } |
        ClientMessage::AgentControlGoalRun{ .. } |
        ClientMessage::AgentListTodos |
        ClientMessage::AgentGetTodos{ .. } |
        ClientMessage::AgentGetWorkContext{ .. } |
        ClientMessage::AgentGetConfig |
        ClientMessage::AgentSetConfigItem{ .. } |
        ClientMessage::AgentSetProviderModel{ .. } |
        ClientMessage::AgentFetchModels{ .. } |
        ClientMessage::AgentHeartbeatGetItems |
        ClientMessage::AgentHeartbeatSetItems{ .. } |
        ClientMessage::AgentResolveTaskApproval{ .. } |
        ClientMessage::AgentSubscribe |
        ClientMessage::AgentUnsubscribe |
        ClientMessage::AgentDeclareAsyncCommandCapability{ .. } |
        ClientMessage::AgentGetOperationStatus{ .. } |
        ClientMessage::AgentGetSubagentMetrics{ .. } |
        ClientMessage::AgentListCheckpoints{ .. } |
        ClientMessage::AgentRestoreCheckpoint{ .. } |
        ClientMessage::AgentGetHealthStatus
    ) {
        match msg {
                ClientMessage::AgentRecordAttention {
                    surface,
                    thread_id,
                    goal_run_id,
                } => {
                    if let Err(e) = agent
                        .record_operator_attention(
                            &surface,
                            thread_id.as_deref(),
                            goal_run_id.as_deref(),
                        )
                        .await
                    {
                        framed
                            .send(DaemonMessage::AgentError {
                                message: format!("failed to record attention surface: {e}"),
                            })
                            .await
                            .ok();
                    }
                }

                ClientMessage::AgentListThreads => {
                    let threads = agent.list_threads().await;
                    let json = serde_json::to_string(&threads).unwrap_or_default();
                    framed
                        .send(DaemonMessage::AgentThreadList { threads_json: json })
                        .await?;
                }

                ClientMessage::AgentGetThread { thread_id } => {
                    client_agent_threads.insert(thread_id.clone());
                    let thread = agent.get_thread(&thread_id).await;
                    let json = serde_json::to_string(&thread).unwrap_or_default();
                    framed
                        .send(DaemonMessage::AgentThreadDetail { thread_json: json })
                        .await?;
                }

                ClientMessage::AgentDeleteThread { thread_id } => {
                    client_agent_threads.remove(&thread_id);
                    agent.delete_thread(&thread_id).await;
                }

                ClientMessage::AgentAddTask {
                    title,
                    description,
                    priority,
                    command,
                    session_id,
                    scheduled_at,
                    dependencies,
                } => {
                    let task = agent
                        .enqueue_task(
                            title,
                            description,
                            &priority,
                            command,
                            session_id,
                            dependencies,
                            scheduled_at,
                            "user",
                            None,
                            None,
                            None,
                            None,
                        )
                        .await;
                    tracing::info!(task_id = %task.id, "agent task added");
                    let json = serde_json::to_string(&task).unwrap_or_default();
                    framed
                        .send(DaemonMessage::AgentTaskEnqueued { task_json: json })
                        .await?;
                }

                ClientMessage::AgentCancelTask { task_id } => {
                    let cancelled = agent.cancel_task(&task_id).await;
                    framed
                        .send(DaemonMessage::AgentTaskCancelled { task_id, cancelled })
                        .await?;
                }

                ClientMessage::AgentListTasks => {
                    let tasks = agent.list_tasks().await;
                    let json = serde_json::to_string(&tasks).unwrap_or_default();
                    framed
                        .send(DaemonMessage::AgentTaskList { tasks_json: json })
                        .await?;
                }

                ClientMessage::AgentListRuns => {
                    let runs = agent.list_runs().await;
                    let json = serde_json::to_string(&runs).unwrap_or_default();
                    framed
                        .send(DaemonMessage::AgentRunList { runs_json: json })
                        .await?;
                }

                ClientMessage::AgentGetRun { run_id } => {
                    let run = agent.get_run(&run_id).await;
                    let json = serde_json::to_string(&run).unwrap_or_default();
                    framed
                        .send(DaemonMessage::AgentRunDetail { run_json: json })
                        .await?;
                }

                ClientMessage::AgentStartGoalRun {
                    goal,
                    title,
                    thread_id,
                    session_id,
                    priority,
                    client_request_id,
                    autonomy_level,
                } => {
                    let goal_run = agent
                        .start_goal_run(
                            goal,
                            title,
                            thread_id,
                            session_id,
                            priority.as_deref(),
                            client_request_id,
                            autonomy_level,
                        )
                        .await;
                    if let Some(thread_id) = goal_run.thread_id.clone() {
                        client_agent_threads.insert(thread_id);
                    }
                    let json = serde_json::to_string(&goal_run).unwrap_or_default();
                    framed
                        .send(DaemonMessage::AgentGoalRunStarted {
                            goal_run_json: json,
                        })
                        .await?;
                }

                ClientMessage::AgentListGoalRuns => {
                    let goal_runs = agent.list_goal_runs().await;
                    let json = serde_json::to_string(&goal_runs).unwrap_or_default();
                    framed
                        .send(DaemonMessage::AgentGoalRunList {
                            goal_runs_json: json,
                        })
                        .await?;
                }

                ClientMessage::AgentGetGoalRun { goal_run_id } => {
                    let goal_run = agent.get_goal_run(&goal_run_id).await;
                    let json = serde_json::to_string(&goal_run).unwrap_or_default();
                    framed
                        .send(DaemonMessage::AgentGoalRunDetail {
                            goal_run_json: json,
                        })
                        .await?;
                }

                ClientMessage::AgentControlGoalRun {
                    goal_run_id,
                    action,
                    step_index,
                } => {
                    let ok = agent
                        .control_goal_run(&goal_run_id, &action, step_index)
                        .await;
                    framed
                        .send(DaemonMessage::AgentGoalRunControlled { goal_run_id, ok })
                        .await?;
                }

                ClientMessage::AgentListTodos => {
                    let todos = agent.list_todos().await;
                    let json = serde_json::to_string(&todos).unwrap_or_default();
                    framed
                        .send(DaemonMessage::AgentTodoList { todos_json: json })
                        .await?;
                }

                ClientMessage::AgentGetTodos { thread_id } => {
                    client_agent_threads.insert(thread_id.clone());
                    let todos = agent.get_todos(&thread_id).await;
                    let json = serde_json::to_string(&todos).unwrap_or_default();
                    framed
                        .send(DaemonMessage::AgentTodoDetail {
                            thread_id,
                            todos_json: json,
                        })
                        .await?;
                }

                ClientMessage::AgentGetWorkContext { thread_id } => {
                    client_agent_threads.insert(thread_id.clone());
                    let context = agent.get_work_context(&thread_id).await;
                    let json = serde_json::to_string(&context).unwrap_or_default();
                    framed
                        .send(DaemonMessage::AgentWorkContextDetail {
                            thread_id,
                            context_json: json,
                        })
                        .await?;
                }

                ClientMessage::AgentGetConfig => {
                    let config = agent.get_config().await;
                    let json = serde_json::to_string(&config).unwrap_or_default();
                    framed
                        .send(DaemonMessage::AgentConfigResponse { config_json: json })
                        .await?;
                }

                ClientMessage::AgentSetConfigItem {
                    key_path,
                    value_json,
                } => match agent.set_config_item_json(&key_path, &value_json).await {
                    Ok(_) => {}
                    Err(e) => {
                        tracing::warn!(error = %e, key_path, "server: AgentSetConfigItem rejected");
                        framed
                            .send(DaemonMessage::Error {
                                message: format!("Invalid config item: {e}"),
                            })
                            .await?;
                    }
                },

                ClientMessage::AgentSetProviderModel { provider_id, model } => {
                    match agent.set_provider_model_json(&provider_id, &model).await {
                        Ok(_) => {}
                        Err(e) => {
                            tracing::warn!(
                                error = %e,
                                provider_id,
                                model,
                                "server: AgentSetProviderModel rejected"
                            );
                            framed
                                .send(DaemonMessage::Error {
                                    message: format!("Invalid provider/model selection: {e}"),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::AgentFetchModels {
                    provider_id,
                    base_url,
                    api_key,
                } => {
                    let operation = async_command_capability
                        .as_ref()
                        .filter(|capability| capability.version >= 1 && capability.supports_operation_acceptance)
                        .map(|_| {
                            operation_registry().accept_operation(
                                OPERATION_KIND_FETCH_MODELS,
                                Some(fetch_models_dedup_key(&agent, &provider_id)),
                            )
                        });

                    if let Some(operation) = operation.as_ref() {
                        framed
                            .send(DaemonMessage::OperationAccepted {
                                operation_id: operation.operation_id.clone(),
                                kind: operation.kind.clone(),
                                dedup: operation.dedup.clone(),
                                revision: operation.revision,
                            })
                            .await?;
                    }

                    let operation_id = operation.map(|record| record.operation_id);
                    let background_daemon_tx = background_daemon_tx.clone();
                    background_daemon_pending = background_daemon_pending.saturating_add(1);
                    tokio::spawn(async move {
                        if let Some(operation_id) = operation_id.as_deref() {
                            operation_registry().mark_started(operation_id);
                        }

                        let result = crate::agent::llm_client::fetch_models(
                            &provider_id,
                            &base_url,
                            &api_key,
                        )
                        .await;

                        let daemon_msg = match result {
                            Ok(models) => {
                                let json = serde_json::to_string(&models).unwrap_or_default();
                                DaemonMessage::AgentModelsResponse { models_json: json }
                            }
                            Err(e) => DaemonMessage::AgentError {
                                message: e.to_string(),
                            },
                        };

                        let _ = background_daemon_tx.send(daemon_msg);

                        if let Some(operation_id) = operation_id.as_deref() {
                            operation_registry().mark_completed(operation_id);
                        }
                    });
                }

                ClientMessage::AgentHeartbeatGetItems => {
                    let items = agent.get_heartbeat_items().await;
                    let json = serde_json::to_string(&items).unwrap_or_default();
                    framed
                        .send(DaemonMessage::AgentHeartbeatItems { items_json: json })
                        .await?;
                }

                ClientMessage::AgentHeartbeatSetItems { items_json } => {
                    match serde_json::from_str(&items_json) {
                        Ok(items) => agent.set_heartbeat_items(items).await,
                        Err(e) => {
                            framed
                                .send(DaemonMessage::Error {
                                    message: format!("Invalid heartbeat items: {e}"),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::AgentResolveTaskApproval {
                    approval_id,
                    decision,
                } => {
                    let decision = match decision.as_str() {
                        "approve-session" => amux_protocol::ApprovalDecision::ApproveSession,
                        "deny" | "denied" => amux_protocol::ApprovalDecision::Deny,
                        _ => amux_protocol::ApprovalDecision::ApproveOnce,
                    };
                    tracing::info!(%approval_id, ?decision, "resolving task approval");
                    let _ = agent
                        .record_operator_approval_resolution(&approval_id, decision)
                        .await;
                    agent
                        .handle_task_approval_resolution(&approval_id, decision)
                        .await;
                }

                ClientMessage::AgentSubscribe => {
                    agent_event_rx = Some(agent.subscribe());
                    tracing::info!("client subscribed to agent events");
                    agent.mark_operator_present("client_subscribe").await;
                    agent.run_anticipatory_tick().await;
                    agent.emit_anticipatory_snapshot().await;
                }

                ClientMessage::AgentUnsubscribe => {
                    agent_event_rx = None;
                    tracing::info!("client unsubscribed from agent events");
                }

                ClientMessage::AgentDeclareAsyncCommandCapability { capability } => {
                    async_command_capability = Some(capability.clone());
                    framed
                        .send(DaemonMessage::AgentAsyncCommandCapabilityAck { capability })
                        .await?;
                }

                ClientMessage::AgentGetOperationStatus { operation_id } => {
                    if let Some(snapshot) = operation_registry().snapshot(&operation_id) {
                        framed
                            .send(DaemonMessage::OperationStatus { snapshot })
                            .await?;
                    } else {
                        framed
                            .send(DaemonMessage::AgentError {
                                message: format!("unknown operation id: {operation_id}"),
                            })
                            .await?;
                    }
                }

                ClientMessage::AgentGetSubagentMetrics { task_id } => {
                    let metrics_json = match agent.history.get_subagent_metrics(&task_id).await {
                        Ok(Some(metrics)) => serde_json::to_string(&serde_json::json!({
                            "task_id": metrics.task_id,
                            "parent_task_id": metrics.parent_task_id,
                            "thread_id": metrics.thread_id,
                            "tool_calls_total": metrics.tool_calls_total,
                            "tool_calls_succeeded": metrics.tool_calls_succeeded,
                            "tool_calls_failed": metrics.tool_calls_failed,
                            "tokens_consumed": metrics.tokens_consumed,
                            "context_budget_tokens": metrics.context_budget_tokens,
                            "progress_rate": metrics.progress_rate,
                            "last_progress_at": metrics.last_progress_at,
                            "stuck_score": metrics.stuck_score,
                            "health_state": metrics.health_state,
                            "created_at": metrics.created_at,
                            "updated_at": metrics.updated_at,
                        }))
                        .unwrap_or_else(|_| "null".to_string()),
                        Ok(None) => "null".to_string(),
                        Err(e) => {
                            framed
                                .send(DaemonMessage::AgentError {
                                    message: format!("failed to fetch subagent metrics: {e}"),
                                })
                                .await
                                .ok();
                            continue;
                        }
                    };
                    framed
                        .send(DaemonMessage::AgentSubagentMetrics { metrics_json })
                        .await
                        .ok();
                }

                ClientMessage::AgentListCheckpoints { goal_run_id } => {
                    let checkpoints_json = match agent
                        .history
                        .list_checkpoints_for_goal_run(&goal_run_id)
                        .await
                    {
                        Ok(jsons) => {
                            let summaries =
                                crate::agent::liveness::checkpoint::checkpoint_list(&jsons);
                            serde_json::to_string(&summaries).unwrap_or_else(|_| "[]".into())
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::AgentError {
                                    message: format!("failed to list checkpoints: {e}"),
                                })
                                .await
                                .ok();
                            continue;
                        }
                    };
                    framed
                        .send(DaemonMessage::AgentCheckpointList { checkpoints_json })
                        .await
                        .ok();
                }

                ClientMessage::AgentRestoreCheckpoint { checkpoint_id } => {
                    let outcome_json = match agent.restore_checkpoint(&checkpoint_id).await {
                        Ok(outcome) => {
                            serde_json::to_string(&outcome).unwrap_or_else(|_| "null".into())
                        }
                        Err(e) => {
                            framed
                                .send(DaemonMessage::AgentError {
                                    message: format!("failed to restore checkpoint: {e}"),
                                })
                                .await
                                .ok();
                            continue;
                        }
                    };
                    framed
                        .send(DaemonMessage::AgentCheckpointRestored { outcome_json })
                        .await
                        .ok();
                }

                ClientMessage::AgentGetHealthStatus => {
                    let status_json = agent.health_status_snapshot().await.to_string();
                    framed
                        .send(DaemonMessage::AgentHealthStatus { status_json })
                        .await
                        .ok();
                }

            _ => unreachable!("message chunk should be exhaustive"),
        }
        continue;
    }
