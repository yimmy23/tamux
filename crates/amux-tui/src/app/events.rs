use super::*;

mod events_activity;
mod events_connection;
mod events_integrations;
mod events_status;
mod events_tasks;

impl TuiModel {
    fn is_internal_agent_thread(thread_id: &str, title: Option<&str>) -> bool {
        let normalized_id = thread_id.trim().to_ascii_lowercase();
        let normalized_title = title.unwrap_or_default().trim().to_ascii_lowercase();
        normalized_id.starts_with("dm:") || normalized_title.starts_with("internal dm")
    }

    fn is_hidden_agent_thread(thread_id: &str, title: Option<&str>) -> bool {
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

    pub fn pump_daemon_events(&mut self) {
        while let Ok(event) = self.daemon_events_rx.try_recv() {
            self.handle_client_event(event);
        }
    }

    pub fn on_tick(&mut self) {
        self.tick_counter = self.tick_counter.saturating_add(1);
        self.chat.clear_expired_copy_feedback(self.tick_counter);
        self.maybe_request_older_chat_history();
        self.maybe_schedule_chat_history_collapse();
        self.chat.maybe_collapse_history(self.tick_counter);
        self.clear_expired_queued_prompt_copy_feedback();
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
            ClientEvent::ThreadDetail(None) => {}
            ClientEvent::ThreadCreated {
                thread_id,
                title,
                agent_name,
            } => {
                self.handle_thread_created_event(thread_id, title, agent_name);
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
                self.handle_goal_run_detail_event(run);
            }
            ClientEvent::GoalRunDetail(None) => {}
            ClientEvent::GoalRunUpdate(run) => {
                self.handle_goal_run_update_event(run);
            }
            ClientEvent::GoalRunCheckpoints {
                goal_run_id,
                checkpoints,
            } => {
                self.handle_goal_run_checkpoints_event(goal_run_id, checkpoints);
            }
            ClientEvent::ThreadTodos { thread_id, items } => {
                self.handle_thread_todos_event(thread_id, items);
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
