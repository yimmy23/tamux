use super::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadParticipantStatus {
    Active,
    Inactive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadParticipantCommandAction {
    Upsert,
    Deactivate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadParticipantState {
    pub agent_id: String,
    pub agent_name: String,
    pub instruction: String,
    pub status: ThreadParticipantStatus,
    pub created_at: u64,
    pub updated_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deactivated_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_contribution_at: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadParticipantSuggestionStatus {
    Queued,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadParticipantSuggestion {
    pub id: String,
    pub target_agent_id: String,
    pub target_agent_name: String,
    pub instruction: String,
    #[serde(default)]
    pub force_send: bool,
    pub status: ThreadParticipantSuggestionStatus,
    pub created_at: u64,
    pub updated_at: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

pub(super) fn normalize_thread_participants(
    participants: Vec<ThreadParticipantState>,
) -> Vec<ThreadParticipantState> {
    participants
        .into_iter()
        .map(|mut participant| {
            let canonical_id = canonical_agent_id(&participant.agent_id).to_string();
            participant.agent_id = canonical_id.clone();
            if participant.agent_name.trim().is_empty() {
                participant.agent_name = canonical_agent_name(&canonical_id).to_string();
            }
            participant
        })
        .collect()
}

fn is_known_main_agent_alias(alias: &str) -> bool {
    matches!(
        alias.trim().to_ascii_lowercase().as_str(),
        MAIN_AGENT_ID | MAIN_AGENT_ALIAS | MAIN_AGENT_LEGACY_ALIAS | MAIN_AGENT_FALLBACK_ALIAS
    )
}

fn is_known_builtin_agent_alias(alias: &str) -> bool {
    let trimmed = alias.trim();
    !trimmed.is_empty()
        && (canonical_agent_id(trimmed) != MAIN_AGENT_ID || is_known_main_agent_alias(trimmed))
}

fn is_participant_deactivation_action(action: &str) -> bool {
    matches!(
        action.trim().to_ascii_lowercase().as_str(),
        "deactivate" | "leave" | "stop" | "done" | "return"
    )
}

impl AgentEngine {
    async fn resolve_thread_participant_target(&self, alias: &str) -> Result<(String, String)> {
        let trimmed = alias.trim();
        if trimmed.is_empty() {
            anyhow::bail!("target agent cannot be empty");
        }

        if is_known_builtin_agent_alias(trimmed) {
            let canonical_id = canonical_agent_id(trimmed).to_string();
            return Ok((
                canonical_id.clone(),
                canonical_agent_name(&canonical_id).to_string(),
            ));
        }

        let normalized = trimmed.to_ascii_lowercase();
        if let Some(sub_agent) = self.list_sub_agents().await.into_iter().find(|entry| {
            entry.id.eq_ignore_ascii_case(&normalized) || entry.name.eq_ignore_ascii_case(trimmed)
        }) {
            return Ok((sub_agent.id, sub_agent.name));
        }

        anyhow::bail!("unknown agent target: {trimmed}");
    }

    pub async fn list_thread_participants(&self, thread_id: &str) -> Vec<ThreadParticipantState> {
        self.thread_participants
            .read()
            .await
            .get(thread_id)
            .cloned()
            .unwrap_or_default()
    }

    pub async fn list_thread_participant_suggestions(
        &self,
        thread_id: &str,
    ) -> Vec<ThreadParticipantSuggestion> {
        self.thread_participant_suggestions
            .read()
            .await
            .get(thread_id)
            .cloned()
            .unwrap_or_default()
    }

    pub async fn queue_thread_participant_suggestion(
        &self,
        thread_id: &str,
        target_agent_id: &str,
        instruction: &str,
        force_send: bool,
    ) -> Result<ThreadParticipantSuggestion> {
        if !self.threads.read().await.contains_key(thread_id) {
            anyhow::bail!("thread not found: {thread_id}");
        }

        let trimmed_instruction = instruction.trim();
        if trimmed_instruction.is_empty() {
            anyhow::bail!("participant suggestion instruction cannot be empty");
        }

        let (agent_id, agent_name) = self
            .resolve_thread_participant_target(target_agent_id)
            .await?;
        let now = now_millis();
        let suggestion = ThreadParticipantSuggestion {
            id: uuid::Uuid::new_v4().to_string(),
            target_agent_id: agent_id,
            target_agent_name: agent_name,
            instruction: trimmed_instruction.to_string(),
            force_send,
            status: ThreadParticipantSuggestionStatus::Queued,
            created_at: now,
            updated_at: now,
            error: None,
        };

        self.thread_participant_suggestions
            .write()
            .await
            .entry(thread_id.to_string())
            .or_default()
            .push(suggestion.clone());

        self.persist_thread_by_id(thread_id).await;
        let _ = self.event_tx.send(AgentEvent::ParticipantSuggestion {
            thread_id: thread_id.to_string(),
            suggestion: suggestion.clone(),
        });
        let _ = self.event_tx.send(AgentEvent::ThreadReloadRequired {
            thread_id: thread_id.to_string(),
        });
        let _ = self
            .record_behavioral_event(
                "participant_suggestion",
                BehavioralEventContext {
                    thread_id: Some(thread_id),
                    task_id: None,
                    goal_run_id: None,
                    approval_id: None,
                },
                serde_json::json!({
                    "action": "queued",
                    "suggestion": suggestion,
                }),
            )
            .await;

        if suggestion.force_send {
            self.send_thread_participant_suggestion(thread_id, &suggestion.id, None)
                .await?;
        }

        Ok(suggestion)
    }

    pub async fn dismiss_thread_participant_suggestion(
        &self,
        thread_id: &str,
        suggestion_id: &str,
    ) -> Result<bool> {
        if !self.threads.read().await.contains_key(thread_id) {
            anyhow::bail!("thread not found: {thread_id}");
        }

        let mut suggestions = self.thread_participant_suggestions.write().await;
        let (changed, removed_suggestion, remove_entry) = {
            let Some(entry) = suggestions.get_mut(thread_id) else {
                return Ok(false);
            };
            let initial_len = entry.len();
            let removed_suggestion = entry
                .iter()
                .find(|suggestion| suggestion.id == suggestion_id)
                .cloned();
            entry.retain(|suggestion| suggestion.id != suggestion_id);
            (
                entry.len() != initial_len,
                removed_suggestion,
                entry.is_empty(),
            )
        };
        if remove_entry {
            suggestions.remove(thread_id);
        }
        drop(suggestions);

        if changed {
            self.persist_thread_by_id(thread_id).await;
            let _ = self.event_tx.send(AgentEvent::ThreadReloadRequired {
                thread_id: thread_id.to_string(),
            });
            if let Some(suggestion) = removed_suggestion {
                let _ = self
                    .record_behavioral_event(
                        "participant_suggestion",
                        BehavioralEventContext {
                            thread_id: Some(thread_id),
                            task_id: None,
                            goal_run_id: None,
                            approval_id: None,
                        },
                        serde_json::json!({
                            "action": "dismissed",
                            "suggestion": suggestion,
                        }),
                    )
                    .await;
            }
        }

        Ok(changed)
    }

    pub async fn fail_thread_participant_suggestion(
        &self,
        thread_id: &str,
        suggestion_id: &str,
        error: &str,
    ) -> Result<Option<ThreadParticipantSuggestion>> {
        if !self.threads.read().await.contains_key(thread_id) {
            anyhow::bail!("thread not found: {thread_id}");
        }

        let trimmed_error = error.trim();
        let mut suggestions = self.thread_participant_suggestions.write().await;
        let updated = suggestions
            .get_mut(thread_id)
            .and_then(|entry| {
                entry
                    .iter_mut()
                    .find(|suggestion| suggestion.id == suggestion_id)
            })
            .map(|suggestion| {
                suggestion.status = ThreadParticipantSuggestionStatus::Failed;
                suggestion.updated_at = now_millis();
                suggestion.error = (!trimmed_error.is_empty()).then(|| trimmed_error.to_string());
                suggestion.clone()
            });
        drop(suggestions);

        if updated.is_some() {
            self.persist_thread_by_id(thread_id).await;
            if let Some(suggestion) = updated.clone() {
                let _ = self.event_tx.send(AgentEvent::ParticipantSuggestion {
                    thread_id: thread_id.to_string(),
                    suggestion: suggestion.clone(),
                });
                let _ = self
                    .record_behavioral_event(
                        "participant_suggestion",
                        BehavioralEventContext {
                            thread_id: Some(thread_id),
                            task_id: None,
                            goal_run_id: None,
                            approval_id: None,
                        },
                        serde_json::json!({
                            "action": "failed",
                            "suggestion": suggestion,
                        }),
                    )
                    .await;
            }
            let _ = self.event_tx.send(AgentEvent::ThreadReloadRequired {
                thread_id: thread_id.to_string(),
            });
        }

        Ok(updated)
    }

    pub async fn send_thread_participant_suggestion(
        &self,
        thread_id: &str,
        suggestion_id: &str,
        preferred_session_hint: Option<&str>,
    ) -> Result<bool> {
        if !self.threads.read().await.contains_key(thread_id) {
            anyhow::bail!("thread not found: {thread_id}");
        }

        let suggestion = self
            .list_thread_participant_suggestions(thread_id)
            .await
            .into_iter()
            .find(|entry| entry.id == suggestion_id)
            .ok_or_else(|| anyhow::anyhow!("participant suggestion not found: {suggestion_id}"))?;

        match self
            .append_visible_thread_participant_message(
                thread_id,
                &suggestion.target_agent_id,
                &suggestion.instruction,
            )
            .await
        {
            Ok(()) => {
                let _ = self
                    .dismiss_thread_participant_suggestion(thread_id, suggestion_id)
                    .await?;
                let _ = self
                    .record_behavioral_event(
                        "participant_suggestion",
                        BehavioralEventContext {
                            thread_id: Some(thread_id),
                            task_id: None,
                            goal_run_id: None,
                            approval_id: None,
                        },
                        serde_json::json!({
                            "action": "sent",
                            "suggestion": suggestion,
                        }),
                    )
                    .await;
                Ok(true)
            }
            Err(error) => {
                let _ = self
                    .fail_thread_participant_suggestion(
                        thread_id,
                        suggestion_id,
                        &error.to_string(),
                    )
                    .await?;
                Err(error)
            }
        }
    }

    pub async fn agent_thread_detail_json(&self, thread_id: &str) -> String {
        let thread = self.get_thread(thread_id).await;
        let mut value = serde_json::to_value(thread).unwrap_or(serde_json::Value::Null);

        if let Some(detail) = value.as_object_mut() {
            let participants = self.list_thread_participants(thread_id).await;
            let suggestions = self.list_thread_participant_suggestions(thread_id).await;
            detail.insert(
                "thread_participants".to_string(),
                serde_json::to_value(participants).unwrap_or(serde_json::Value::Array(Vec::new())),
            );
            detail.insert(
                "queued_participant_suggestions".to_string(),
                serde_json::to_value(suggestions).unwrap_or(serde_json::Value::Array(Vec::new())),
            );
        }

        serde_json::to_string(&value).unwrap_or_else(|_| "null".to_string())
    }

    pub async fn upsert_thread_participant(
        &self,
        thread_id: &str,
        target_agent_id: &str,
        instruction: &str,
    ) -> Result<ThreadParticipantState> {
        if !self.threads.read().await.contains_key(thread_id) {
            anyhow::bail!("thread not found: {thread_id}");
        }

        let trimmed_instruction = instruction.trim();
        if trimmed_instruction.is_empty() {
            anyhow::bail!("participant instruction cannot be empty");
        }

        let (agent_id, agent_name) = self
            .resolve_thread_participant_target(target_agent_id)
            .await?;
        let now = now_millis();
        let mut participants = self.thread_participants.write().await;
        let entry = participants.entry(thread_id.to_string()).or_default();
        let updated = if let Some(existing) = entry
            .iter_mut()
            .find(|participant| participant.agent_id.eq_ignore_ascii_case(&agent_id))
        {
            existing.agent_id = agent_id.clone();
            existing.agent_name = agent_name.clone();
            existing.instruction = trimmed_instruction.to_string();
            existing.status = ThreadParticipantStatus::Active;
            existing.updated_at = now;
            existing.deactivated_at = None;
            existing.clone()
        } else {
            let state = ThreadParticipantState {
                agent_id: agent_id.clone(),
                agent_name: agent_name.clone(),
                instruction: trimmed_instruction.to_string(),
                status: ThreadParticipantStatus::Active,
                created_at: now,
                updated_at: now,
                deactivated_at: None,
                last_contribution_at: None,
            };
            entry.push(state.clone());
            state
        };
        *entry = normalize_thread_participants(entry.clone());
        drop(participants);

        self.persist_thread_by_id(thread_id).await;
        let _ = self.event_tx.send(AgentEvent::ThreadReloadRequired {
            thread_id: thread_id.to_string(),
        });

        Ok(updated)
    }

    pub async fn deactivate_thread_participant(
        &self,
        thread_id: &str,
        target_agent_id: &str,
    ) -> Result<Option<ThreadParticipantState>> {
        if !self.threads.read().await.contains_key(thread_id) {
            anyhow::bail!("thread not found: {thread_id}");
        }

        let (agent_id, _) = self
            .resolve_thread_participant_target(target_agent_id)
            .await?;
        let now = now_millis();
        let mut participants = self.thread_participants.write().await;
        let Some(entry) = participants.get_mut(thread_id) else {
            return Ok(None);
        };
        let updated = entry
            .iter_mut()
            .find(|participant| participant.agent_id.eq_ignore_ascii_case(&agent_id))
            .map(|participant| {
                participant.status = ThreadParticipantStatus::Inactive;
                participant.updated_at = now;
                participant.deactivated_at = Some(now);
                participant.clone()
            });
        drop(participants);

        if updated.is_some() {
            self.persist_thread_by_id(thread_id).await;
            let _ = self.event_tx.send(AgentEvent::ThreadReloadRequired {
                thread_id: thread_id.to_string(),
            });
        }

        Ok(updated)
    }

    pub async fn apply_thread_participant_command(
        &self,
        thread_id: &str,
        target_agent_id: &str,
        action: &str,
        instruction: Option<&str>,
    ) -> Result<()> {
        if is_participant_deactivation_action(action) {
            let _ = self
                .deactivate_thread_participant(thread_id, target_agent_id)
                .await?;
            return Ok(());
        }

        self.upsert_thread_participant(
            thread_id,
            target_agent_id,
            instruction.unwrap_or("").trim(),
        )
        .await?;
        if let Err(error) = self.run_participant_observers(thread_id).await {
            tracing::warn!(
                thread_id = %thread_id,
                participant = %target_agent_id,
                error = %error,
                "participant initial observer failed"
            );
            let _ = self.event_tx.send(AgentEvent::WorkflowNotice {
                thread_id: thread_id.to_string(),
                kind: "participant_observer_error".to_string(),
                message: "participant observers failed".to_string(),
                details: Some(error.to_string()),
            });
        }
        Ok(())
    }

    pub async fn send_internal_delegate_message(
        &self,
        thread_id: Option<&str>,
        target_agent_id: &str,
        preferred_session_hint: Option<&str>,
        content: &str,
    ) -> Result<()> {
        let (resolved_target_id, _) = self
            .resolve_thread_participant_target(target_agent_id)
            .await?;
        let sender = match thread_id {
            Some(thread_id) => self
                .thread_handoff_state(thread_id)
                .await
                .map(|state| state.active_agent_id)
                .unwrap_or_else(|| MAIN_AGENT_ID.to_string()),
            None => MAIN_AGENT_ID.to_string(),
        };

        let mut payload = String::new();
        if let Some(thread_id) = thread_id {
            payload.push_str(&format!(
                "Visible thread id: {thread_id}\nThread delegation mode: internal_hidden\n\n"
            ));
            if let Some(thread) = self.get_thread(thread_id).await {
                payload.push_str(&format!("Visible thread title: {}\n", thread.title.trim()));
                let recent_messages = thread
                    .messages
                    .iter()
                    .rev()
                    .take(8)
                    .cloned()
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<Vec<_>>();
                if !recent_messages.is_empty() {
                    payload.push_str("Recent visible thread messages:\n");
                    for message in recent_messages {
                        let role = match message.role {
                            MessageRole::Assistant => "assistant",
                            MessageRole::System => "system",
                            MessageRole::Tool => "tool",
                            _ => "user",
                        };
                        payload.push_str(&format!("- {role}: {}\n", message.content.trim()));
                    }
                    payload.push('\n');
                }
            }
        }
        payload.push_str("Delegation request:\n");
        payload.push_str(content.trim());

        self.send_internal_agent_message(
            &sender,
            &resolved_target_id,
            &payload,
            preferred_session_hint,
        )
        .await?;

        Ok(())
    }

    pub async fn append_visible_thread_participant_message(
        &self,
        thread_id: &str,
        target_agent_id: &str,
        content: &str,
    ) -> Result<()> {
        if !self.threads.read().await.contains_key(thread_id) {
            anyhow::bail!("thread not found: {thread_id}");
        }

        let trimmed_content = content.trim();
        if trimmed_content.is_empty() {
            anyhow::bail!("participant message content cannot be empty");
        }

        let (agent_id, agent_name) = self
            .resolve_thread_participant_target(target_agent_id)
            .await?;

        let participant_exists =
            self.list_thread_participants(thread_id)
                .await
                .iter()
                .any(|participant| {
                    participant.agent_id.eq_ignore_ascii_case(&agent_id)
                        && participant.status == ThreadParticipantStatus::Active
                });
        if !participant_exists {
            anyhow::bail!("participant is not active on thread: {agent_id}");
        }

        let now = now_millis();
        {
            let mut threads = self.threads.write().await;
            let thread = threads
                .get_mut(thread_id)
                .ok_or_else(|| anyhow::anyhow!("thread not found: {thread_id}"))?;
            thread.messages.push(AgentMessage {
                id: generate_message_id(),
                role: MessageRole::Assistant,
                content: trimmed_content.to_string(),
                tool_calls: None,
                tool_call_id: None,
                tool_name: None,
                tool_arguments: None,
                tool_status: None,
                weles_review: None,
                input_tokens: 0,
                output_tokens: 0,
                provider: None,
                model: None,
                api_transport: None,
                response_id: None,
                upstream_message: None,
                provider_final_result: None,
                author_agent_id: Some(agent_id.clone()),
                author_agent_name: Some(agent_name.clone()),
                reasoning: None,
                message_kind: AgentMessageKind::Normal,
                compaction_strategy: None,
                compaction_payload: None,
                offloaded_payload_id: None,
                structural_refs: Vec::new(),
                timestamp: now,
            });
            thread.updated_at = now;
        }

        {
            let mut participants = self.thread_participants.write().await;
            if let Some(entry) = participants.get_mut(thread_id) {
                if let Some(participant) = entry
                    .iter_mut()
                    .find(|participant| participant.agent_id.eq_ignore_ascii_case(&agent_id))
                {
                    participant.last_contribution_at = Some(now);
                    participant.updated_at = now;
                    participant.status = ThreadParticipantStatus::Active;
                }
            }
        }

        self.persist_thread_by_id(thread_id).await;
        let _ = self.event_tx.send(AgentEvent::ThreadReloadRequired {
            thread_id: thread_id.to_string(),
        });

        Ok(())
    }

    pub async fn send_visible_thread_participant_message(
        &self,
        thread_id: &str,
        target_agent_id: &str,
        preferred_session_hint: Option<&str>,
        content: &str,
    ) -> Result<()> {
        if !self.threads.read().await.contains_key(thread_id) {
            anyhow::bail!("thread not found: {thread_id}");
        }

        let trimmed_content = content.trim();
        if trimmed_content.is_empty() {
            anyhow::bail!("participant message content cannot be empty");
        }

        let (agent_id, agent_name) = self
            .resolve_thread_participant_target(target_agent_id)
            .await?;

        let participant_exists =
            self.list_thread_participants(thread_id)
                .await
                .iter()
                .any(|participant| {
                    participant.agent_id.eq_ignore_ascii_case(&agent_id)
                        && participant.status == ThreadParticipantStatus::Active
                });
        if !participant_exists {
            anyhow::bail!("participant is not active on thread: {agent_id}");
        }

        let existing_message_ids: HashSet<String> = {
            let threads = self.threads.read().await;
            threads
                .get(thread_id)
                .map(|thread| {
                    thread
                        .messages
                        .iter()
                        .map(|message| message.id.clone())
                        .collect()
                })
                .unwrap_or_default()
        };
        let client_surface = self.get_thread_client_surface(thread_id).await;

        Box::pin(self.send_message_inner(
            Some(thread_id),
            trimmed_content,
            None,
            preferred_session_hint,
            None,
            None,
            None,
            client_surface,
            true,
        ))
        .await?;

        let now = now_millis();
        let mut tagged_assistant = false;
        {
            let mut threads = self.threads.write().await;
            let thread = threads.get_mut(thread_id).ok_or_else(|| {
                anyhow::anyhow!("thread not found after participant send: {thread_id}")
            })?;

            thread.messages.retain(|message| {
                !(message.role == MessageRole::User && !existing_message_ids.contains(&message.id))
            });

            for message in thread.messages.iter_mut().rev() {
                if existing_message_ids.contains(&message.id) {
                    continue;
                }
                if message.role == MessageRole::Assistant {
                    message.author_agent_id = Some(agent_id.clone());
                    message.author_agent_name = Some(agent_name.clone());
                    tagged_assistant = true;
                    break;
                }
            }

            if !tagged_assistant {
                anyhow::bail!("participant send did not produce an assistant message");
            }

            thread.updated_at = now;
        }

        {
            let mut participants = self.thread_participants.write().await;
            if let Some(entry) = participants.get_mut(thread_id) {
                if let Some(participant) = entry
                    .iter_mut()
                    .find(|participant| participant.agent_id.eq_ignore_ascii_case(&agent_id))
                {
                    participant.last_contribution_at = Some(now);
                    participant.updated_at = now;
                    participant.status = ThreadParticipantStatus::Active;
                }
            }
        }

        self.persist_thread_by_id(thread_id).await;
        let _ = self.event_tx.send(AgentEvent::ThreadReloadRequired {
            thread_id: thread_id.to_string(),
        });

        Ok(())
    }
}
