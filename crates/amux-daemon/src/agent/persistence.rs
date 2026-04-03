//! State persistence — hydration from disk/SQLite and serialization helpers.

use super::*;

const WELES_RUNTIME_CONTEXT_PREFIX: &str = "weles_runtime_context:";

pub(super) fn weles_runtime_context_key(task_id: &str) -> String {
    format!("{WELES_RUNTIME_CONTEXT_PREFIX}{task_id}")
}

pub(super) fn sanitize_task_for_external_view(task: &mut AgentTask) {
    if task.sub_agent_def_id.as_deref()
        == Some(crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID)
    {
        if let Some(prompt) = task.override_system_prompt.as_deref() {
            let sanitized =
                crate::agent::weles_governance::strip_weles_internal_payload_markers(prompt);
            task.override_system_prompt = if sanitized.trim().is_empty() {
                None
            } else {
                Some(sanitized)
            };
        }
    }
}

async fn restore_weles_runtime_context(engine: &AgentEngine, task: &mut AgentTask) {
    if task.sub_agent_def_id.as_deref()
        != Some(crate::agent::agent_identity::WELES_BUILTIN_SUBAGENT_ID)
    {
        return;
    }

    let Some(prompt) = task.override_system_prompt.as_deref() else {
        return;
    };
    if crate::agent::weles_governance::parse_weles_internal_override_payload(prompt).is_some() {
        return;
    }

    let scope = if prompt.contains("Your current internal scope is vitality.") {
        Some(crate::agent::agent_identity::WELES_VITALITY_SCOPE)
    } else if prompt.contains("Your current internal scope is governance.") {
        Some(crate::agent::agent_identity::WELES_GOVERNANCE_SCOPE)
    } else {
        None
    };
    let Some(scope) = scope else {
        return;
    };

    let key = weles_runtime_context_key(&task.id);
    let Ok(Some(raw_context)) = engine.history.get_consolidation_state(&key).await else {
        return;
    };
    let Ok(inspection_context) = serde_json::from_str::<serde_json::Value>(&raw_context) else {
        tracing::warn!(task_id = %task.id, "failed to parse persisted WELES runtime context");
        return;
    };
    let Some(internal_payload) =
        crate::agent::weles_governance::build_weles_internal_override_payload(
            scope,
            &inspection_context,
        )
    else {
        tracing::warn!(task_id = %task.id, "failed to rebuild WELES runtime payload from persisted context");
        return;
    };
    task.override_system_prompt = Some(format!("{prompt}\n\n{internal_payload}"));
    engine
        .trusted_weles_tasks
        .write()
        .await
        .insert(task.id.clone());
}

mod save;

impl AgentEngine {
    // -----------------------------------------------------------------------
    // Lifecycle
    // -----------------------------------------------------------------------

    /// Load persisted state (threads, tasks, heartbeat, memory, config).
    pub async fn hydrate(&self) -> Result<()> {
        // Load config from SQLite-backed config items.
        match self.history.list_agent_config_items().await {
            Ok(items) if !items.is_empty() => {
                match super::config::load_config_from_items_with_weles_cleanup(items) {
                    Ok((cfg, collisions)) => {
                        self.persist_sanitized_config(cfg, collisions).await;
                    }
                    Err(error) => {
                        tracing::warn!("failed to load agent config from sqlite: {error}")
                    }
                }
            }
            Ok(_) => {}
            Err(error) => tracing::warn!("failed to read agent config items from sqlite: {error}"),
        }

        // Load threads
        match self.history.list_threads().await {
            Ok(thread_rows) if !thread_rows.is_empty() => {
                let mut threads = HashMap::new();
                let mut handoff_states = HashMap::new();
                let mut thread_client_surfaces = HashMap::new();
                for thread_row in thread_rows {
                    let thread_id = thread_row.id.clone();
                    let thread_title = thread_row.title.clone();
                    let thread_metadata =
                        parse_thread_metadata(thread_row.metadata_json.as_deref());
                    if let Some(client_surface) = thread_metadata.client_surface {
                        thread_client_surfaces.insert(thread_id.clone(), client_surface);
                    }
                    let handoff_state = normalized_thread_handoff_state(
                        &thread_id,
                        thread_row.agent_name.as_deref(),
                        thread_row.created_at as u64,
                        thread_metadata.handoff_state,
                    );
                    let messages = self
                        .history
                        .list_messages(&thread_id, None)
                        .await
                        .unwrap_or_default()
                        .into_iter()
                        .map(|message| {
                            let metadata = parse_message_metadata(message.metadata_json.as_deref());
                            AgentMessage {
                                id: message.id.clone(),
                                role: match message.role.as_str() {
                                    "system" => MessageRole::System,
                                    "assistant" => MessageRole::Assistant,
                                    "tool" => MessageRole::Tool,
                                    _ => MessageRole::User,
                                },
                                content: message.content,
                                tool_calls: message
                                    .tool_calls_json
                                    .as_deref()
                                    .and_then(|json| serde_json::from_str(json).ok()),
                                tool_call_id: metadata.tool_call_id,
                                tool_name: metadata.tool_name,
                                tool_arguments: metadata.tool_arguments,
                                tool_status: metadata.tool_status,
                                weles_review: metadata.weles_review,
                                input_tokens: message.input_tokens.unwrap_or(0) as u64,
                                output_tokens: message.output_tokens.unwrap_or(0) as u64,
                                provider: message.provider,
                                model: message.model,
                                api_transport: metadata.api_transport,
                                response_id: metadata.response_id,
                                reasoning: message.reasoning,
                                message_kind: metadata.message_kind,
                                compaction_strategy: metadata.compaction_strategy,
                                compaction_payload: metadata.compaction_payload,
                                timestamp: message.created_at as u64,
                            }
                        })
                        .collect::<Vec<_>>();

                    threads.insert(
                        thread_id.clone(),
                        AgentThread {
                            id: thread_id.clone(),
                            agent_name: Some(
                                canonical_agent_name(&handoff_state.active_agent_id).to_string(),
                            ),
                            title: thread_title,
                            messages,
                            pinned: false,
                            upstream_thread_id: thread_metadata.upstream_thread_id,
                            upstream_transport: thread_metadata.upstream_transport,
                            upstream_provider: thread_metadata.upstream_provider,
                            upstream_model: thread_metadata.upstream_model,
                            upstream_assistant_id: thread_metadata.upstream_assistant_id,
                            created_at: thread_row.created_at as u64,
                            updated_at: thread_row.updated_at as u64,
                            total_input_tokens: 0,
                            total_output_tokens: 0,
                        },
                    );
                    handoff_states.insert(thread_id, handoff_state);
                }
                *self.threads.write().await = threads;
                *self.thread_handoff_states.write().await = handoff_states;
                *self.thread_client_surfaces.write().await = thread_client_surfaces;
            }
            Ok(_) => {}
            Err(e) => tracing::warn!("failed to load agent threads from sqlite: {e}"),
        }

        // Load AJQ tasks from SQLite first; fall back to legacy JSON migration.
        match self.history.list_agent_tasks().await {
            Ok(mut tasks) if !tasks.is_empty() => {
                for task in &mut tasks {
                    sanitize_task_for_external_view(task);
                    restore_weles_runtime_context(self, task).await;
                    if task.status == TaskStatus::InProgress {
                        task.status = TaskStatus::Queued;
                        task.started_at = None;
                        task.lane_id = None;
                        task.logs.push(make_task_log_entry(
                            task.retry_count,
                            TaskLogLevel::Warn,
                            "hydrate",
                            "daemon restarted while task was in progress; task re-queued",
                            None,
                        ));
                    }
                }
                *self.tasks.lock().await = tasks.into_iter().collect();
                self.persist_tasks().await;
            }
            Ok(_) => {
                let tasks_path = self.data_dir.join("tasks.json");
                if tasks_path.exists() {
                    match tokio::fs::read_to_string(&tasks_path).await {
                        Ok(raw) => {
                            if let Ok(mut tasks) = serde_json::from_str::<VecDeque<AgentTask>>(&raw)
                            {
                                for task in tasks.iter_mut() {
                                    sanitize_task_for_external_view(task);
                                    restore_weles_runtime_context(self, task).await;
                                    if task.status == TaskStatus::InProgress {
                                        task.status = TaskStatus::Queued;
                                        task.started_at = None;
                                    }
                                    task.max_retries = task.max_retries.max(1);
                                }
                                *self.tasks.lock().await = tasks;
                                self.persist_tasks().await;
                            }
                        }
                        Err(e) => tracing::warn!("failed to migrate legacy agent tasks: {e}"),
                    }
                }
            }
            Err(e) => tracing::warn!("failed to load agent tasks from sqlite: {e}"),
        }

        match self.history.list_goal_runs().await {
            Ok(goal_runs) if !goal_runs.is_empty() => {
                let mut runs: VecDeque<GoalRun> = goal_runs.into_iter().collect();
                let mut paused_count = 0;

                // D-11: Mark interrupted goal runs as Paused on restart.
                for goal_run in runs.iter_mut() {
                    if matches!(
                        goal_run.status,
                        GoalRunStatus::Running | GoalRunStatus::Planning
                    ) {
                        goal_run.status = GoalRunStatus::Paused;
                        goal_run.events.push(GoalRunEvent {
                            id: uuid::Uuid::new_v4().to_string(),
                            timestamp: now_millis(),
                            phase: "restart".to_string(),
                            message: "Daemon restarted; goal run paused for operator review."
                                .to_string(),
                            details: None,
                            step_index: None,
                            todo_snapshot: Vec::new(),
                        });
                        paused_count += 1;
                    }
                }

                if paused_count > 0 {
                    tracing::info!(
                        paused_count,
                        "paused interrupted goal runs on restart (D-11)"
                    );
                }

                *self.goal_runs.lock().await = runs;
                // Persist the paused status back to SQLite immediately
                self.persist_goal_runs().await;
            }
            Ok(_) => {
                let goal_runs_path = self.data_dir.join("goal-runs.json");
                if goal_runs_path.exists() {
                    match tokio::fs::read_to_string(&goal_runs_path).await {
                        Ok(raw) => {
                            if let Ok(goal_runs) = serde_json::from_str::<VecDeque<GoalRun>>(&raw) {
                                *self.goal_runs.lock().await = goal_runs;
                                self.persist_goal_runs().await;
                            }
                        }
                        Err(e) => tracing::warn!("failed to migrate legacy goal runs: {e}"),
                    }
                }
            }
            Err(e) => tracing::warn!("failed to load goal runs from sqlite: {e}"),
        }

        let todos_path = self.data_dir.join("todos.json");
        if todos_path.exists() {
            match tokio::fs::read_to_string(&todos_path).await {
                Ok(raw) => {
                    if let Ok(items) = serde_json::from_str::<HashMap<String, Vec<TodoItem>>>(&raw)
                    {
                        *self.thread_todos.write().await = items;
                    }
                }
                Err(e) => tracing::warn!("failed to load thread todos: {e}"),
            }
        }

        let work_context_path = self.data_dir.join("work-context.json");
        if work_context_path.exists() {
            match tokio::fs::read_to_string(&work_context_path).await {
                Ok(raw) => {
                    if let Ok(items) =
                        serde_json::from_str::<HashMap<String, ThreadWorkContext>>(&raw)
                    {
                        *self.thread_work_contexts.write().await = items;
                    }
                }
                Err(e) => tracing::warn!("failed to load thread work context: {e}"),
            }
        }

        match self.history.list_gateway_thread_bindings().await {
            Ok(bindings) if !bindings.is_empty() => {
                let map: HashMap<String, String> = bindings.into_iter().collect();
                *self.gateway_threads.write().await = map;
            }
            Ok(_) => {}
            Err(e) => tracing::warn!("failed to load gateway thread map from sqlite: {e}"),
        }
        match self.history.list_gateway_route_modes().await {
            Ok(modes) if !modes.is_empty() => {
                let map: HashMap<String, gateway::GatewayRouteMode> = modes
                    .into_iter()
                    .map(|(channel_key, route_mode)| {
                        (channel_key, gateway::GatewayRouteMode::parse(&route_mode))
                    })
                    .collect();
                *self.gateway_route_modes.write().await = map;
            }
            Ok(_) => {}
            Err(e) => tracing::warn!("failed to load gateway route modes from sqlite: {e}"),
        }

        // One-time migration from legacy file persistence to SQLite table.
        let legacy_gateway_threads_path = self.data_dir.join("gateway-threads.json");
        if legacy_gateway_threads_path.exists() {
            match tokio::fs::read_to_string(&legacy_gateway_threads_path).await {
                Ok(raw) => match serde_json::from_str::<HashMap<String, String>>(&raw) {
                    Ok(items) => {
                        let now = now_millis();
                        let mut imported = 0usize;
                        let mut failed = 0usize;
                        for (channel_key, thread_id) in &items {
                            match self
                                .history
                                .upsert_gateway_thread_binding(channel_key, thread_id, now)
                                .await
                            {
                                Ok(_) => imported += 1,
                                Err(error) => {
                                    failed += 1;
                                    tracing::warn!(
                                        channel_key = %channel_key,
                                        thread_id = %thread_id,
                                        %error,
                                        "failed to migrate legacy gateway thread binding"
                                    );
                                }
                            }
                        }

                        if failed == 0 {
                            if let Err(error) =
                                tokio::fs::remove_file(&legacy_gateway_threads_path).await
                            {
                                tracing::warn!(%error, "failed to remove legacy gateway thread map file after migration");
                            } else {
                                tracing::info!(
                                    imported,
                                    "migrated legacy gateway thread map into sqlite"
                                );
                            }
                        } else {
                            tracing::warn!(imported, failed, "legacy gateway thread map migration partially failed; keeping legacy file");
                        }

                        if !items.is_empty() {
                            match self.history.list_gateway_thread_bindings().await {
                                Ok(bindings) if !bindings.is_empty() => {
                                    let map: HashMap<String, String> =
                                        bindings.into_iter().collect();
                                    *self.gateway_threads.write().await = map;
                                }
                                Ok(_) => {}
                                Err(error) => tracing::warn!(
                                    %error,
                                    "failed to reload gateway thread bindings from sqlite after migration"
                                ),
                            }
                        }
                    }
                    Err(error) => tracing::warn!(
                        %error,
                        "failed to parse legacy gateway-threads.json for migration"
                    ),
                },
                Err(error) => tracing::warn!(
                    %error,
                    "failed to read legacy gateway-threads.json for migration"
                ),
            }
        }

        match self.history.list_operator_profile_sessions().await {
            Ok(rows) if !rows.is_empty() => {
                let mut sessions = HashMap::new();
                for row in rows {
                    match serde_json::from_str::<OperatorProfileSessionState>(&row.session_json) {
                        Ok(session) => {
                            sessions.insert(row.session_id, session);
                        }
                        Err(error) => tracing::warn!(
                            session_id = %row.session_id,
                            kind = %row.kind,
                            updated_at = row.updated_at,
                            %error,
                            "failed to hydrate operator profile session"
                        ),
                    }
                }
                *self.operator_profile_sessions.write().await = sessions;
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!("failed to load operator profile sessions from sqlite: {error}")
            }
        }

        // Load heartbeat items
        let heartbeat_path = self.data_dir.join("heartbeat.json");
        if heartbeat_path.exists() {
            match tokio::fs::read_to_string(&heartbeat_path).await {
                Ok(raw) => {
                    if let Ok(items) = serde_json::from_str::<Vec<HeartbeatItem>>(&raw) {
                        *self.heartbeat_items.write().await = items;
                    }
                }
                Err(e) => tracing::warn!("failed to load heartbeat items: {e}"),
            }
        }

        // Load/seed memory files
        ensure_memory_files(&self.data_dir).await?;
        self.refresh_memory_cache().await;
        self.refresh_operator_model().await?;
        match self.history.list_collaboration_sessions().await {
            Ok(rows) if !rows.is_empty() => {
                let mut collaboration = HashMap::new();
                for row in rows {
                    match serde_json::from_str::<super::collaboration::CollaborationSession>(
                        &row.session_json,
                    ) {
                        Ok(session) => {
                            collaboration.insert(row.parent_task_id, session);
                        }
                        Err(error) => tracing::warn!(
                            parent_task_id = %row.parent_task_id,
                            "failed to hydrate collaboration session: {error}"
                        ),
                    }
                }
                *self.collaboration.write().await = collaboration;
            }
            Ok(_) => {}
            Err(error) => tracing::warn!("failed to load collaboration sessions: {error}"),
        }

        // Seed built-in skill documents into ~/.tamux/skills/builtin/
        seed_builtin_skills(&self.data_dir);

        // Restore HeuristicStore from persistence (D-10)
        let heuristic_path = self.data_dir.join("heuristics.json");
        if heuristic_path.exists() {
            match tokio::fs::read_to_string(&heuristic_path).await {
                Ok(json) => match serde_json::from_str(&json) {
                    Ok(store) => {
                        *self.heuristic_store.write().await = store;
                        tracing::info!(
                            "restored heuristic store from {}",
                            heuristic_path.display()
                        );
                    }
                    Err(e) => tracing::warn!(
                        error = %e,
                        "failed to parse heuristics.json, using defaults"
                    ),
                },
                Err(e) => tracing::warn!(error = %e, "failed to read heuristics.json"),
            }
        }

        // Restore PatternStore from persistence (D-10)
        let pattern_path = self.data_dir.join("patterns.json");
        if pattern_path.exists() {
            match tokio::fs::read_to_string(&pattern_path).await {
                Ok(json) => match serde_json::from_str(&json) {
                    Ok(store) => {
                        *self.pattern_store.write().await = store;
                        tracing::info!("restored pattern store from {}", pattern_path.display());
                    }
                    Err(e) => tracing::warn!(
                        error = %e,
                        "failed to parse patterns.json, using defaults"
                    ),
                },
                Err(e) => tracing::warn!(error = %e, "failed to read patterns.json"),
            }
        }

        // D-10: Restore context for the most recent active thread.
        {
            let threads = self.threads.read().await;
            let most_recent = threads
                .values()
                .filter(|t| !t.messages.is_empty())
                .max_by_key(|t| t.messages.last().map(|m| m.timestamp).unwrap_or(0));

            if let Some(thread) = most_recent {
                let thread_id = thread.id.clone();
                let last_topic = thread
                    .messages
                    .iter()
                    .rev()
                    .find(|m| matches!(m.role, MessageRole::User))
                    .map(|m| {
                        let content: String = m.content.chars().take(100).collect();
                        if m.content.len() > 100 {
                            format!("{}...", content)
                        } else {
                            content
                        }
                    })
                    .unwrap_or_else(|| "previous session".to_string());
                drop(threads);

                // Try FTS5 archive restoration
                match self
                    .history
                    .list_context_archive_entries(&thread_id, 20)
                    .await
                {
                    Ok(rows) if !rows.is_empty() => {
                        let entries: Vec<super::context::archive::ArchiveEntry> = rows
                            .into_iter()
                            .map(|row| super::context::archive::ArchiveEntry {
                                id: row.id,
                                thread_id: row.thread_id,
                                original_role: row.original_role,
                                compressed_content: row.compressed_content,
                                summary: row.summary,
                                relevance_score: row.relevance_score,
                                token_count_original: row.token_count_original as u32,
                                token_count_compressed: row.token_count_compressed as u32,
                                metadata: row
                                    .metadata_json
                                    .and_then(|j| serde_json::from_str(&j).ok()),
                                archived_at: row.archived_at as u64,
                                last_accessed_at: row.last_accessed_at.map(|v| v as u64),
                            })
                            .collect();

                        let request = super::context::restoration::RestorationRequest {
                            thread_id: thread_id.clone(),
                            query: Some(last_topic.clone()),
                            max_items: 10,
                            max_tokens: 2000,
                        };
                        let restored =
                            super::context::restoration::rank_and_select(&entries, &request);
                        if !restored.is_empty() {
                            tracing::info!(
                                thread_id = %thread_id,
                                items = restored.len(),
                                "restored context for most recent thread (D-10)"
                            );

                            // Store a continuity flag -- the next agent message in this thread
                            // should acknowledge the context restoration.
                            self.history
                                .set_consolidation_state(
                                    "continuity_thread_id",
                                    &thread_id,
                                    now_millis(),
                                )
                                .await
                                .ok();
                            self.history
                                .set_consolidation_state(
                                    "continuity_topic",
                                    &last_topic,
                                    now_millis(),
                                )
                                .await
                                .ok();
                        }
                    }
                    Ok(_) => {
                        tracing::debug!(
                            "no archive entries for most recent thread, skipping context restoration"
                        );
                    }
                    Err(e) => {
                        tracing::warn!(
                            error = %e,
                            "failed to list archive entries for context restoration"
                        );
                    }
                }
            }
        }

        let repo_watches = {
            let contexts = self.thread_work_contexts.read().await;
            contexts
                .iter()
                .filter_map(|(thread_id, context)| {
                    context
                        .entries
                        .iter()
                        .find_map(|entry| entry.repo_root.clone())
                        .map(|repo_root| (thread_id.clone(), repo_root))
                })
                .collect::<Vec<_>>()
        };
        for (thread_id, repo_root) in repo_watches {
            self.ensure_repo_watcher(&thread_id, &repo_root).await;
        }

        tracing::info!("agent engine hydrated from {:?}", self.data_dir);

        // Initialize gateway runtime ownership and spawn the standalone gateway when enabled.
        self.maybe_spawn_gateway().await;

        Ok(())
    }
}

#[cfg(test)]
#[path = "tests/persistence.rs"]
mod tests;
