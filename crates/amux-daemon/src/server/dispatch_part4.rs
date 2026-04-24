if matches!(
        &msg,
        ClientMessage::AgentRecordAttention{ .. } |
        ClientMessage::AgentListThreads{ .. } |
        ClientMessage::AgentGetThread{ .. } |
        ClientMessage::AgentDeleteThread{ .. } |
        ClientMessage::AgentPinThreadMessageForCompaction{ .. } |
        ClientMessage::AgentUnpinThreadMessageForCompaction{ .. } |
        ClientMessage::AgentAddTask{ .. } |
        ClientMessage::AgentCancelTask{ .. } |
        ClientMessage::AgentListTasks |
        ClientMessage::AgentListRuns |
        ClientMessage::AgentGetRun{ .. } |
        ClientMessage::AgentStartGoalRun{ .. } |
        ClientMessage::AgentListGoalRuns{ .. } |
        ClientMessage::AgentGetGoalRun{ .. } |
        ClientMessage::AgentGetGoalRunPage{ .. } |
        ClientMessage::AgentControlGoalRun{ .. } |
        ClientMessage::AgentDeleteGoalRun{ .. } |
        ClientMessage::AgentListTodos |
        ClientMessage::AgentGetTodos{ .. } |
        ClientMessage::AgentGetWorkContext{ .. } |
        ClientMessage::AgentListTools{ .. } |
        ClientMessage::AgentSearchTools{ .. } |
        ClientMessage::AgentGetConfig |
        ClientMessage::AgentGetGatewayConfig |
        ClientMessage::AgentGetEffectiveConfigState |
        ClientMessage::AgentListTaskApprovalRules |
        ClientMessage::AgentCreateTaskApprovalRule{ .. } |
        ClientMessage::AgentRevokeTaskApprovalRule{ .. } |
        ClientMessage::AgentSetConfigItem{ .. } |
        ClientMessage::AgentSetProviderModel{ .. } |
        ClientMessage::AgentSetTargetAgentProviderModel{ .. } |
        ClientMessage::AgentFetchModels{ .. } |
        ClientMessage::AgentHeartbeatGetItems |
        ClientMessage::AgentHeartbeatSetItems{ .. } |
        ClientMessage::AgentResolveTaskApproval{ .. } |
        ClientMessage::AgentSubscribe |
        ClientMessage::AgentUnsubscribe |
        ClientMessage::AgentDeclareAsyncCommandCapability{ .. } |
        ClientMessage::AgentGetOperationStatus{ .. } |
        ClientMessage::AgentGetSubagentMetrics{ .. } |
        ClientMessage::AgentGetSubsystemMetrics |
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

                ClientMessage::AgentListThreads {
                    limit,
                    offset,
                    include_internal,
                } => {
                    let threads = agent
                        .list_threads_paginated(limit, offset.unwrap_or(0), include_internal)
                        .await;
                    let (threads, truncated) = cap_agent_thread_list_for_ipc(threads);
                    client_agent_threads.extend(threads.iter().map(|thread| thread.id.clone()));
                    if truncated {
                        tracing::warn!("truncated agent thread list to fit IPC frame limit");
                    }
                    let json = serde_json::to_string(&threads).unwrap_or_default();
                    tracing::debug!(
                        thread_count = threads.len(),
                        payload_bytes = json.len(),
                        "sending agent thread list to client"
                    );
                    framed
                        .send(DaemonMessage::AgentThreadList { threads_json: json })
                        .await?;
                }

                ClientMessage::AgentGetThread {
                    thread_id,
                    message_limit,
                    message_offset,
                } => {
                    client_agent_threads.insert(thread_id.clone());
                    let json = agent
                        .agent_thread_detail_json(&thread_id, message_limit, message_offset)
                        .await;
                    if thread_detail_fits_single_ipc_frame(&json) {
                        framed
                            .send(DaemonMessage::AgentThreadDetail { thread_json: json })
                            .await?;
                    } else {
                        let chunks = thread_detail_chunks_for_ipc(&json).collect::<Vec<_>>();
                        let total_chunks = chunks.len();
                        for (index, chunk) in chunks.into_iter().enumerate() {
                            framed
                                .send(DaemonMessage::AgentThreadDetailChunk {
                                    thread_id: thread_id.clone(),
                                    thread_json_chunk: chunk.to_vec(),
                                    done: index + 1 == total_chunks,
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::AgentDeleteThread { thread_id } => {
                    client_agent_threads.remove(&thread_id);
                    let deleted = agent.delete_thread(&thread_id).await;
                    framed
                        .send(DaemonMessage::AgentThreadDeleted { thread_id, deleted })
                        .await?;
                }

                ClientMessage::AgentPinThreadMessageForCompaction {
                    thread_id,
                    message_id,
                } => {
                    let result = agent
                        .pin_thread_message_for_compaction(&thread_id, &message_id)
                        .await;
                    let result_json = serde_json::to_string(&result).unwrap_or_default();
                    framed
                        .send(DaemonMessage::AgentThreadMessagePinResult { result_json })
                        .await?;
                }

                ClientMessage::AgentUnpinThreadMessageForCompaction {
                    thread_id,
                    message_id,
                } => {
                    let result = agent
                        .unpin_thread_message_for_compaction(&thread_id, &message_id)
                        .await;
                    let result_json = serde_json::to_string(&result).unwrap_or_default();
                    framed
                        .send(DaemonMessage::AgentThreadMessagePinResult { result_json })
                        .await?;
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
                    let (tasks, truncated) = agent.list_tasks_capped_for_ipc().await;
                    if truncated {
                        tracing::warn!("truncated task list to fit IPC frame limit");
                    }
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
                    launch_assignments,
                    autonomy_level,
                    client_surface,
                    requires_approval,
                } => {
                    let goal_run = agent
                        .start_goal_run_with_surface_and_approval_policy(
                            goal,
                            title,
                            thread_id,
                            session_id,
                            priority.as_deref(),
                            client_request_id,
                            autonomy_level,
                            client_surface,
                            requires_approval,
                            if launch_assignments.is_empty() {
                                None
                            } else {
                                Some(
                                    launch_assignments
                                        .into_iter()
                                        .map(|assignment| crate::agent::types::GoalAgentAssignment {
                                            role_id: assignment.role_id,
                                            enabled: assignment.enabled,
                                            provider: assignment.provider,
                                            model: assignment.model,
                                            reasoning_effort: assignment.reasoning_effort,
                                            inherit_from_main: assignment.inherit_from_main,
                                        })
                                        .collect(),
                                )
                            },
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

                ClientMessage::AgentListGoalRuns { limit, offset } => {
                    let (goal_runs, truncated) = agent
                        .list_goal_runs_paginated_capped_for_ipc(limit, offset)
                        .await;
                    if truncated {
                        tracing::warn!(
                            "truncated goal run list to fit IPC frame limit"
                        );
                    }
                    let json = serde_json::to_string(&goal_runs).unwrap_or_default();
                    framed
                        .send(DaemonMessage::AgentGoalRunList {
                            goal_runs_json: json,
                        })
                        .await?;
                }

                ClientMessage::AgentGetGoalRun { goal_run_id } => {
                    let detail = agent.get_goal_run_capped_for_ipc(&goal_run_id).await;
                    if detail.as_ref().is_some_and(|(_, truncated)| *truncated) {
                        tracing::warn!(
                            goal_run_id = %goal_run_id,
                            "truncated goal run detail to fit IPC frame limit"
                        );
                    }
                    let json = detail
                        .map(|(goal_run_json, _)| goal_run_json)
                        .unwrap_or_else(|| {
                            serde_json::json!({
                                "id": goal_run_id.clone(),
                            })
                            .to_string()
                        });
                    framed
                        .send(DaemonMessage::AgentGoalRunDetail {
                            goal_run_json: json,
                        })
                        .await?;
                }

                ClientMessage::AgentGetGoalRunPage {
                    goal_run_id,
                    step_offset,
                    step_limit,
                    event_offset,
                    event_limit,
                } => {
                    let detail = agent
                        .get_goal_run_page_capped_for_ipc(
                            &goal_run_id,
                            step_offset,
                            step_limit,
                            event_offset,
                            event_limit,
                        )
                        .await;
                    if detail.as_ref().is_some_and(|(_, truncated)| *truncated) {
                        tracing::warn!(
                            goal_run_id = %goal_run_id,
                            "truncated goal run detail page to fit IPC frame limit"
                        );
                    }
                    let json = detail
                        .map(|(goal_run_json, _)| goal_run_json)
                        .unwrap_or_else(|| {
                            serde_json::json!({
                                "id": goal_run_id.clone(),
                            })
                            .to_string()
                        });
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

                ClientMessage::AgentDeleteGoalRun { goal_run_id } => {
                    let deleted = agent.delete_goal_run(&goal_run_id).await;
                    framed
                        .send(DaemonMessage::AgentGoalRunDeleted {
                            goal_run_id,
                            deleted,
                        })
                        .await?;
                }

                ClientMessage::AgentListTodos => {
                    let (todos, truncated) = agent.list_todos_capped_for_ipc().await;
                    if truncated {
                        tracing::warn!("truncated todo list to fit IPC frame limit");
                    }
                    let json = serde_json::to_string(&todos).unwrap_or_default();
                    framed
                        .send(DaemonMessage::AgentTodoList { todos_json: json })
                        .await?;
                }

                ClientMessage::AgentGetTodos { thread_id } => {
                    client_agent_threads.insert(thread_id.clone());
                    let (todos, truncated) = agent.get_todos_capped_for_ipc(&thread_id).await;
                    if truncated {
                        tracing::warn!(
                            thread_id = %thread_id,
                            "truncated todo detail to fit IPC frame limit"
                        );
                    }
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
                    let (context, truncated) =
                        agent.get_work_context_capped_for_ipc(&thread_id).await;
                    if truncated {
                        tracing::warn!(
                            thread_id = %thread_id,
                            "truncated work context detail to fit IPC frame limit"
                        );
                    }
                    let json = serde_json::to_string(&context).unwrap_or_default();
                    framed
                        .send(DaemonMessage::AgentWorkContextDetail {
                            thread_id,
                            context_json: json,
                        })
                        .await?;
                }

                ClientMessage::AgentListTools { limit, offset } => {
                    let config = agent.config.read().await;
                    let has_workspace_topology =
                        agent.session_manager.read_workspace_topology().is_some();
                    let result = crate::agent::tool_executor::list_available_tools_public(
                        &config,
                        &agent.data_dir,
                        has_workspace_topology,
                        limit.unwrap_or(50),
                        offset.unwrap_or(0),
                    );
                    framed
                        .send(DaemonMessage::AgentToolList { result })
                        .await?;
                }

                ClientMessage::AgentSearchTools {
                    query,
                    limit,
                    offset,
                } => {
                    let config = agent.config.read().await;
                    let has_workspace_topology =
                        agent.session_manager.read_workspace_topology().is_some();
                    let result = crate::agent::tool_executor::search_available_tools_public(
                        &config,
                        &agent.data_dir,
                        has_workspace_topology,
                        &query,
                        limit.unwrap_or(20),
                        offset.unwrap_or(0),
                    );
                    framed
                        .send(DaemonMessage::AgentToolSearchResult { result })
                        .await?;
                }

                ClientMessage::AgentGetConfig => {
                    let config = agent.get_config().await;
                    let json = serde_json::to_string(&config).unwrap_or_default();
                    framed
                        .send(DaemonMessage::AgentConfigResponse { config_json: json })
                        .await?;
                }

                ClientMessage::AgentGetGatewayConfig => {
                    let gateway = agent.get_config().await.gateway;
                    let json = serde_json::to_string(&gateway).unwrap_or_default();
                    framed
                        .send(DaemonMessage::AgentGatewayConfig { config_json: json })
                        .await?;
                }

                ClientMessage::AgentGetEffectiveConfigState => {
                    let state = agent.current_effective_config_runtime_state().await;
                    let json = serde_json::to_string(&state).unwrap_or_default();
                    framed
                        .send(DaemonMessage::AgentEffectiveConfigState { state_json: json })
                        .await?;
                }

                ClientMessage::AgentGetSubsystemMetrics => {
                    let metrics_json = subsystem_metrics().all_snapshots_json();
                    framed
                        .send(DaemonMessage::AgentSubsystemMetrics { metrics_json })
                        .await?;
                }

                ClientMessage::AgentSetConfigItem {
                    key_path,
                    value_json,
                } => match agent.prepare_config_item_json(&key_path, &value_json).await {
                    Ok((merged, value)) => {
                        if !background_daemon_pending.has_capacity(BackgroundSubsystem::ConfigReconcile) {
                            background_daemon_pending.note_rejection(BackgroundSubsystem::ConfigReconcile);
                            framed
                                .send(DaemonMessage::Error {
                                    message: "config_reconcile background queue is full".to_string(),
                                })
                                .await?;
                            continue;
                        }

                        if let Err(e) = agent
                            .persist_prepared_config_item_json(&key_path, &value, merged)
                            .await
                        {
                            tracing::warn!(error = %e, key_path, "server: AgentSetConfigItem persist failed");
                            framed
                                .send(DaemonMessage::Error {
                                    message: format!("Invalid config item: {e}"),
                                })
                                .await?;
                            continue;
                        }

                        let operation = operation_registry().accept_operation(
                            OPERATION_KIND_CONFIG_SET_ITEM,
                            Some(config_set_item_dedup_key(&agent, &key_path, &value_json)),
                        );

                        framed
                            .send(DaemonMessage::OperationAccepted {
                                operation_id: operation.operation_id.clone(),
                                kind: operation.kind.clone(),
                                dedup: operation.dedup.clone(),
                                revision: operation.revision,
                            })
                            .await?;

                        let agent = agent.clone();
                        let background_daemon_tx =
                            background_daemon_queues.sender(BackgroundSubsystem::ConfigReconcile);
                        spawn_background_side_effect(
                            BackgroundSubsystem::ConfigReconcile,
                            Some(operation.operation_id.clone()),
                            background_daemon_tx,
                            &mut background_daemon_pending,
                            async move {
                                match agent.reconcile_config_runtime_after_commit().await {
                                    Ok(()) => BackgroundSideEffectOutcome::Completed,
                                    Err(_) => BackgroundSideEffectOutcome::Failed,
                                }
                            },
                        );
                    }
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
                    match agent.prepare_provider_model_json(&provider_id, &model).await {
                        Ok(merged) => {
                            if !background_daemon_pending.has_capacity(BackgroundSubsystem::ConfigReconcile) {
                                background_daemon_pending.note_rejection(BackgroundSubsystem::ConfigReconcile);
                                framed
                                    .send(DaemonMessage::Error {
                                        message: "config_reconcile background queue is full".to_string(),
                                    })
                                    .await?;
                                continue;
                            }

                            agent.persist_prepared_provider_model_json(merged).await;

                            let operation = operation_registry().accept_operation(
                                OPERATION_KIND_SET_PROVIDER_MODEL,
                                Some(set_provider_model_dedup_key(
                                    &agent,
                                    &provider_id,
                                    &model,
                                )),
                            );

                            framed
                                .send(DaemonMessage::OperationAccepted {
                                    operation_id: operation.operation_id.clone(),
                                    kind: operation.kind.clone(),
                                    dedup: operation.dedup.clone(),
                                    revision: operation.revision,
                                })
                                .await?;

                            let agent = agent.clone();
                            let background_daemon_tx = background_daemon_queues
                                .sender(BackgroundSubsystem::ConfigReconcile);
                            spawn_background_side_effect(
                                BackgroundSubsystem::ConfigReconcile,
                                Some(operation.operation_id.clone()),
                                background_daemon_tx,
                                &mut background_daemon_pending,
                                async move {
                                    match agent.reconcile_config_runtime_after_commit().await {
                                        Ok(()) => BackgroundSideEffectOutcome::Completed,
                                        Err(_) => BackgroundSideEffectOutcome::Failed,
                                    }
                                },
                            );
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, provider_id, model, "server: AgentSetProviderModel rejected");
                            framed
                                .send(DaemonMessage::Error {
                                    message: format!("Invalid provider/model selection: {e}"),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::AgentSetTargetAgentProviderModel {
                    target_agent_id,
                    provider_id,
                    model,
                } => {
                    match agent
                        .prepare_agent_provider_model_json(&target_agent_id, &provider_id, &model)
                        .await
                    {
                        Ok(merged) => {
                            if !background_daemon_pending
                                .has_capacity(BackgroundSubsystem::ConfigReconcile)
                            {
                                background_daemon_pending
                                    .note_rejection(BackgroundSubsystem::ConfigReconcile);
                                framed
                                    .send(DaemonMessage::Error {
                                        message: "config_reconcile background queue is full"
                                            .to_string(),
                                    })
                                    .await?;
                                continue;
                            }

                            agent.persist_prepared_provider_model_json(merged).await;

                            let operation = operation_registry().accept_operation(
                                OPERATION_KIND_SET_PROVIDER_MODEL,
                                Some(set_target_agent_provider_model_dedup_key(
                                    &agent,
                                    &target_agent_id,
                                    &provider_id,
                                    &model,
                                )),
                            );

                            framed
                                .send(DaemonMessage::OperationAccepted {
                                    operation_id: operation.operation_id.clone(),
                                    kind: operation.kind.clone(),
                                    dedup: operation.dedup.clone(),
                                    revision: operation.revision,
                                })
                                .await?;

                            let agent = agent.clone();
                            let background_daemon_tx = background_daemon_queues
                                .sender(BackgroundSubsystem::ConfigReconcile);
                            spawn_background_side_effect(
                                BackgroundSubsystem::ConfigReconcile,
                                Some(operation.operation_id.clone()),
                                background_daemon_tx,
                                &mut background_daemon_pending,
                                async move {
                                    match agent.reconcile_config_runtime_after_commit().await {
                                        Ok(()) => BackgroundSideEffectOutcome::Completed,
                                        Err(_) => BackgroundSideEffectOutcome::Failed,
                                    }
                                },
                            );
                        }
                        Err(e) => {
                            tracing::warn!(
                                error = %e,
                                target_agent_id,
                                provider_id,
                                model,
                                "server: AgentSetTargetAgentProviderModel rejected"
                            );
                            framed
                                .send(DaemonMessage::Error {
                                    message: format!(
                                        "Invalid target agent provider/model selection: {e}"
                                    ),
                                })
                                .await?;
                        }
                    }
                },

                ClientMessage::AgentFetchModels {
                    provider_id,
                    base_url,
                    api_key,
                    output_modalities,
                } => {
                    let _ = crate::agent::types::reload_custom_provider_catalog_from_default_path();
                    let (resolved_url, resolved_key) = {
                        let config = agent.config.read().await;
                        let url = if base_url.is_empty() {
                            config
                                .providers
                                .get(&provider_id)
                                .map(|pc| pc.base_url.clone())
                                .filter(|value| !value.is_empty())
                                .or_else(|| {
                                    crate::agent::types::custom_provider_config(&provider_id)
                                        .map(|pc| pc.base_url)
                                })
                                .unwrap_or_default()
                        } else {
                            base_url
                        };
                        let key = if api_key.is_empty() {
                            config
                                .providers
                                .get(&provider_id)
                                .map(|pc| pc.api_key.clone())
                                .filter(|value| !value.is_empty())
                                .or_else(|| {
                                    crate::agent::types::custom_provider_config(&provider_id)
                                        .map(|pc| pc.api_key)
                                })
                                .unwrap_or_default()
                        } else {
                            api_key
                        };
                        (url, key)
                    };
                    if !background_daemon_pending.has_capacity(BackgroundSubsystem::ProviderIo) {
                        background_daemon_pending.note_rejection(BackgroundSubsystem::ProviderIo);
                        framed
                            .send(DaemonMessage::Error {
                                message: "provider_io background queue is full".to_string(),
                            })
                            .await?;
                        continue;
                    }

                    let operation = operation_registry().accept_operation(
                        OPERATION_KIND_FETCH_MODELS,
                        Some(fetch_models_dedup_key(
                            &agent,
                            &provider_id,
                            output_modalities.as_deref(),
                        )),
                    );

                    framed
                        .send(DaemonMessage::OperationAccepted {
                            operation_id: operation.operation_id.clone(),
                            kind: operation.kind.clone(),
                            dedup: operation.dedup.clone(),
                            revision: operation.revision,
                        })
                        .await?;

                    let operation_id = Some(operation.operation_id.clone());
                    let result_operation_id = operation_id.clone();
                    let background_daemon_tx =
                        background_daemon_queues.sender(BackgroundSubsystem::ProviderIo);
                    spawn_background_operation(
                        BackgroundSubsystem::ProviderIo,
                        operation_id,
                        background_daemon_tx,
                        &mut background_daemon_pending,
                        async move {
                            let result = crate::agent::llm_client::fetch_models(
                                &provider_id,
                                &resolved_url,
                                &resolved_key,
                                output_modalities.as_deref(),
                            )
                            .await;

                            let daemon_msg = match result {
                                Ok(models) => {
                                    let json = serde_json::to_string(&models).unwrap_or_default();
                                    DaemonMessage::AgentModelsResponse {
                                        operation_id: result_operation_id.clone(),
                                        models_json: json,
                                    }
                                }
                                Err(e) => DaemonMessage::AgentError {
                                    message: e.to_string(),
                                },
                            };

                            BackgroundOperationOutput::Completed(daemon_msg)
                        },
                    );
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

                ClientMessage::AgentListTaskApprovalRules => {
                    let rules = agent.list_task_approval_rules().await;
                    tracing::debug!(
                        rule_count = rules.len(),
                        "sending task approval rules to client"
                    );
                    framed
                        .send(DaemonMessage::AgentTaskApprovalRules { rules })
                        .await?;
                }

                ClientMessage::AgentCreateTaskApprovalRule { approval_id } => {
                    match agent.create_task_approval_rule_from_pending(&approval_id).await {
                        Ok(Some(_)) => {
                            let rules = agent.list_task_approval_rules().await;
                            tracing::debug!(
                                rule_count = rules.len(),
                                "sending task approval rules to client after create"
                            );
                            framed
                                .send(DaemonMessage::AgentTaskApprovalRules { rules })
                                .await?;
                        }
                        Ok(None) => {
                            framed
                                .send(DaemonMessage::AgentError {
                                    message: format!(
                                        "could not create approval rule: no live approval found for {approval_id}"
                                    ),
                                })
                                .await?;
                        }
                        Err(error) => {
                            framed
                                .send(DaemonMessage::AgentError {
                                    message: format!(
                                        "failed to create approval rule from {approval_id}: {error}"
                                    ),
                                })
                                .await?;
                        }
                    }
                }

                ClientMessage::AgentRevokeTaskApprovalRule { rule_id } => {
                    if !agent.revoke_task_approval_rule(&rule_id).await {
                        framed
                            .send(DaemonMessage::AgentError {
                                message: format!(
                                    "could not revoke approval rule: {rule_id} was not found"
                                ),
                            })
                            .await?;
                    }
                    let rules = agent.list_task_approval_rules().await;
                    tracing::debug!(
                        rule_count = rules.len(),
                        "sending task approval rules to client after revoke"
                    );
                    framed
                        .send(DaemonMessage::AgentTaskApprovalRules { rules })
                        .await?;
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
                    framed
                        .send(DaemonMessage::ApprovalResolved {
                            id: uuid::Uuid::nil(),
                            approval_id,
                            decision,
                        })
                        .await?;
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
                    framed
                        .send(DaemonMessage::AgentAsyncCommandCapabilityAck { capability })
                        .await?;
                }

                ClientMessage::AgentGetOperationStatus { operation_id } => {
                    if let Some(status) = agent
                        .session_manager
                        .get_background_task_status(&operation_id)
                        .await?
                    {
                        let snapshot = amux_protocol::OperationStatusSnapshot {
                            operation_id: status.background_task_id,
                            kind: status.kind,
                            dedup: None,
                            state: match status.state {
                                crate::session_manager::BackgroundTaskState::Queued => {
                                    amux_protocol::OperationLifecycleState::Accepted
                                }
                                crate::session_manager::BackgroundTaskState::Running => {
                                    amux_protocol::OperationLifecycleState::Started
                                }
                                crate::session_manager::BackgroundTaskState::Completed => {
                                    amux_protocol::OperationLifecycleState::Completed
                                }
                                crate::session_manager::BackgroundTaskState::Failed => {
                                    amux_protocol::OperationLifecycleState::Failed
                                }
                            },
                            revision: 0,
                        };
                        framed
                            .send(DaemonMessage::OperationStatus { snapshot })
                            .await?;
                    } else if let Some(snapshot) = operation_registry().snapshot(&operation_id) {
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
                        .send(DaemonMessage::AgentCheckpointList {
                            goal_run_id,
                            checkpoints_json,
                        })
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
