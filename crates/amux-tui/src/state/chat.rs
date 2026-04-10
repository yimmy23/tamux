// Temporary local copies until wire.rs rename (Task 9)
// These mirror the types in state.rs
#![allow(dead_code)]

use serde_json::Value;

#[path = "chat_types.rs"]
mod chat_types;
#[path = "chat_interactions.rs"]
mod interactions;

pub use chat_types::*;

use amux_protocol::AGENT_NAME_RAROG;

#[derive(Default)]
struct ThreadActivityState {
    streaming_content: String,
    streaming_reasoning: String,
    active_tool_calls: Vec<ToolCallVm>,
    retry_status: Option<RetryStatusVm>,
}

// ── ChatState ─────────────────────────────────────────────────────────────────

pub struct ChatState {
    threads: Vec<AgentThread>,
    active_thread_id: Option<String>,
    thread_activity: std::collections::HashMap<String, ThreadActivityState>,
    render_revision: u64,
    scroll_offset: usize,
    scroll_locked: bool,
    transcript_mode: TranscriptMode,
    expanded_reasoning: std::collections::HashSet<usize>,
    selected_message: Option<usize>,
    selected_message_action: usize,
    expanded_tools: std::collections::HashSet<usize>,
    pinned_message_top: Option<usize>,
    copied_message_feedback: Option<CopiedMessageFeedback>,
}

impl ChatState {
    pub fn new() -> Self {
        Self {
            threads: Vec::new(),
            active_thread_id: None,
            thread_activity: std::collections::HashMap::new(),
            render_revision: 0,
            scroll_offset: 0,
            expanded_reasoning: std::collections::HashSet::new(),
            scroll_locked: false,
            transcript_mode: TranscriptMode::Compact,
            selected_message: None,
            selected_message_action: 0,
            expanded_tools: std::collections::HashSet::new(),
            pinned_message_top: None,
            copied_message_feedback: None,
        }
    }

    pub fn threads(&self) -> &[AgentThread] {
        &self.threads
    }

    pub fn active_thread_id(&self) -> Option<&str> {
        self.active_thread_id.as_deref()
    }

    pub fn active_thread(&self) -> Option<&AgentThread> {
        let id = self.active_thread_id.as_deref()?;
        self.threads.iter().find(|t| t.id == id)
    }

    pub fn active_thread_runtime_metadata(&self) -> Option<ThreadRuntimeMetadata> {
        let thread = self.active_thread()?;
        if thread.runtime_provider.is_none()
            && thread.runtime_model.is_none()
            && thread.runtime_reasoning_effort.is_none()
        {
            return None;
        }

        Some(ThreadRuntimeMetadata {
            provider: thread.runtime_provider.clone(),
            model: thread.runtime_model.clone(),
            reasoning_effort: thread.runtime_reasoning_effort.clone(),
        })
    }

    pub fn active_thread_mut(&mut self) -> Option<&mut AgentThread> {
        let id = self.active_thread_id.as_deref()?.to_owned();
        self.threads.iter_mut().find(|t| t.id == id)
    }

    /// Actions from the last assistant message in the active thread that has any.
    pub fn active_actions(&self) -> &[MessageAction] {
        self.active_thread()
            .and_then(|thread| {
                thread
                    .messages
                    .iter()
                    .rev()
                    .find(|m| m.role == MessageRole::Assistant && !m.actions.is_empty())
            })
            .map(|m| m.actions.as_slice())
            .unwrap_or(&[])
    }

    pub fn resolve_operator_question_answer(
        &mut self,
        question_id: &str,
        answer: String,
    ) -> bool {
        let mut updated = false;
        for thread in &mut self.threads {
            if let Some(message) = thread.messages.iter_mut().find(|message| {
                message.operator_question_id.as_deref() == Some(question_id)
            }) {
                message.operator_question_answer = Some(answer);
                message.actions.clear();
                updated = true;
                break;
            }
        }

        if updated {
            self.bump_render_revision();
        }

        updated
    }

    fn active_activity(&self) -> Option<&ThreadActivityState> {
        let thread_id = self.active_thread_id.as_deref()?;
        self.thread_activity.get(thread_id)
    }

    fn activity_for_thread_mut(&mut self, thread_id: &str) -> &mut ThreadActivityState {
        self.thread_activity
            .entry(thread_id.to_string())
            .or_default()
    }

    fn cleanup_thread_activity(&mut self, thread_id: &str) {
        let should_remove = self
            .thread_activity
            .get(thread_id)
            .map(|activity| {
                activity.streaming_content.is_empty()
                    && activity.streaming_reasoning.is_empty()
                    && activity.active_tool_calls.is_empty()
                    && activity.retry_status.is_none()
            })
            .unwrap_or(false);
        if should_remove {
            self.thread_activity.remove(thread_id);
        }
    }

    pub fn streaming_content(&self) -> &str {
        self.active_activity()
            .map(|activity| activity.streaming_content.as_str())
            .unwrap_or("")
    }

    pub fn streaming_reasoning(&self) -> &str {
        self.active_activity()
            .map(|activity| activity.streaming_reasoning.as_str())
            .unwrap_or("")
    }

    pub fn active_tool_calls(&self) -> &[ToolCallVm] {
        self.active_activity()
            .map(|activity| activity.active_tool_calls.as_slice())
            .unwrap_or(&[])
    }

    pub fn render_revision(&self) -> u64 {
        self.render_revision
    }

    pub fn render_cache_epoch(&self, current_tick: u64) -> u64 {
        if self.copied_message_feedback.is_some() {
            return current_tick;
        }

        if self
            .retry_status()
            .is_some_and(|status| status.phase == RetryPhase::Waiting)
        {
            let ticks_per_second = (1_000 / crate::app::TUI_TICK_RATE_MS).max(1);
            return current_tick / ticks_per_second;
        }

        0
    }

    pub fn has_running_tool_calls(&self) -> bool {
        self.active_tool_calls()
            .iter()
            .any(|tc| tc.status == ToolCallStatus::Running)
    }

    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    pub fn scroll_locked(&self) -> bool {
        self.scroll_locked
    }

    pub fn transcript_mode(&self) -> TranscriptMode {
        self.transcript_mode
    }

    pub fn retry_status(&self) -> Option<&RetryStatusVm> {
        self.active_activity()
            .and_then(|activity| activity.retry_status.as_ref())
    }

    pub fn pinned_message_top(&self) -> Option<usize> {
        self.pinned_message_top
    }

    pub fn is_streaming(&self) -> bool {
        !self.streaming_content().is_empty()
            || !self.streaming_reasoning().is_empty()
            || self.has_running_tool_calls()
    }

    fn bump_render_revision(&mut self) {
        self.render_revision = self.render_revision.wrapping_add(1);
    }

    fn move_thread_to_front(&mut self, thread_id: &str) {
        let Some(index) = self
            .threads
            .iter()
            .position(|thread| thread.id == thread_id)
        else {
            return;
        };
        if index == 0 {
            return;
        }
        let thread = self.threads.remove(index);
        self.threads.insert(0, thread);
    }

    pub fn reduce(&mut self, action: ChatAction) {
        let mut should_bump_render_revision = true;
        match action {
            ChatAction::Delta { thread_id, content } => {
                self.pinned_message_top = None;
                // Set active thread if not set, or if it matches the incoming thread
                if self.active_thread_id.is_none()
                    || self.active_thread_id.as_deref() == Some(thread_id.as_str())
                {
                    self.active_thread_id = Some(thread_id.clone());
                }
                if self.scroll_locked
                    && self.active_thread_id.as_deref() == Some(thread_id.as_str())
                {
                    self.scroll_offset = self
                        .scroll_offset
                        .saturating_add(content.matches('\n').count());
                }
                self.activity_for_thread_mut(&thread_id)
                    .streaming_content
                    .push_str(&content);
            }

            ChatAction::Reasoning { thread_id, content } => {
                self.pinned_message_top = None;
                self.activity_for_thread_mut(&thread_id)
                    .streaming_reasoning
                    .push_str(&content);
            }

            ChatAction::ToolCall {
                thread_id,
                call_id,
                name,
                args,
                weles_review,
            } => {
                self.pinned_message_top = None;
                let (content, reasoning) = {
                    let activity = self.activity_for_thread_mut(&thread_id);
                    let content = std::mem::take(&mut activity.streaming_content);
                    let reasoning = if activity.streaming_reasoning.is_empty() {
                        None
                    } else {
                        Some(std::mem::take(&mut activity.streaming_reasoning))
                    };
                    (content, reasoning)
                };
                // Flush any accumulated streaming content as an ASST message first
                // (the assistant said something before calling the tool)
                if !content.is_empty() || reasoning.is_some() {
                    if let Some(thread) = self.threads.iter_mut().find(|t| t.id == thread_id) {
                        thread.messages.push(AgentMessage {
                            role: MessageRole::Assistant,
                            content,
                            reasoning,
                            ..Default::default()
                        });
                    }
                }

                // Push tool call as a TOOL message immediately (running status)
                if let Some(thread) = self.threads.iter_mut().find(|t| t.id == thread_id) {
                    thread.messages.push(AgentMessage {
                        role: MessageRole::Tool,
                        tool_name: Some(name.clone()),
                        tool_call_id: Some(call_id.clone()),
                        tool_arguments: Some(args),
                        tool_status: Some("running".to_string()),
                        weles_review: weles_review.clone(),
                        ..Default::default()
                    });
                }

                // Still track in active_tool_calls for status updates
                self.activity_for_thread_mut(&thread_id)
                    .active_tool_calls
                    .push(ToolCallVm {
                        call_id,
                        name,
                        arguments: String::new(),
                        status: ToolCallStatus::Running,
                        result: None,
                        is_error: false,
                        weles_review,
                        started_at: 0,
                    });
            }

            ChatAction::ToolResult {
                thread_id,
                call_id,
                name: _,
                content,
                is_error,
                weles_review,
            } => {
                self.pinned_message_top = None;
                // Update the active tracker
                if let Some(activity) = self.thread_activity.get_mut(&thread_id) {
                    if let Some(tc) = activity
                        .active_tool_calls
                        .iter_mut()
                        .find(|tc| tc.call_id == call_id)
                    {
                        tc.status = if is_error {
                            ToolCallStatus::Error
                        } else {
                            ToolCallStatus::Done
                        };
                        tc.result = Some(content.clone());
                        tc.is_error = is_error;
                        tc.weles_review = weles_review.clone();
                    }
                }

                // Update the TOOL message in the thread
                if let Some(thread) = self.threads.iter_mut().find(|t| t.id == thread_id) {
                    if let Some(msg) = thread.messages.iter_mut().rev().find(|m| {
                        m.role == MessageRole::Tool && m.tool_call_id.as_deref() == Some(&call_id)
                    }) {
                        msg.tool_status = Some(if is_error {
                            "error".to_string()
                        } else {
                            "done".to_string()
                        });
                        msg.weles_review = weles_review;
                        msg.content = content;
                    }
                }
                self.cleanup_thread_activity(&thread_id);
            }

            ChatAction::TurnDone {
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
                self.pinned_message_top = None;
                let (content, mut final_reasoning) = {
                    let activity = self.activity_for_thread_mut(&thread_id);
                    activity.active_tool_calls.clear();
                    activity.retry_status = None;
                    let content = std::mem::take(&mut activity.streaming_content);
                    let reasoning = std::mem::take(&mut activity.streaming_reasoning);
                    (content, reasoning)
                };
                if final_reasoning.trim().is_empty() {
                    final_reasoning = reasoning.unwrap_or_default();
                }

                if let Some(thread) = self.threads.iter_mut().find(|t| t.id == thread_id) {
                    if provider.is_some() {
                        thread.runtime_provider = provider.clone();
                    }
                    if model.is_some() {
                        thread.runtime_model = model.clone();
                    }
                    if let Some(reasoning_effort) =
                        extract_reasoning_effort(provider_final_result_json.as_deref())
                    {
                        thread.runtime_reasoning_effort = Some(reasoning_effort);
                    }
                }

                if !content.is_empty() || !final_reasoning.is_empty() {
                    let msg = AgentMessage {
                        role: MessageRole::Assistant,
                        content,
                        reasoning: if final_reasoning.is_empty() {
                            None
                        } else {
                            Some(final_reasoning)
                        },
                        provider_final_result_json,
                        input_tokens,
                        output_tokens,
                        tps,
                        generation_ms,
                        cost,
                        ..Default::default()
                    };

                    if let Some(thread) = self.threads.iter_mut().find(|t| t.id == thread_id) {
                        thread.messages.push(msg);
                        thread.total_input_tokens += input_tokens;
                        thread.total_output_tokens += output_tokens;
                    }
                }
                self.cleanup_thread_activity(&thread_id);
            }

            ChatAction::SetRetryStatus {
                thread_id,
                phase,
                attempt,
                max_retries,
                delay_ms,
                failure_class,
                message,
                received_at_tick,
            } => {
                if self.active_thread_id.is_none() {
                    self.active_thread_id = Some(thread_id.clone());
                }
                self.activity_for_thread_mut(&thread_id).retry_status = Some(RetryStatusVm {
                    phase,
                    attempt,
                    max_retries,
                    delay_ms,
                    failure_class,
                    message,
                    received_at_tick,
                });
            }

            ChatAction::ClearRetryStatus { thread_id } => {
                if let Some(activity) = self.thread_activity.get_mut(&thread_id) {
                    activity.retry_status = None;
                }
                self.cleanup_thread_activity(&thread_id);
            }

            ChatAction::ThreadListReceived(new_threads) => {
                // Preserve active selection if the thread still exists
                if let Some(active_id) = &self.active_thread_id {
                    if !new_threads.iter().any(|t| &t.id == active_id) {
                        self.active_thread_id = None;
                    }
                }

                let existing_threads = std::mem::take(&mut self.threads);
                self.threads = new_threads
                    .into_iter()
                    .map(|mut incoming| {
                        if incoming.messages.is_empty() {
                            if let Some(existing) = existing_threads
                                .iter()
                                .find(|thread| thread.id == incoming.id)
                            {
                                incoming.messages = existing.messages.clone();
                            }
                        }
                        incoming
                    })
                    .collect();
            }

            ChatAction::ThreadDetailReceived(incoming) => {
                // Skip merging the concierge thread — the ConciergeWelcome
                // event is the authoritative source for its content.
                if incoming.id == "concierge" {
                    return;
                }
                if let Some(existing) = self.threads.iter_mut().find(|t| t.id == incoming.id) {
                    // Merge: keep local user messages and local messages that carry
                    // interactive UI actions (e.g. concierge action buttons), then
                    // add incoming daemon messages.
                    let local_kept_msgs: Vec<AgentMessage> = existing
                        .messages
                        .iter()
                        .filter(|m| {
                            m.role == MessageRole::User
                                || !m.actions.is_empty()
                                || m.is_concierge_welcome
                        })
                        .cloned()
                        .collect();
                    let mut merged = local_kept_msgs;
                    // Add incoming messages that aren't already present
                    for msg in incoming.messages {
                        if !merged
                            .iter()
                            .any(|m| m.content == msg.content && m.role == msg.role)
                        {
                            merged.push(msg);
                        }
                    }
                    // Sort by timestamp (0 timestamps go last)
                    merged.sort_by_key(|m| {
                        if m.timestamp == 0 {
                            u64::MAX
                        } else {
                            m.timestamp
                        }
                    });
                    existing.messages = merged;
                    existing.total_input_tokens =
                        incoming.total_input_tokens.max(existing.total_input_tokens);
                    existing.total_output_tokens = incoming
                        .total_output_tokens
                        .max(existing.total_output_tokens);
                    if incoming.agent_name.is_some() {
                        existing.agent_name = incoming.agent_name;
                    }
                    if !incoming.title.is_empty() {
                        existing.title = incoming.title;
                    }
                } else {
                    self.threads.push(incoming);
                }
            }

            ChatAction::ThreadCreated { thread_id, title } => {
                self.pinned_message_top = None;
                // Transfer messages from any local pending thread to the real thread
                let local_messages = self
                    .active_thread()
                    .map(|t| t.messages.clone())
                    .unwrap_or_default();

                // Remove local thread if it exists (it was a placeholder)
                if let Some(active_id) = &self.active_thread_id {
                    if active_id.starts_with("local-") {
                        self.threads.retain(|t| t.id != *active_id);
                    }
                }

                // Check if thread already exists (avoid duplicates)
                if let Some(existing) = self.threads.iter_mut().find(|t| t.id == thread_id) {
                    // Merge local messages into existing
                    for msg in &local_messages {
                        if !existing
                            .messages
                            .iter()
                            .any(|m| m.content == msg.content && m.role == msg.role)
                        {
                            existing.messages.insert(0, msg.clone());
                        }
                    }
                } else {
                    let thread = AgentThread {
                        id: thread_id.clone(),
                        agent_name: None,
                        title,
                        messages: local_messages,
                        ..Default::default()
                    };
                    self.threads.push(thread);
                }
                self.move_thread_to_front(&thread_id);
                self.active_thread_id = Some(thread_id);
            }

            ChatAction::ClearThread { thread_id } => {
                if let Some(thread) = self.threads.iter_mut().find(|t| t.id == thread_id) {
                    thread.messages.clear();
                }
                self.thread_activity.remove(&thread_id);
            }

            ChatAction::DismissConciergeWelcome => {
                if let Some(thread) = self.threads.iter_mut().find(|t| t.id == "concierge") {
                    thread
                        .messages
                        .retain(|message| !message.is_concierge_welcome);
                }
            }

            ChatAction::AppendMessage { thread_id, message } => {
                if let Some(thread) = self.threads.iter_mut().find(|t| t.id == thread_id) {
                    if thread_id == "concierge" && message.is_concierge_welcome {
                        thread.messages.retain(|msg| !msg.is_concierge_welcome);
                    }
                    thread.messages.push(message);
                } else {
                    let title = if thread_id == "concierge" {
                        AGENT_NAME_RAROG.to_string()
                    } else {
                        thread_id.clone()
                    };
                    self.threads.push(AgentThread {
                        id: thread_id.clone(),
                        agent_name: None,
                        title,
                        messages: vec![message],
                        ..Default::default()
                    });
                }
            }

            ChatAction::SelectThread(thread_id) => {
                self.pinned_message_top = None;
                self.active_thread_id = if thread_id.is_empty() {
                    None
                } else {
                    Some(thread_id)
                };
                self.scroll_offset = 0;
                self.scroll_locked = false;
            }

            ChatAction::ScrollChat(delta) => {
                should_bump_render_revision = false;
                self.pinned_message_top = None;
                if delta > 0 {
                    self.scroll_offset = self.scroll_offset.saturating_add(delta as usize);
                    self.scroll_locked = true;
                } else if delta < 0 {
                    let decrease = (-delta) as usize;
                    self.scroll_offset = self.scroll_offset.saturating_sub(decrease);
                    if self.scroll_offset == 0 {
                        self.scroll_locked = false;
                    }
                }
            }

            ChatAction::PinMessageTop(index) => {
                should_bump_render_revision = false;
                self.pinned_message_top = Some(index);
                self.scroll_locked = false;
            }

            ChatAction::NewThread => {
                self.pinned_message_top = None;
                self.active_thread_id = None;
                self.copied_message_feedback = None;
            }

            ChatAction::SetTranscriptMode(mode) => {
                self.transcript_mode = mode;
            }

            ChatAction::ResetStreaming => {
                if let Some(thread_id) = self.active_thread_id.clone() {
                    self.thread_activity.remove(&thread_id);
                }
            }

            ChatAction::ForceStopStreaming => {
                // Finalize current streaming as incomplete message with [stopped] marker
                let Some(thread_id) = self.active_thread_id.clone() else {
                    self.bump_render_revision();
                    return;
                };
                let (content, reasoning) =
                    if let Some(activity) = self.thread_activity.get_mut(&thread_id) {
                        (
                            std::mem::take(&mut activity.streaming_content),
                            std::mem::take(&mut activity.streaming_reasoning),
                        )
                    } else {
                        (String::new(), String::new())
                    };
                if !content.is_empty() || !reasoning.is_empty() {
                    let stopped_content = if content.is_empty() {
                        "[stopped]".to_string()
                    } else {
                        format!("{} [stopped]", content)
                    };
                    if let Some(thread) = self.threads.iter_mut().find(|t| t.id == thread_id) {
                        thread.messages.push(AgentMessage {
                            role: MessageRole::Assistant,
                            content: stopped_content,
                            reasoning: if reasoning.is_empty() {
                                None
                            } else {
                                Some(reasoning)
                            },
                            ..Default::default()
                        });
                    }
                }
                self.thread_activity.remove(&thread_id);
            }
        }

        if should_bump_render_revision {
            self.bump_render_revision();
        }
    }
}

fn extract_reasoning_effort(provider_final_result_json: Option<&str>) -> Option<String> {
    let json = provider_final_result_json?.trim();
    if json.is_empty() {
        return None;
    }

    let value: Value = serde_json::from_str(json).ok()?;
    value
        .get("reasoning_effort")
        .and_then(Value::as_str)
        .or_else(|| {
            value
                .get("reasoning")
                .and_then(|reasoning| reasoning.get("effort"))
                .and_then(Value::as_str)
        })
        .or_else(|| {
            value
                .get("response")
                .and_then(|response| response.get("reasoning"))
                .and_then(|reasoning| reasoning.get("effort"))
                .and_then(Value::as_str)
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

impl Default for ChatState {
    fn default() -> Self {
        Self::new()
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "tests/chat.rs"]
mod tests;
