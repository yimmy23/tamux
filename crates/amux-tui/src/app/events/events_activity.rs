use super::*;

impl TuiModel {
    pub(in crate::app) fn handle_delta_event(&mut self, thread_id: String, content: String) {
        if Self::is_hidden_agent_thread(&thread_id, None)
            || self.should_ignore_internal_thread_activity(&thread_id)
        {
            return;
        }
        self.agent_activity = Some("writing".to_string());
        self.anticipatory
            .reduce(crate::state::AnticipatoryAction::Clear);
        self.chat
            .reduce(chat::ChatAction::Delta { thread_id, content });
    }

    pub(in crate::app) fn handle_reasoning_event(&mut self, thread_id: String, content: String) {
        if Self::is_hidden_agent_thread(&thread_id, None)
            || self.should_ignore_internal_thread_activity(&thread_id)
        {
            return;
        }
        self.agent_activity = Some("reasoning".to_string());
        self.anticipatory
            .reduce(crate::state::AnticipatoryAction::Clear);
        self.chat
            .reduce(chat::ChatAction::Reasoning { thread_id, content });
    }

    pub(in crate::app) fn handle_tool_call_event(
        &mut self,
        thread_id: String,
        call_id: String,
        name: String,
        arguments: String,
        weles_review: Option<crate::client::WelesReviewMetaVm>,
    ) {
        if Self::is_hidden_agent_thread(&thread_id, None)
            || self.should_ignore_internal_thread_activity(&thread_id)
        {
            return;
        }
        self.agent_activity = Some(format!("⚙  {}", name));
        self.anticipatory
            .reduce(crate::state::AnticipatoryAction::Clear);
        self.chat.reduce(chat::ChatAction::ToolCall {
            thread_id,
            call_id,
            name,
            args: arguments,
            weles_review,
        });
    }

    pub(in crate::app) fn handle_tool_result_event(
        &mut self,
        thread_id: String,
        call_id: String,
        name: String,
        content: String,
        is_error: bool,
        weles_review: Option<crate::client::WelesReviewMetaVm>,
    ) {
        if Self::is_hidden_agent_thread(&thread_id, None)
            || self.should_ignore_internal_thread_activity(&thread_id)
        {
            return;
        }
        self.agent_activity = Some(format!("⚙  {} ✓", name));
        self.anticipatory
            .reduce(crate::state::AnticipatoryAction::Clear);
        self.chat.reduce(chat::ChatAction::ToolResult {
            thread_id,
            call_id,
            name,
            content,
            is_error,
            weles_review,
        });
        self.dispatch_next_queued_prompt_if_ready();
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::app) fn handle_done_event(
        &mut self,
        thread_id: String,
        input_tokens: u64,
        output_tokens: u64,
        cost: Option<f64>,
        provider: Option<String>,
        model: Option<String>,
        tps: Option<f64>,
        generation_ms: Option<u64>,
        reasoning: Option<String>,
        provider_final_result_json: Option<String>,
    ) {
        if Self::is_hidden_agent_thread(&thread_id, None)
            || self.should_ignore_internal_thread_activity(&thread_id)
        {
            return;
        }
        self.agent_activity = None;
        self.pending_stop = false;
        self.anticipatory
            .reduce(crate::state::AnticipatoryAction::Clear);
        if self
            .input_notice
            .as_ref()
            .is_some_and(|notice| notice.kind == InputNoticeKind::Warning)
        {
            self.input_notice = None;
        }
        self.chat.reduce(chat::ChatAction::TurnDone {
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
        });

        self.dispatch_next_queued_prompt_if_ready();
    }

    pub(in crate::app) fn handle_operator_profile_session_started_event(
        &mut self,
        session_id: String,
        kind: String,
    ) {
        self.operator_profile.visible = true;
        self.operator_profile.loading = true;
        self.operator_profile.session_id = Some(session_id);
        self.operator_profile.session_kind = Some(kind);
        self.operator_profile.question = None;
        self.operator_profile.warning = None;
        self.set_main_pane_conversation(FocusArea::Input);
        self.status_line = "Operator profile onboarding started".to_string();
        self.send_daemon_command(DaemonCommand::GetOperatorProfileSummary);
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::app) fn handle_operator_profile_question_event(
        &mut self,
        session_id: String,
        question_id: String,
        field_key: String,
        prompt: String,
        input_kind: String,
        optional: bool,
    ) {
        self.operator_profile.visible = true;
        self.operator_profile.loading = false;
        self.operator_profile.session_id = Some(session_id.clone());
        self.operator_profile.question = Some(super::OperatorProfileQuestionVm {
            session_id,
            question_id,
            field_key,
            prompt,
            input_kind,
            optional,
        });
        self.operator_profile.warning = None;
        self.set_main_pane_conversation(FocusArea::Input);
        self.input.reduce(input::InputAction::Clear);
        if let Some(options) = self.current_operator_profile_select_options() {
            if let Some(first) = options.first() {
                self.input.set_text(first);
            }
        }
        self.status_line = "Operator profile question ready".to_string();
        self.show_input_notice(
            "Answer then Enter • Ctrl+S skip • Ctrl+D defer",
            InputNoticeKind::Success,
            120,
            true,
        );
    }

    pub(in crate::app) fn handle_operator_profile_progress_event(
        &mut self,
        session_id: String,
        answered: u32,
        remaining: u32,
        completion_ratio: f64,
    ) {
        self.operator_profile.visible = true;
        self.operator_profile.loading = true;
        self.operator_profile.session_id = Some(session_id.clone());
        self.operator_profile.progress = Some(super::OperatorProfileProgressVm {
            answered,
            remaining,
            completion_ratio,
        });
        self.send_daemon_command(DaemonCommand::NextOperatorProfileQuestion { session_id });
        self.status_line = format!(
            "Operator profile progress: {} answered, {} remaining",
            answered, remaining
        );
    }

    pub(in crate::app) fn handle_operator_profile_summary_event(&mut self, summary_json: String) {
        self.operator_profile.summary_json = Some(summary_json.clone());
        if self.operator_profile.progress.is_none() {
            if let Ok(summary) = serde_json::from_str::<serde_json::Value>(&summary_json) {
                let answered = summary
                    .get("field_count")
                    .and_then(|value| value.as_u64())
                    .unwrap_or(0) as u32;
                self.operator_profile.progress = Some(super::OperatorProfileProgressVm {
                    answered,
                    remaining: self
                        .operator_profile
                        .question
                        .as_ref()
                        .map(|_| 1u32)
                        .unwrap_or(0),
                    completion_ratio: 0.0,
                });
            }
        }
    }

    pub(in crate::app) fn handle_operator_model_summary_event(&mut self, model_json: String) {
        let pretty = serde_json::from_str::<serde_json::Value>(&model_json)
            .ok()
            .and_then(|value| serde_json::to_string_pretty(&value).ok())
            .unwrap_or(model_json);
        self.last_error = Some(pretty);
        self.error_active = true;
        self.modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::ErrorViewer));
        self.status_line = "Operator model snapshot loaded".to_string();
    }

    pub(in crate::app) fn handle_operator_model_reset_event(&mut self, ok: bool) {
        if ok {
            self.last_error = None;
            self.error_active = false;
            self.status_line = "Operator model reset".to_string();
        } else {
            self.last_error = Some("Operator model reset failed".to_string());
            self.error_active = true;
            self.modal
                .reduce(modal::ModalAction::Push(modal::ModalKind::ErrorViewer));
            self.status_line = "Operator model reset failed".to_string();
        }
    }

    pub(in crate::app) fn handle_collaboration_sessions_event(&mut self, sessions_json: String) {
        let pretty = serde_json::from_str::<serde_json::Value>(&sessions_json)
            .ok()
            .and_then(|value| serde_json::to_string_pretty(&value).ok())
            .unwrap_or(sessions_json);
        self.last_error = Some(pretty);
        self.error_active = true;
        self.modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::ErrorViewer));
        self.status_line = "Collaboration sessions loaded".to_string();
    }

    pub(in crate::app) fn handle_generated_tools_event(&mut self, tools_json: String) {
        let pretty = serde_json::from_str::<serde_json::Value>(&tools_json)
            .ok()
            .and_then(|value| serde_json::to_string_pretty(&value).ok())
            .unwrap_or(tools_json);
        self.last_error = Some(pretty);
        self.error_active = true;
        self.modal
            .reduce(modal::ModalAction::Push(modal::ModalKind::ErrorViewer));
        self.status_line = "Generated tools loaded".to_string();
    }

    pub(in crate::app) fn handle_operator_profile_session_completed_event(
        &mut self,
        session_id: String,
        updated_fields: Vec<String>,
    ) {
        self.operator_profile.loading = false;
        self.operator_profile.question = None;
        self.operator_profile.warning = None;
        self.operator_profile.visible = false;
        self.operator_profile.session_id = Some(session_id);
        self.operator_profile.progress = Some(super::OperatorProfileProgressVm {
            answered: updated_fields.len() as u32,
            remaining: 0,
            completion_ratio: 1.0,
        });
        self.input.reduce(input::InputAction::Clear);
        self.status_line = "Operator profile onboarding complete".to_string();
        self.show_input_notice(
            "Operator profile updated",
            InputNoticeKind::Success,
            120,
            true,
        );
        self.send_daemon_command(DaemonCommand::RequestConciergeWelcome);
    }

    pub(in crate::app) fn handle_error_event(&mut self, message: String) {
        let should_refresh_subagents = {
            let lowercase = message.to_ascii_lowercase();
            lowercase.contains("sub-agent")
                || lowercase.contains("subagent")
                || lowercase.contains("protected mutation")
                || lowercase.contains("reserved built-in")
                || lowercase.contains("weles")
        };
        let busy = self.assistant_busy();
        if busy {
            self.chat.reduce(chat::ChatAction::ForceStopStreaming);
        }
        self.agent_activity = None;
        self.clear_pending_stop();
        self.concierge
            .reduce(crate::state::ConciergeAction::WelcomeLoading(false));
        self.last_error = Some(message.clone());
        self.error_active = true;
        self.error_tick = self.tick_counter;
        if busy && self.modal.top().is_none() {
            if let Some(thread) = self.chat.active_thread_mut() {
                thread.messages.push(chat::AgentMessage {
                    role: chat::MessageRole::System,
                    content: format!("Error: {}", message),
                    ..Default::default()
                });
            }
        } else {
            self.status_line = "Error recorded. Press Ctrl+E for details".to_string();
        }
        if should_refresh_subagents {
            self.send_daemon_command(DaemonCommand::ListSubAgents);
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::app) fn handle_retry_status_event(
        &mut self,
        thread_id: String,
        phase: String,
        attempt: u32,
        max_retries: u32,
        delay_ms: u64,
        failure_class: String,
        message: String,
    ) {
        if phase == "cleared" {
            self.chat
                .reduce(chat::ChatAction::ClearRetryStatus { thread_id });
            self.retry_wait_start_selected = false;
            if !self.chat.is_streaming() {
                self.agent_activity = None;
            }
            return;
        }
        self.retry_wait_start_selected = false;
        self.agent_activity = Some(match phase.as_str() {
            "waiting" => "retry wait".to_string(),
            _ => "retrying".to_string(),
        });
        self.chat.reduce(chat::ChatAction::SetRetryStatus {
            thread_id,
            phase: if phase == "waiting" {
                chat::RetryPhase::Waiting
            } else {
                chat::RetryPhase::Retrying
            },
            attempt,
            max_retries,
            delay_ms,
            failure_class,
            message,
            received_at_tick: self.tick_counter,
        });
    }

    pub(in crate::app) fn handle_workflow_notice_event(
        &mut self,
        kind: String,
        message: String,
        details: Option<String>,
    ) {
        let details_ref = details.as_deref();
        if kind == "transport-fallback" {
            if let Some(details) = details_ref {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(details) {
                    if let Some(to) = parsed.get("to").and_then(|value| value.as_str()) {
                        self.config.api_transport = to.to_string();
                    }
                }
            }
        }
        self.status_line = if let Some(details) = details_ref {
            format!("{message} ({details})")
        } else {
            message.clone()
        };
        if kind == "operator-profile-warning" {
            let warning = if let Some(details) = details_ref {
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(details) {
                    parsed
                        .get("error")
                        .and_then(|value| value.as_str())
                        .map(str::to_string)
                        .unwrap_or_else(|| message.clone())
                } else {
                    details.to_string()
                }
            } else {
                message.clone()
            };
            self.operator_profile.warning = Some(warning);
            self.operator_profile.loading = false;
            self.show_input_notice(
                "operator profile warning (Ctrl+R to retry)",
                InputNoticeKind::Warning,
                120,
                false,
            );
        }
    }

    pub(in crate::app) fn handle_weles_health_update_event(
        &mut self,
        state: String,
        reason: Option<String>,
        checked_at: u64,
    ) {
        let degraded = state.eq_ignore_ascii_case("degraded");
        self.weles_health = Some(crate::client::WelesHealthVm {
            state,
            reason: reason.clone(),
            checked_at,
        });
        if degraded {
            let detail = reason
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "daemon vitality checks require attention".to_string());
            self.status_line = format!("WELES degraded: {detail}");
            let thread_id = self
                .chat
                .active_thread_id()
                .map(str::to_string)
                .unwrap_or_else(|| "local-weles-health".to_string());
            self.chat.reduce(chat::ChatAction::AppendMessage {
                thread_id,
                message: chat::AgentMessage {
                    role: chat::MessageRole::System,
                    content: format!("WELES degraded\n\n{detail}"),
                    ..Default::default()
                },
            });
        }
    }

    pub(in crate::app) fn handle_status_diagnostics_event(
        &mut self,
        operator_profile_sync_state: String,
        operator_profile_sync_dirty: bool,
        operator_profile_scheduler_fallback: bool,
    ) {
        if operator_profile_sync_dirty {
            self.status_line = format!(
                "Operator profile sync state: {} (retry with Ctrl+R)",
                operator_profile_sync_state
            );
            self.show_input_notice(
                format!(
                    "operator profile sync={} (Ctrl+R to retry)",
                    operator_profile_sync_state
                ),
                InputNoticeKind::Warning,
                120,
                false,
            );
        } else if operator_profile_scheduler_fallback {
            self.status_line =
                "Operator profile scheduler fallback active (contextual-only)".to_string();
            self.show_input_notice(
                "operator profile scheduler fallback active",
                InputNoticeKind::Warning,
                120,
                false,
            );
        }
    }

    pub(in crate::app) fn handle_agent_explanation_event(&mut self, payload: serde_json::Value) {
        let thread_id = self
            .chat
            .active_thread_id()
            .map(str::to_string)
            .unwrap_or_else(|| "local-explain".to_string());
        let content =
            serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string());
        self.chat.reduce(chat::ChatAction::AppendMessage {
            thread_id,
            message: chat::AgentMessage {
                role: chat::MessageRole::System,
                content: format!("Explainability\n\n{}", content),
                ..Default::default()
            },
        });
        self.status_line = "Explainability result received".to_string();
    }

    pub(in crate::app) fn handle_divergent_session_started_event(
        &mut self,
        payload: serde_json::Value,
    ) {
        let session_id = payload
            .get("session_id")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let thread_id = self
            .chat
            .active_thread_id()
            .map(str::to_string)
            .unwrap_or_else(|| "local-divergent".to_string());
        self.chat.reduce(chat::ChatAction::AppendMessage {
            thread_id,
            message: chat::AgentMessage {
                role: chat::MessageRole::System,
                content: if session_id.is_empty() {
                    "Divergent session started".to_string()
                } else {
                    format!(
                        "Divergent session started: `{}`\nUse `/diverge-get {}` to fetch results.",
                        session_id, session_id
                    )
                },
                ..Default::default()
            },
        });
        self.status_line = "Divergent session started".to_string();
    }

    pub(in crate::app) fn handle_divergent_session_event(&mut self, payload: serde_json::Value) {
        let thread_id = self
            .chat
            .active_thread_id()
            .map(str::to_string)
            .unwrap_or_else(|| "local-divergent".to_string());
        let content =
            serde_json::to_string_pretty(&payload).unwrap_or_else(|_| payload.to_string());
        self.chat.reduce(chat::ChatAction::AppendMessage {
            thread_id,
            message: chat::AgentMessage {
                role: chat::MessageRole::System,
                content: format!("Divergent session payload\n\n{}", content),
                ..Default::default()
            },
        });
        self.status_line = "Divergent session payload received".to_string();
    }
}
