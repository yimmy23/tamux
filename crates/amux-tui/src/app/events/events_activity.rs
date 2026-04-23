use super::events_audio::text_to_speech_result_path;
use super::*;

fn parse_workflow_notice_details(details: Option<&str>) -> Option<serde_json::Value> {
    serde_json::from_str::<serde_json::Value>(details?).ok()
}

fn auto_compaction_reload_window(details: Option<&str>) -> Option<(usize, usize)> {
    let parsed = parse_workflow_notice_details(details)?;
    let split_at = parsed.get("split_at")?.as_u64()? as usize;
    let total_message_count = parsed.get("total_message_count")?.as_u64()? as usize;
    Some((total_message_count.saturating_sub(split_at).max(1), 0))
}

fn normalized_skill_workflow_notice(
    kind: &str,
    message: &str,
    details: Option<&str>,
) -> Option<(String, String, Option<String>)> {
    let parsed = parse_workflow_notice_details(details);
    let recommended_skill = parsed
        .as_ref()
        .and_then(|value| value.get("recommended_skill"))
        .and_then(|value| value.as_str());
    let confidence_tier = parsed
        .as_ref()
        .and_then(|value| value.get("confidence_tier"))
        .and_then(|value| value.as_str());
    let recommended_action = parsed
        .as_ref()
        .and_then(|value| value.get("recommended_action"))
        .and_then(|value| value.as_str());
    let skip_rationale = parsed
        .as_ref()
        .and_then(|value| value.get("skip_rationale"))
        .and_then(|value| value.as_str());

    match kind {
        "skill-preflight" => {
            let normalized_kind = if confidence_tier == Some("strong") {
                "skill-discovery-required"
            } else {
                "skill-discovery-recommended"
            };
            let status = [
                if normalized_kind == "skill-discovery-required" {
                    Some("Skill gate required".to_string())
                } else {
                    Some("Skill guidance ready".to_string())
                },
                recommended_skill.map(|value| format!("skill={value}")),
                confidence_tier.map(|value| format!("confidence={value}")),
                recommended_action.map(|value| format!("next={value}")),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" | ");
            let activity = if normalized_kind == "skill-discovery-required" {
                Some("skill gate".to_string())
            } else {
                Some("skill review".to_string())
            };
            Some((normalized_kind.to_string(), status, activity))
        }
        "skill-gate" => {
            let status = [
                Some("Skill gate blocked progress".to_string()),
                recommended_skill.map(|value| format!("skill={value}")),
                recommended_action.map(|value| format!("next={value}")),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" | ");
            Some((
                "skill-discovery-required".to_string(),
                status,
                Some("skill gate".to_string()),
            ))
        }
        "skill-discovery-skipped" => {
            let status = [
                Some("Skill recommendation skipped".to_string()),
                recommended_skill.map(|value| format!("skill={value}")),
                skip_rationale.map(|value| format!("why={value}")),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" | ");
            Some((kind.to_string(), status, None))
        }
        "skill-community-scout" => {
            let candidates = parsed
                .as_ref()
                .and_then(|value| value.get("candidates"))
                .and_then(|value| value.as_array())
                .map(|value| value.len());
            let timeout = parsed
                .as_ref()
                .and_then(|value| value.get("community_preapprove_timeout_secs"))
                .and_then(|value| value.as_u64());
            let status = [
                Some("Community scout update".to_string()),
                candidates.map(|value| format!("candidates={value}")),
                timeout.map(|value| format!("timeout={}s", value)),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" | ");
            Some((kind.to_string(), status, Some("skill scout".to_string())))
        }
        "skill-discovery-required" | "skill-discovery-recommended" => Some((
            kind.to_string(),
            message.to_string(),
            Some(if kind == "skill-discovery-required" {
                "skill gate".to_string()
            } else {
                "skill review".to_string()
            }),
        )),
        _ => None,
    }
}

impl TuiModel {
    fn participant_playground_target(thread_id: &str) -> Option<(&str, &str)> {
        let remainder = thread_id.strip_prefix("playground:")?;
        let (participant_agent_id, visible_thread_id) = remainder.split_once(':')?;
        if participant_agent_id.is_empty() || visible_thread_id.is_empty() {
            return None;
        }
        Some((participant_agent_id, visible_thread_id))
    }

    fn fallback_participant_agent_name(agent_id: &str) -> String {
        let mut chars = agent_id.chars();
        match chars.next() {
            Some(first) => {
                let mut name = first.to_uppercase().collect::<String>();
                name.push_str(chars.as_str());
                name
            }
            None => "Participant".to_string(),
        }
    }

    fn resolve_participant_agent_name(
        &self,
        visible_thread_id: &str,
        participant_agent_id: &str,
    ) -> String {
        self.chat
            .threads()
            .iter()
            .find(|thread| thread.id == visible_thread_id)
            .and_then(|thread| {
                thread
                    .thread_participants
                    .iter()
                    .find(|participant| {
                        participant
                            .agent_id
                            .eq_ignore_ascii_case(participant_agent_id)
                    })
                    .map(|participant| participant.agent_name.clone())
            })
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| Self::fallback_participant_agent_name(participant_agent_id))
    }

    fn mark_participant_playground_active(&mut self, playground_thread_id: &str) -> bool {
        let Some((participant_agent_id, visible_thread_id)) =
            Self::participant_playground_target(playground_thread_id)
        else {
            return false;
        };

        let participant_agent_name =
            self.resolve_participant_agent_name(visible_thread_id, participant_agent_id);
        self.participant_playground_activity.insert(
            playground_thread_id.to_string(),
            super::ParticipantPlaygroundActivity {
                visible_thread_id: visible_thread_id.to_string(),
                participant_agent_id: participant_agent_id.to_string(),
                participant_agent_name,
            },
        );
        true
    }

    fn clear_participant_playground_activity(
        &mut self,
        playground_thread_id: &str,
    ) -> Option<String> {
        self.participant_playground_activity
            .remove(playground_thread_id)
            .map(|activity| activity.visible_thread_id)
    }

    pub(crate) fn participant_footer_activity(&self) -> Option<String> {
        let active_thread_id = self.chat.active_thread_id()?;
        let mut participants = self
            .participant_playground_activity
            .values()
            .filter(|activity| activity.visible_thread_id == active_thread_id)
            .map(|activity| {
                (
                    activity.participant_agent_id.to_ascii_lowercase(),
                    activity.participant_agent_name.clone(),
                )
            })
            .collect::<Vec<_>>();
        participants.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
        participants.dedup_by(|left, right| left.0 == right.0);

        let first = participants.first()?.1.clone();
        if participants.len() == 1 {
            Some(format!("{first} crafting response"))
        } else {
            Some(format!(
                "{first} +{} crafting responses",
                participants.len() - 1
            ))
        }
    }

    pub(crate) fn footer_activity_text(&self) -> Option<String> {
        self.current_thread_agent_activity()
            .map(str::to_string)
            .or_else(|| self.participant_footer_activity())
    }

    fn should_surface_thread_activity(&self, thread_id: &str) -> bool {
        match self.chat.active_thread_id() {
            None => true,
            Some(active_thread_id) => active_thread_id == thread_id,
        }
    }

    fn should_accept_retry_status_event(&self, thread_id: &str) -> bool {
        if self.chat.is_streaming()
            || self.chat.retry_status().is_some()
            || self.current_thread_agent_activity().is_some()
        {
            return true;
        }

        match self.chat.active_thread_id() {
            None => true,
            Some(active_thread_id) if active_thread_id != thread_id => false,
            Some(_) => self
                .chat
                .active_thread()
                .and_then(|thread| {
                    thread
                        .messages
                        .iter()
                        .rev()
                        .find(|message| {
                            !message.content.trim().is_empty()
                                && !matches!(message.role, chat::MessageRole::System)
                        })
                        .map(|message| message.role == chat::MessageRole::User)
                })
                .unwrap_or(false),
        }
    }

    pub(in crate::app) fn handle_delta_event(&mut self, thread_id: String, content: String) {
        if self.mark_participant_playground_active(&thread_id) {
            return;
        }
        if Self::is_hidden_agent_thread(&thread_id, None)
            || self.should_ignore_internal_thread_activity(&thread_id)
        {
            return;
        }
        self.clear_bootstrap_pending_activity_thread(thread_id.as_str());
        self.set_agent_activity_for(Some(thread_id.clone()), "writing");
        if self.should_surface_thread_activity(&thread_id) {
            self.anticipatory
                .reduce(crate::state::AnticipatoryAction::Clear);
        }
        self.chat
            .reduce(chat::ChatAction::Delta { thread_id, content });
    }

    pub(in crate::app) fn handle_reasoning_event(&mut self, thread_id: String, content: String) {
        if self.mark_participant_playground_active(&thread_id) {
            return;
        }
        if Self::is_hidden_agent_thread(&thread_id, None)
            || self.should_ignore_internal_thread_activity(&thread_id)
        {
            return;
        }
        self.clear_bootstrap_pending_activity_thread(thread_id.as_str());
        self.set_agent_activity_for(Some(thread_id.clone()), "reasoning");
        if self.should_surface_thread_activity(&thread_id) {
            self.anticipatory
                .reduce(crate::state::AnticipatoryAction::Clear);
        }
        let active_thread_id = thread_id.clone();
        self.reduce_chat_for_thread(
            Some(active_thread_id.as_str()),
            chat::ChatAction::Reasoning { thread_id, content },
        );
    }

    pub(in crate::app) fn handle_tool_call_event(
        &mut self,
        thread_id: String,
        call_id: String,
        name: String,
        arguments: String,
        weles_review: Option<crate::client::WelesReviewMetaVm>,
    ) {
        if self.mark_participant_playground_active(&thread_id) {
            return;
        }
        if Self::is_hidden_agent_thread(&thread_id, None)
            || self.should_ignore_internal_thread_activity(&thread_id)
        {
            return;
        }
        self.clear_bootstrap_pending_activity_thread(thread_id.as_str());
        self.set_agent_activity_for(Some(thread_id.clone()), format!("⚙  {}", name));
        if self.should_surface_thread_activity(&thread_id) {
            self.anticipatory
                .reduce(crate::state::AnticipatoryAction::Clear);
        }
        let active_thread_id = thread_id.clone();
        self.reduce_chat_for_thread(
            Some(active_thread_id.as_str()),
            chat::ChatAction::ToolCall {
                thread_id,
                call_id,
                name,
                args: arguments,
                weles_review,
            },
        );
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
        if self.mark_participant_playground_active(&thread_id) {
            return;
        }
        if Self::is_hidden_agent_thread(&thread_id, None)
            || self.should_ignore_internal_thread_activity(&thread_id)
        {
            return;
        }
        let tool_call_still_active = self
            .chat
            .thread_has_active_tool_call(thread_id.as_str(), call_id.as_str());
        if tool_call_still_active {
            self.clear_bootstrap_pending_activity_thread(thread_id.as_str());
            self.set_agent_activity_for(Some(thread_id.clone()), format!("⚙  {} ✓", name));
        }
        if tool_call_still_active && self.should_surface_thread_activity(&thread_id) {
            self.anticipatory
                .reduce(crate::state::AnticipatoryAction::Clear);
        }
        let maybe_tts_path = text_to_speech_result_path(&name, &content, is_error);
        let active_thread_id = thread_id.clone();
        self.reduce_chat_for_thread(
            Some(active_thread_id.as_str()),
            chat::ChatAction::ToolResult {
                thread_id,
                call_id,
                name,
                content,
                is_error,
                weles_review,
            },
        );
        if let Some(path) = maybe_tts_path {
            self.play_audio_path(&path);
        }
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
        if let Some(visible_thread_id) = self.clear_participant_playground_activity(&thread_id) {
            if self.chat.active_thread_id() == Some(visible_thread_id.as_str()) {
                self.request_authoritative_thread_refresh(visible_thread_id, false);
            }
            return;
        }
        if Self::is_hidden_agent_thread(&thread_id, None)
            || self.should_ignore_internal_thread_activity(&thread_id)
        {
            return;
        }
        self.clear_bootstrap_pending_activity_thread(thread_id.as_str());
        self.clear_pending_prompt_response_thread(thread_id.as_str());
        self.clear_agent_activity_for(Some(thread_id.as_str()));
        if self.should_surface_thread_activity(&thread_id) {
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
        }
        self.reduce_chat_for_thread(
            Some(thread_id.as_str()),
            chat::ChatAction::TurnDone {
                thread_id: thread_id.clone(),
                input_tokens,
                output_tokens,
                cost,
                provider,
                model,
                tps,
                generation_ms,
                reasoning,
                provider_final_result_json,
            },
        );

        let _ = self.maybe_request_auto_response_for_open_thread(&thread_id);
        let _ = self.maybe_auto_send_always_auto_response();
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

    pub(in crate::app) fn handle_operator_question_event(
        &mut self,
        question_id: String,
        content: String,
        options: Vec<String>,
        thread_id: Option<String>,
    ) {
        let target_thread_id =
            thread_id.or_else(|| self.chat.active_thread_id().map(str::to_string));
        let Some(target_thread_id) = target_thread_id else {
            return;
        };

        self.reduce_chat_for_thread(
            Some(target_thread_id.as_str()),
            chat::ChatAction::AppendMessage {
                thread_id: target_thread_id.clone(),
                message: chat::AgentMessage {
                    role: chat::MessageRole::Assistant,
                    content,
                    is_operator_question: true,
                    operator_question_id: Some(question_id.clone()),
                    actions: options
                        .into_iter()
                        .map(|option| chat::MessageAction {
                            label: option.clone(),
                            action_type: format!("operator_question_answer:{question_id}:{option}"),
                            thread_id: Some(target_thread_id.clone()),
                        })
                        .collect(),
                    ..Default::default()
                },
            },
        );
        self.status_line = "Operator question ready".to_string();
    }

    pub(in crate::app) fn handle_operator_question_resolved_event(
        &mut self,
        question_id: String,
        answer: String,
    ) {
        if self
            .chat
            .resolve_operator_question_answer(&question_id, answer.clone())
        {
            self.status_line = format!("Operator question answered: {answer}");
        }
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
        let sessions = serde_json::from_str::<serde_json::Value>(&sessions_json)
            .ok()
            .and_then(parse_collaboration_sessions)
            .unwrap_or_default();
        let escalation_notice = sessions.iter().find_map(|session| {
            session
                .escalation
                .as_ref()
                .map(|escalation| escalation.reason.clone())
        });
        self.main_pane_view = MainPaneView::Collaboration;
        self.collaboration
            .reduce(CollaborationAction::SessionsLoaded(sessions));
        if self
            .collaboration
            .rows()
            .get(1)
            .and_then(CollaborationRowVm::disagreement_id)
            .is_some()
        {
            self.collaboration.reduce(CollaborationAction::SelectRow(1));
        }
        self.last_error = None;
        self.error_active = false;
        self.status_line = "Collaboration sessions loaded".to_string();
        if let Some(reason) = escalation_notice {
            self.show_input_notice(
                format!("Collaboration escalation: {reason}"),
                InputNoticeKind::Warning,
                120,
                true,
            );
        }
    }

    pub(in crate::app) fn handle_collaboration_vote_result_event(&mut self, report_json: String) {
        let resolution = serde_json::from_str::<serde_json::Value>(&report_json)
            .ok()
            .and_then(|value| {
                value
                    .get("resolution")
                    .and_then(serde_json::Value::as_str)
                    .map(ToOwned::to_owned)
            })
            .unwrap_or_else(|| "updated".to_string());
        self.status_line = format!("Vote recorded: {resolution}.");
        self.send_daemon_command(DaemonCommand::GetCollaborationSessions);
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
        self.request_concierge_welcome();
    }

    pub(in crate::app) fn handle_error_event(&mut self, message: String) {
        if self.status_modal_loading {
            self.status_modal_loading = false;
            self.status_modal_error = Some(message.clone());
        }
        if self.prompt_modal_loading {
            self.prompt_modal_loading = false;
            self.prompt_modal_error = Some(message.clone());
            self.prompt_modal_scroll = 0;
        }
        if self.statistics_modal_loading {
            self.statistics_modal_loading = false;
            self.statistics_modal_error = Some(message.clone());
            self.statistics_modal_scroll = 0;
        }
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
        self.bootstrap_pending_activity_threads.clear();
        self.pending_prompt_response_threads.clear();
        self.clear_active_thread_activity();
        self.clear_pending_stop();
        self.concierge
            .reduce(crate::state::ConciergeAction::WelcomeLoading(false));
        self.last_error = Some(message.clone());
        self.error_active = true;
        self.error_tick = self.tick_counter;
        if busy && self.modal.top().is_none() {
            if let Some(thread_id) = self.chat.active_thread_id().map(str::to_string) {
                let active_thread_id = thread_id.clone();
                self.reduce_chat_for_thread(
                    Some(active_thread_id.as_str()),
                    chat::ChatAction::AppendMessage {
                        thread_id,
                        message: chat::AgentMessage {
                            role: chat::MessageRole::System,
                            content: format!("Error: {}", message),
                            ..Default::default()
                        },
                    },
                );
            }
        } else {
            self.status_line = "Error recorded. Press Ctrl+E for details".to_string();
        }
        if should_refresh_subagents {
            self.send_daemon_command(DaemonCommand::ListSubAgents);
        }
    }

    pub(in crate::app) fn handle_thread_message_pin_result_event(
        &mut self,
        result: crate::client::ThreadMessagePinResultVm,
    ) {
        if result.ok {
            return;
        }

        if result.error.as_deref() == Some("pinned_budget_exceeded") {
            if let Some(candidate_pinned_chars) = result.candidate_pinned_chars {
                self.open_pinned_budget_exceeded_modal(PendingPinnedBudgetExceeded {
                    thread_id: result.thread_id,
                    message_id: result.message_id,
                    current_pinned_chars: result.current_pinned_chars,
                    pinned_budget_chars: result.pinned_budget_chars,
                    candidate_pinned_chars,
                });
                return;
            }
        }

        self.status_line = format!(
            "Pin failed: {}",
            result.error.unwrap_or_else(|| "unknown_error".to_string())
        );
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
        self.clear_bootstrap_pending_activity_thread(thread_id.as_str());
        self.clear_pending_prompt_response_thread(thread_id.as_str());
        if phase == "cleared" {
            self.reduce_chat_for_thread(
                Some(thread_id.as_str()),
                chat::ChatAction::ClearRetryStatus {
                    thread_id: thread_id.clone(),
                },
            );
            self.retry_wait_start_selected = false;
            self.clear_agent_activity_for(Some(thread_id.as_str()));
            return;
        }
        if phase != "waiting" && !self.should_accept_retry_status_event(thread_id.as_str()) {
            return;
        }
        self.retry_wait_start_selected = false;
        self.set_agent_activity_for(
            Some(thread_id.clone()),
            match phase.as_str() {
                "waiting" => "retry wait",
                _ => "retrying",
            },
        );
        let active_thread_id = thread_id.clone();
        self.reduce_chat_for_thread(
            Some(active_thread_id.as_str()),
            chat::ChatAction::SetRetryStatus {
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
            },
        );
    }

    pub(in crate::app) fn handle_workflow_notice_event(
        &mut self,
        thread_id: Option<String>,
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
        if let Some((_normalized_kind, status_line, agent_activity)) =
            normalized_skill_workflow_notice(&kind, &message, details_ref)
        {
            self.status_line = status_line;
            if let Some(agent_activity) = agent_activity {
                self.set_agent_activity_for(thread_id.clone(), agent_activity);
            }
        } else {
            self.status_line = if let Some(details) = details_ref {
                format!("{message} ({details})")
            } else {
                message.clone()
            };
        }
        if kind == "auto-compaction" || kind == "manual-compaction" {
            if let (
                Some(thread_id),
                Some(active_thread_id),
                Some((message_limit, message_offset)),
            ) = (
                thread_id.as_deref(),
                self.chat.active_thread_id(),
                auto_compaction_reload_window(details_ref),
            ) {
                if thread_id == active_thread_id {
                    self.request_thread_page(
                        thread_id.to_string(),
                        message_limit,
                        message_offset,
                        false,
                    );
                }
            }
        }
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
            let active_thread_id = thread_id.clone();
            self.reduce_chat_for_thread(
                Some(active_thread_id.as_str()),
                chat::ChatAction::AppendMessage {
                    thread_id,
                    message: chat::AgentMessage {
                        role: chat::MessageRole::System,
                        content: format!("WELES degraded\n\n{detail}"),
                        ..Default::default()
                    },
                },
            );
        }
    }

    pub(in crate::app) fn handle_status_diagnostics_event(
        &mut self,
        operator_profile_sync_state: String,
        operator_profile_sync_dirty: bool,
        operator_profile_scheduler_fallback: bool,
        diagnostics_json: String,
    ) {
        let parsed = serde_json::from_str::<serde_json::Value>(&diagnostics_json).ok();
        self.status_modal_diagnostics_json = Some(diagnostics_json);
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
        } else if let Some(mesh_state) = parsed
            .as_ref()
            .and_then(|value| value.get("skill_mesh"))
            .and_then(|value| value.get("state"))
            .and_then(|value| value.as_str())
            .filter(|state| *state != "fresh" && *state != "legacy")
        {
            self.status_line = format!("skill mesh: {mesh_state}");
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
        let active_thread_id = thread_id.clone();
        self.reduce_chat_for_thread(
            Some(active_thread_id.as_str()),
            chat::ChatAction::AppendMessage {
                thread_id,
                message: chat::AgentMessage {
                    role: chat::MessageRole::System,
                    content: format!("Explainability\n\n{}", content),
                    ..Default::default()
                },
            },
        );
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
        let active_thread_id = thread_id.clone();
        self.reduce_chat_for_thread(
            Some(active_thread_id.as_str()),
            chat::ChatAction::AppendMessage {
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
            },
        );
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
        let active_thread_id = thread_id.clone();
        self.reduce_chat_for_thread(
            Some(active_thread_id.as_str()),
            chat::ChatAction::AppendMessage {
                thread_id,
                message: chat::AgentMessage {
                    role: chat::MessageRole::System,
                    content: format!("Divergent session payload\n\n{}", content),
                    ..Default::default()
                },
            },
        );
        self.status_line = "Divergent session payload received".to_string();
    }
}

fn parse_collaboration_sessions(value: serde_json::Value) -> Option<Vec<CollaborationSessionVm>> {
    let items = value.as_array()?;
    Some(
        items
            .iter()
            .filter_map(|session| {
                let id = session.get("id")?.as_str()?.to_string();
                let disagreement_values = session
                    .get("disagreements")
                    .and_then(serde_json::Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let disagreements = session
                    .get("disagreements")
                    .and_then(serde_json::Value::as_array)
                    .map(|items| {
                        items
                            .iter()
                            .filter_map(|disagreement| {
                                Some(CollaborationDisagreementVm {
                                    id: disagreement.get("id")?.as_str()?.to_string(),
                                    topic: disagreement
                                        .get("topic")
                                        .and_then(serde_json::Value::as_str)
                                        .unwrap_or("disagreement")
                                        .to_string(),
                                    positions: disagreement
                                        .get("positions")
                                        .and_then(serde_json::Value::as_array)
                                        .map(|positions| {
                                            positions
                                                .iter()
                                                .filter_map(|position| {
                                                    position.as_str().map(ToOwned::to_owned)
                                                })
                                                .collect::<Vec<_>>()
                                        })
                                        .unwrap_or_default(),
                                    vote_count: disagreement
                                        .get("votes")
                                        .and_then(serde_json::Value::as_array)
                                        .map(|votes| votes.len())
                                        .unwrap_or(0),
                                    resolution: disagreement
                                        .get("resolution")
                                        .and_then(serde_json::Value::as_str)
                                        .map(ToOwned::to_owned),
                                })
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                let escalation = disagreement_values.iter().find_map(|disagreement| {
                    let resolution = disagreement
                        .get("resolution")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("pending");
                    let confidence_gap = disagreement
                        .get("confidence_gap")
                        .and_then(serde_json::Value::as_f64)
                        .unwrap_or(1.0);
                    if resolution == "escalated"
                        || (resolution == "pending" && confidence_gap < 0.15)
                    {
                        Some(CollaborationEscalationVm {
                            from_level: "L1".to_string(),
                            to_level: if resolution == "escalated" {
                                "L2".to_string()
                            } else {
                                "L1".to_string()
                            },
                            reason: disagreement
                                .get("topic")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("subagent disagreement requires attention")
                                .to_string(),
                            attempts: 1,
                        })
                    } else {
                        None
                    }
                });
                Some(CollaborationSessionVm {
                    id,
                    parent_task_id: session
                        .get("parent_task_id")
                        .and_then(serde_json::Value::as_str)
                        .map(ToOwned::to_owned),
                    parent_thread_id: session
                        .get("parent_thread_id")
                        .and_then(serde_json::Value::as_str)
                        .map(ToOwned::to_owned),
                    agent_count: session
                        .get("agents")
                        .and_then(serde_json::Value::as_array)
                        .map(|agents| agents.len())
                        .unwrap_or(0),
                    disagreement_count: disagreements.len(),
                    consensus_summary: session
                        .get("consensus")
                        .and_then(|consensus| consensus.get("summary"))
                        .and_then(serde_json::Value::as_str)
                        .map(ToOwned::to_owned),
                    escalation,
                    disagreements,
                })
            })
            .collect(),
    )
}
