use super::*;

mod events_activity;
mod events_audio;
mod events_connection;
mod events_integrations;
mod events_status;
mod events_tasks;

impl TuiModel {
    pub(in crate::app) fn is_internal_agent_thread(thread_id: &str, title: Option<&str>) -> bool {
        let normalized_id = thread_id.trim().to_ascii_lowercase();
        let normalized_title = title.unwrap_or_default().trim().to_ascii_lowercase();
        normalized_id.starts_with("dm:") || normalized_title.starts_with("internal dm")
    }

    pub(in crate::app) fn is_hidden_agent_thread(thread_id: &str, title: Option<&str>) -> bool {
        let normalized_id = thread_id.trim().to_ascii_lowercase();
        let normalized_title = title.unwrap_or_default().trim().to_ascii_lowercase();
        normalized_id.starts_with("handoff:")
            || normalized_id.starts_with("playground:")
            || normalized_title.starts_with("handoff ")
            || normalized_title.starts_with("participant playground ")
            || normalized_title == "weles"
            || normalized_title.starts_with("weles ")
    }

    fn should_ignore_internal_thread_activity(&self, thread_id: &str) -> bool {
        Self::is_internal_agent_thread(thread_id, None)
            && self.chat.active_thread_id() != Some(thread_id)
    }

    fn sync_open_thread_picker(&mut self) {
        if self.modal.top() == Some(modal::ModalKind::ThreadPicker) {
            self.sync_thread_picker_item_count();
        }
    }

    pub fn pump_daemon_events_budgeted(&mut self, limit: usize) -> usize {
        let mut processed = 0;
        while processed < limit {
            match self.daemon_events_rx.try_recv() {
                Ok(event) => {
                    self.handle_client_event(event);
                    processed += 1;
                }
                Err(_) => break,
            }
        }
        processed
    }

    pub fn pump_daemon_events(&mut self) {
        let _ = self.pump_daemon_events_budgeted(usize::MAX);
    }

    pub fn on_tick(&mut self) {
        self.tick_counter = self.tick_counter.saturating_add(1);
        self.chat.clear_expired_copy_feedback(self.tick_counter);
        self.maybe_request_older_chat_history();
        self.maybe_request_older_goal_run_history();
        self.maybe_refresh_spawned_sidebar_tasks();
        self.maybe_schedule_chat_history_collapse();
        self.chat.maybe_collapse_history(self.tick_counter);
        self.clear_expired_queued_prompt_copy_feedback();

        if let Some(player) = self.voice_player.as_mut() {
            match player.try_wait() {
                Ok(Some(_status)) => {
                    self.voice_player = None;
                    if self.status_line == "Playing synthesized speech..." {
                        self.status_line = "Audio playback finished".to_string();
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    self.voice_player = None;
                    self.status_line = "Audio playback process error".to_string();
                    self.last_error = Some(format!("Audio playback monitor failed: {error}"));
                    self.error_active = true;
                    self.error_tick = self.tick_counter;
                }
            }
        }

        let _ = self.maybe_auto_send_always_auto_response();
        if self
            .active_auto_response_countdown_secs()
            .is_some_and(|remaining| remaining == 0)
            && !self.assistant_busy()
        {
            let _ = self.execute_active_auto_response_action(AutoResponseActionSelection::Yes);
        }
        if self.pending_stop && !self.pending_stop_active() {
            self.pending_stop = false;
        }
        if self
            .input_notice
            .as_ref()
            .is_some_and(|notice| self.tick_counter >= notice.expires_at_tick)
        {
            self.input_notice = None;
        }
        self.publish_attention_surface_if_changed();
    }

    fn maybe_refresh_spawned_sidebar_tasks(&mut self) {
        let Some(active_thread_id) = self.chat.active_thread_id() else {
            return;
        };
        if self.thread_loading_id.is_some() {
            return;
        }
        if self.sidebar.active_tab() != sidebar::SidebarTab::Spawned {
            return;
        }
        if !widgets::sidebar::has_spawned_tab(&self.tasks, &self.chat, Some(active_thread_id)) {
            return;
        }
        if self.tick_counter < self.next_spawned_sidebar_task_refresh_tick {
            return;
        }

        self.send_daemon_command(DaemonCommand::ListTasks);
        self.next_spawned_sidebar_task_refresh_tick = self
            .tick_counter
            .saturating_add(SPAWNED_SIDEBAR_TASK_REFRESH_TICKS);
    }

    pub(crate) fn handle_client_event(&mut self, event: ClientEvent) {
        if let Some(ref cancelled_id) = self.cancelled_thread_id.clone() {
            let skip = match &event {
                ClientEvent::Delta { thread_id, .. }
                | ClientEvent::Reasoning { thread_id, .. }
                | ClientEvent::ToolCall { thread_id, .. }
                | ClientEvent::ToolResult { thread_id, .. }
                | ClientEvent::RetryStatus { thread_id, .. } => thread_id == cancelled_id,
                ClientEvent::Done { thread_id, .. } => {
                    if thread_id == cancelled_id {
                        self.cancelled_thread_id = None;
                        true
                    } else {
                        false
                    }
                }
                _ => false,
            };
            if skip {
                return;
            }
        }

        match event {
            ClientEvent::Connected => {
                self.handle_connected_event();
            }
            ClientEvent::Disconnected => {
                self.handle_disconnected_event();
            }
            ClientEvent::Reconnecting { delay_secs } => {
                self.handle_reconnecting_event(delay_secs);
            }
            ClientEvent::SessionSpawned { session_id } => {
                self.handle_session_spawned_event(session_id);
            }
            ClientEvent::ApprovalRequired {
                approval_id,
                command,
                rationale,
                reasons,
                risk_level,
                blast_radius,
            } => {
                self.handle_approval_required_event(
                    approval_id,
                    command,
                    rationale,
                    reasons,
                    risk_level,
                    blast_radius,
                );
            }
            ClientEvent::ApprovalResolved {
                approval_id,
                decision,
            } => {
                self.handle_approval_resolved_event(approval_id, decision);
            }
            ClientEvent::TaskApprovalRules(rules) => {
                self.handle_task_approval_rules_event(rules);
            }
            ClientEvent::ThreadList(threads) => {
                self.handle_thread_list_event(threads);
            }
            ClientEvent::ThreadDetail(Some(thread)) => {
                self.handle_thread_detail_event(thread);
            }
            ClientEvent::ThreadDetail(None) => {
                let _ = self.fallback_pending_reconnect_restore();
            }
            ClientEvent::ThreadCreated {
                thread_id,
                title,
                agent_name,
            } => {
                self.handle_thread_created_event(thread_id, title, agent_name);
            }
            ClientEvent::ThreadDeleted { thread_id, deleted } => {
                if deleted {
                    self.chat.reduce(chat::ChatAction::ThreadDeleted {
                        thread_id: thread_id.clone(),
                    });
                    self.sync_open_thread_picker();
                    self.status_line = "Thread deleted".to_string();
                } else {
                    self.status_line = "Thread delete failed".to_string();
                }
            }
            ClientEvent::ThreadMessagePinResult(result) => {
                self.handle_thread_message_pin_result_event(result);
            }
            ClientEvent::ThreadReloadRequired { thread_id } => {
                self.handle_thread_reload_required_event(thread_id);
            }
            ClientEvent::ParticipantSuggestion {
                thread_id,
                suggestion,
            } => {
                if suggestion
                    .suggestion_kind
                    .eq_ignore_ascii_case("auto_response")
                {
                    if self.chat.active_thread_id() == Some(thread_id.as_str())
                        && self.active_always_auto_response_participant().is_some_and(
                            |participant| {
                                participant
                                    .agent_id
                                    .eq_ignore_ascii_case(&suggestion.target_agent_id)
                            },
                        )
                        && !self.assistant_busy()
                    {
                        self.hidden_auto_response_suggestion_ids
                            .insert(suggestion.id.clone());
                        self.send_daemon_command(DaemonCommand::SendParticipantSuggestion {
                            thread_id,
                            suggestion_id: suggestion.id,
                        });
                    }
                } else {
                    self.queue_participant_suggestion(
                        thread_id,
                        suggestion.id,
                        suggestion.target_agent_id,
                        suggestion.target_agent_name,
                        suggestion.instruction,
                        suggestion.force_send,
                    );
                }
            }
            ClientEvent::TaskList(tasks) => {
                self.handle_task_list_event(tasks);
            }
            ClientEvent::TaskUpdate(task) => {
                self.handle_task_update_event(task);
            }
            ClientEvent::GoalRunList(runs) => {
                self.handle_goal_run_list_event(runs);
            }
            ClientEvent::GoalRunStarted(run) => {
                self.handle_goal_run_started_event(run);
            }
            ClientEvent::GoalRunDetail(Some(run)) => {
                if self.is_placeholder_goal_run_detail(&run) {
                    self.clear_goal_hydration_refresh(&run.id);
                } else {
                    self.clear_goal_hydration_refresh(&run.id);
                    self.handle_goal_run_detail_event(run);
                }
            }
            ClientEvent::GoalRunDetail(None) => {}
            ClientEvent::GoalRunUpdate(run) => {
                self.handle_goal_run_update_event(run);
            }
            ClientEvent::GoalRunDeleted {
                goal_run_id,
                deleted,
            } => {
                if deleted {
                    let cleared_approval_id = self
                        .tasks
                        .goal_run_by_id(&goal_run_id)
                        .and_then(|run| run.awaiting_approval_id.clone());
                    let viewing_deleted_goal = if let MainPaneView::Task(target) =
                        &self.main_pane_view
                    {
                        target_goal_run_id(self, target).as_deref() == Some(goal_run_id.as_str())
                    } else {
                        false
                    };
                    let deleted_goal_run_id = goal_run_id.clone();
                    self.tasks
                        .reduce(task::TaskAction::GoalRunDeleted { goal_run_id });
                    self.clear_goal_hydration_refresh(&deleted_goal_run_id);
                    if let Some(approval_id) = cleared_approval_id {
                        self.approval
                            .reduce(crate::state::ApprovalAction::ClearResolved(approval_id));
                    }
                    if self.modal.top() == Some(modal::ModalKind::GoalPicker) {
                        self.sync_goal_picker_item_count();
                    }
                    if viewing_deleted_goal {
                        self.main_pane_view = MainPaneView::Conversation;
                    }
                    self.status_line = "Goal run deleted".to_string();
                } else {
                    self.status_line = "Goal run delete failed".to_string();
                }
            }
            ClientEvent::GoalRunCheckpoints {
                goal_run_id,
                checkpoints,
            } => {
                self.handle_goal_run_checkpoints_event(goal_run_id, checkpoints);
            }
            ClientEvent::GoalHydrationScheduleFailed { goal_run_id } => {
                self.clear_goal_hydration_refresh(&goal_run_id);
            }
            ClientEvent::ThreadTodos {
                thread_id,
                goal_run_id,
                step_index,
                items,
            } => {
                self.handle_thread_todos_event(thread_id, goal_run_id, step_index, items);
            }
            ClientEvent::WorkContext(context) => {
                self.handle_work_context_event(context);
            }
            ClientEvent::GitDiff {
                repo_path,
                file_path,
                diff,
            } => {
                self.handle_git_diff_event(repo_path, file_path, diff);
            }
            ClientEvent::FilePreview {
                path,
                content,
                truncated,
                is_text,
            } => {
                self.handle_file_preview_event(path, content, truncated, is_text);
            }
            ClientEvent::AgentConfig(cfg) => {
                self.handle_agent_config_event(cfg);
            }
            ClientEvent::AgentConfigRaw(raw) => {
                self.handle_agent_config_raw_event(raw);
            }
            ClientEvent::SpeechToTextResult { content } => {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(error) = value.get("error").and_then(|v| v.as_str()) {
                        self.status_line = format!("STT failed: {error}");
                        self.show_input_notice(
                            "Speech-to-text failed (see status/error)",
                            InputNoticeKind::Warning,
                            80,
                            true,
                        );
                        self.last_error = Some(format!("STT failed: {error}"));
                        self.error_active = true;
                        self.error_tick = self.tick_counter;
                        return;
                    }
                }

                let transcript = serde_json::from_str::<serde_json::Value>(&content)
                    .ok()
                    .and_then(|value| {
                        value
                            .get("text")
                            .and_then(|value| value.as_str())
                            .map(str::to_string)
                    })
                    .unwrap_or_else(|| content.trim().to_string());
                if !transcript.is_empty() {
                    if !self.input.buffer().trim().is_empty() {
                        self.input.reduce(input::InputAction::InsertChar(' '));
                    }
                    for ch in transcript.chars() {
                        self.input.reduce(input::InputAction::InsertChar(ch));
                    }
                    self.focus = FocusArea::Input;
                    self.status_line = "Voice transcription ready".to_string();
                }
            }
            ClientEvent::TextToSpeechResult { content } => {
                self.clear_matching_agent_activity("preparing speech");
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(error) = value.get("error").and_then(|v| v.as_str()) {
                        self.status_line = format!("TTS failed: {error}");
                        self.show_input_notice(
                            "Text-to-speech failed (see status/error)",
                            InputNoticeKind::Warning,
                            80,
                            true,
                        );
                        self.last_error = Some(format!("TTS failed: {error}"));
                        self.error_active = true;
                        self.error_tick = self.tick_counter;
                        return;
                    }
                }

                let path = serde_json::from_str::<serde_json::Value>(&content)
                    .ok()
                    .and_then(|value| {
                        value
                            .get("path")
                            .and_then(|value| value.as_str())
                            .map(str::to_string)
                    });
                if let Some(path) = path {
                    self.play_audio_path(&path);
                } else {
                    self.status_line = "TTS result missing audio path".to_string();
                    self.show_input_notice(
                        "TTS returned no playable path",
                        InputNoticeKind::Warning,
                        70,
                        true,
                    );
                }
            }
            ClientEvent::GenerateImageResult { content } => {
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&content) {
                    if let Some(error) = value.get("error").and_then(|v| v.as_str()) {
                        self.status_line = format!("Image generation failed: {error}");
                        self.show_input_notice(
                            "Image generation failed (see status/error)",
                            InputNoticeKind::Warning,
                            80,
                            true,
                        );
                        self.last_error = Some(format!("Image generation failed: {error}"));
                        self.error_active = true;
                        self.error_tick = self.tick_counter;
                        return;
                    }

                    let thread_id = value
                        .get("thread_id")
                        .and_then(|entry| entry.as_str())
                        .map(str::to_string);
                    let status_target = value
                        .get("path")
                        .and_then(|entry| entry.as_str())
                        .or_else(|| value.get("url").and_then(|entry| entry.as_str()))
                        .or_else(|| value.get("file_url").and_then(|entry| entry.as_str()));

                    if let Some(thread_id) = thread_id {
                        if self.chat.active_thread_id() == Some(thread_id.as_str()) {
                            self.request_authoritative_thread_refresh(thread_id.clone(), false);
                        } else {
                            self.open_thread_conversation(thread_id.clone());
                        }
                        self.send_daemon_command(DaemonCommand::RequestThreadWorkContext(
                            thread_id,
                        ));
                    }

                    self.status_line = status_target
                        .map(|target| format!("Image generated: {target}"))
                        .unwrap_or_else(|| "Image generated".to_string());
                } else {
                    self.status_line = "Image generated".to_string();
                }
            }
            ClientEvent::ModelsFetched(models) => {
                self.handle_models_fetched_event(models);
            }
            ClientEvent::HeartbeatItems(items) => {
                self.handle_heartbeat_items_event(items);
            }
            ClientEvent::HeartbeatDigest {
                cycle_id,
                actionable,
                digest,
                items,
                checked_at,
                explanation,
            } => {
                self.handle_heartbeat_digest_event(
                    cycle_id,
                    actionable,
                    digest,
                    items,
                    checked_at,
                    explanation,
                );
            }
            ClientEvent::AuditEntry {
                id,
                timestamp,
                action_type,
                summary,
                explanation,
                confidence,
                confidence_band,
                causal_trace_id,
                thread_id,
            } => {
                self.handle_audit_entry_event(
                    id,
                    timestamp,
                    action_type,
                    summary,
                    explanation,
                    confidence,
                    confidence_band,
                    causal_trace_id,
                    thread_id,
                );
            }
            ClientEvent::EscalationUpdate {
                thread_id,
                from_level,
                to_level,
                reason,
                attempts,
                audit_id,
            } => {
                self.handle_escalation_update_event(
                    thread_id, from_level, to_level, reason, attempts, audit_id,
                );
            }
            ClientEvent::AnticipatoryItems(items) => {
                self.handle_anticipatory_items_event(items);
            }
            ClientEvent::GatewayStatus {
                platform,
                status,
                last_error,
                consecutive_failures,
            } => {
                self.handle_gateway_status_event(
                    platform,
                    status,
                    last_error,
                    consecutive_failures,
                );
            }
            ClientEvent::WhatsAppLinkStatus {
                state,
                phone,
                last_error,
            } => {
                self.handle_whatsapp_link_status_event(state, phone, last_error);
            }
            ClientEvent::WhatsAppLinkQr {
                ascii_qr,
                expires_at_ms,
            } => {
                self.handle_whatsapp_link_qr_event(ascii_qr, expires_at_ms);
            }
            ClientEvent::WhatsAppLinked { phone } => {
                self.handle_whatsapp_linked_event(phone);
            }
            ClientEvent::WhatsAppLinkError { message, .. } => {
                self.handle_whatsapp_link_error_event(message);
            }
            ClientEvent::WhatsAppLinkDisconnected { reason } => {
                self.handle_whatsapp_link_disconnected_event(reason);
            }
            ClientEvent::TierChanged { new_tier } => {
                self.handle_tier_changed_event(new_tier);
            }
            ClientEvent::Delta { thread_id, content } => {
                self.handle_delta_event(thread_id, content);
            }
            ClientEvent::Reasoning { thread_id, content } => {
                self.handle_reasoning_event(thread_id, content);
            }
            ClientEvent::ToolCall {
                thread_id,
                call_id,
                name,
                arguments,
                weles_review,
            } => {
                self.handle_tool_call_event(thread_id, call_id, name, arguments, weles_review);
            }
            ClientEvent::ToolResult {
                thread_id,
                call_id,
                name,
                content,
                is_error,
                weles_review,
            } => {
                self.handle_tool_result_event(
                    thread_id,
                    call_id,
                    name,
                    content,
                    is_error,
                    weles_review,
                );
            }
            ClientEvent::Done {
                thread_id,
                input_tokens,
                output_tokens,
                cost,
                provider,
                model,
                tps,
                generation_ms,
                reasoning,
                provider_final_result_json,
            } => {
                self.handle_done_event(
                    thread_id,
                    input_tokens,
                    output_tokens,
                    cost,
                    provider,
                    model,
                    tps,
                    generation_ms,
                    reasoning,
                    provider_final_result_json,
                );
            }
            ClientEvent::ProviderAuthStates(entries) => {
                self.handle_provider_auth_states_event(entries);
            }
            ClientEvent::OpenAICodexAuthStatus(status) => {
                self.handle_openai_codex_auth_status_event(status);
            }
            ClientEvent::OpenAICodexAuthLoginResult(status) => {
                self.handle_openai_codex_auth_login_result_event(status);
            }
            ClientEvent::OpenAICodexAuthLogoutResult { ok, error } => {
                self.handle_openai_codex_auth_logout_result_event(ok, error);
            }
            ClientEvent::ProviderValidation {
                provider_id,
                valid,
                error,
            } => {
                self.handle_provider_validation_event(provider_id, valid, error);
            }
            ClientEvent::SubAgentList(entries) => {
                self.handle_subagent_list_event(entries);
            }
            ClientEvent::SubAgentUpdated(entry) => {
                self.handle_subagent_updated_event(entry);
            }
            ClientEvent::SubAgentRemoved { sub_agent_id } => {
                self.handle_subagent_removed_event(sub_agent_id);
            }
            ClientEvent::ConciergeConfig(raw) => {
                self.handle_concierge_config_event(raw);
            }
            ClientEvent::ConciergeWelcome { content, actions } => {
                self.handle_concierge_welcome_event(content, actions);
            }
            ClientEvent::ConciergeWelcomeDismissed => {
                self.handle_concierge_welcome_dismissed_event();
            }
            ClientEvent::OperatorProfileSessionStarted { session_id, kind } => {
                self.handle_operator_profile_session_started_event(session_id, kind);
            }
            ClientEvent::OperatorProfileQuestion {
                session_id,
                question_id,
                field_key,
                prompt,
                input_kind,
                optional,
            } => {
                self.handle_operator_profile_question_event(
                    session_id,
                    question_id,
                    field_key,
                    prompt,
                    input_kind,
                    optional,
                );
            }
            ClientEvent::OperatorQuestion {
                question_id,
                content,
                options,
                session_id: _,
                thread_id,
            } => {
                self.handle_operator_question_event(question_id, content, options, thread_id);
            }
            ClientEvent::OperatorQuestionResolved {
                question_id,
                answer,
            } => {
                self.handle_operator_question_resolved_event(question_id, answer);
            }
            ClientEvent::OperatorProfileProgress {
                session_id,
                answered,
                remaining,
                completion_ratio,
            } => {
                self.handle_operator_profile_progress_event(
                    session_id,
                    answered,
                    remaining,
                    completion_ratio,
                );
            }
            ClientEvent::OperatorProfileSummary { summary_json } => {
                self.handle_operator_profile_summary_event(summary_json);
            }
            ClientEvent::OperatorModelSummary { model_json } => {
                self.handle_operator_model_summary_event(model_json);
            }
            ClientEvent::OperatorModelReset { ok } => {
                self.handle_operator_model_reset_event(ok);
            }
            ClientEvent::CollaborationSessions { sessions_json } => {
                self.handle_collaboration_sessions_event(sessions_json);
            }
            ClientEvent::CollaborationVoteResult { report_json } => {
                self.handle_collaboration_vote_result_event(report_json);
            }
            ClientEvent::GeneratedTools { tools_json } => {
                self.handle_generated_tools_event(tools_json);
            }
            ClientEvent::OperatorProfileSessionCompleted {
                session_id,
                updated_fields,
            } => {
                self.handle_operator_profile_session_completed_event(session_id, updated_fields);
            }
            // Plugin settings events (Plan 16-03)
            ClientEvent::PluginList(plugins) => {
                self.handle_plugin_list_event(plugins);
            }
            ClientEvent::PluginGet {
                plugin: _,
                settings_schema,
            } => {
                self.handle_plugin_get_event(settings_schema);
            }
            ClientEvent::PluginSettings {
                plugin_name: _,
                settings,
            } => {
                self.handle_plugin_settings_event(settings);
            }
            ClientEvent::PluginTestConnection {
                plugin_name: _,
                success,
                message,
            } => {
                self.handle_plugin_test_connection_event(success, message);
            }
            ClientEvent::PluginAction { success, message } => {
                self.handle_plugin_action_event(success, message);
            }
            ClientEvent::PluginCommands(commands) => {
                self.handle_plugin_commands_event(commands);
            }
            ClientEvent::PluginOAuthUrl { name, url } => {
                self.handle_plugin_oauth_url_event(name, url);
            }
            ClientEvent::PluginOAuthComplete {
                name,
                success,
                error,
            } => {
                self.handle_plugin_oauth_complete_event(name, success, error);
            }
            ClientEvent::NotificationSnapshot(notifications) => {
                self.handle_notification_snapshot_event(notifications);
            }
            ClientEvent::NotificationUpsert(notification) => {
                self.handle_notification_upsert_event(notification);
            }
            ClientEvent::Error(message) => {
                self.handle_error_event(message);
            }
            ClientEvent::RetryStatus {
                thread_id,
                phase,
                attempt,
                max_retries,
                delay_ms,
                failure_class,
                message,
            } => {
                self.handle_retry_status_event(
                    thread_id,
                    phase,
                    attempt,
                    max_retries,
                    delay_ms,
                    failure_class,
                    message,
                );
            }
            ClientEvent::WorkflowNotice {
                thread_id,
                kind,
                message,
                details,
            } => {
                self.handle_workflow_notice_event(thread_id, kind, message, details);
            }
            ClientEvent::WelesHealthUpdate {
                state,
                reason,
                checked_at,
            } => {
                self.handle_weles_health_update_event(state, reason, checked_at);
            }
            ClientEvent::StatusDiagnostics {
                operator_profile_sync_state,
                operator_profile_sync_dirty,
                operator_profile_scheduler_fallback,
                diagnostics_json,
            } => {
                self.handle_status_diagnostics_event(
                    operator_profile_sync_state,
                    operator_profile_sync_dirty,
                    operator_profile_scheduler_fallback,
                    diagnostics_json,
                );
            }
            ClientEvent::StatusSnapshot(snapshot) => {
                self.handle_status_snapshot_event(snapshot);
            }
            ClientEvent::StatisticsSnapshot(snapshot) => {
                self.handle_statistics_snapshot_event(snapshot);
            }
            ClientEvent::PromptInspection(prompt) => {
                self.handle_prompt_inspection_event(prompt);
            }
            ClientEvent::AgentExplanation(payload) => {
                self.handle_agent_explanation_event(payload);
            }
            ClientEvent::DivergentSessionStarted(payload) => {
                self.handle_divergent_session_started_event(payload);
            }
            ClientEvent::DivergentSession(payload) => {
                self.handle_divergent_session_event(payload);
            }
        }
    }
}

#[cfg(test)]
#[path = "tests/events.rs"]
mod tests;
