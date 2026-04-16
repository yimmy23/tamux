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

pub const CHAT_HISTORY_PAGE_SIZE: usize = 100;
pub const CHAT_HISTORY_COLLAPSE_DELAY_TICKS: u64 = 20;
pub const CHAT_HISTORY_FETCH_DEBOUNCE_TICKS: u64 = 6;

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
    history_page_size: usize,
    active_thread_id: Option<String>,
    thread_activity: std::collections::HashMap<String, ThreadActivityState>,
    render_revision: u64,
    scroll_offset: usize,
    scroll_locked: bool,
    transcript_mode: TranscriptMode,
    expanded_reasoning: std::collections::HashSet<StoredMessageRef>,
    selected_message: Option<StoredMessageRef>,
    selected_message_action: usize,
    expanded_tools: std::collections::HashSet<StoredMessageRef>,
    pinned_message_top: Option<StoredMessageRef>,
    copied_message_feedback: Option<CopiedMessageFeedback>,
}

fn merge_message_pair(
    existing: Option<&AgentMessage>,
    incoming: Option<&AgentMessage>,
) -> AgentMessage {
    match (existing, incoming) {
        (Some(existing), Some(incoming)) => {
            let mut merged = incoming.clone();
            if !existing.actions.is_empty() && merged.actions.is_empty() {
                merged.actions = existing.actions.clone();
            }
            if existing.is_concierge_welcome {
                merged.is_concierge_welcome = true;
            }
            if merged.timestamp == 0 && existing.timestamp != 0 {
                merged.timestamp = existing.timestamp;
            }
            if merged.message_kind.is_empty() && !existing.message_kind.is_empty() {
                merged.message_kind = existing.message_kind.clone();
            }
            if merged.compaction_strategy.is_none() && existing.compaction_strategy.is_some() {
                merged.compaction_strategy = existing.compaction_strategy.clone();
            }
            if merged.compaction_payload.is_none() && existing.compaction_payload.is_some() {
                merged.compaction_payload = existing.compaction_payload.clone();
            }
            if merged.cost.is_none() && existing.cost.is_some() {
                merged.cost = existing.cost;
            }
            if merged.author_agent_id.is_none() && existing.author_agent_id.is_some() {
                merged.author_agent_id = existing.author_agent_id.clone();
            }
            if merged.author_agent_name.is_none() && existing.author_agent_name.is_some() {
                merged.author_agent_name = existing.author_agent_name.clone();
            }
            merged
        }
        (Some(existing), None) => existing.clone(),
        (None, Some(incoming)) => incoming.clone(),
        (None, None) => AgentMessage::default(),
    }
}

fn normalize_thread_window(thread: &mut AgentThread) {
    if thread.total_message_count == 0 {
        thread.total_message_count = thread.messages.len();
    }
    if thread.loaded_message_end == 0 && !thread.messages.is_empty() {
        thread.loaded_message_end = thread.total_message_count;
    }
    if thread.loaded_message_end < thread.loaded_message_start {
        thread.loaded_message_end = thread.loaded_message_start;
    }
    if thread.loaded_message_end > thread.total_message_count {
        thread.total_message_count = thread.loaded_message_end;
    }
    let loaded_count = thread.messages.len();
    if loaded_count == 0 {
        thread.loaded_message_start = thread.loaded_message_end.min(thread.total_message_count);
        return;
    }
    let max_start = thread.loaded_message_end.saturating_sub(loaded_count);
    thread.loaded_message_start = thread.loaded_message_start.min(max_start);
}

fn merge_thread_window(
    existing: &AgentThread,
    incoming: &AgentThread,
) -> (Vec<AgentMessage>, usize, usize, bool) {
    let existing_start = existing.loaded_message_start;
    let existing_end = existing
        .loaded_message_end
        .max(existing_start + existing.messages.len());
    let incoming_start = incoming.loaded_message_start;
    let incoming_end = incoming
        .loaded_message_end
        .max(incoming_start + incoming.messages.len());

    let union_start = existing_start.min(incoming_start);
    let union_end = existing_end.max(incoming_end);
    let mut merged = Vec::with_capacity(union_end.saturating_sub(union_start));

    for absolute_index in union_start..union_end {
        let existing_message = if absolute_index >= existing_start && absolute_index < existing_end
        {
            existing.messages.get(absolute_index - existing_start)
        } else {
            None
        };
        let incoming_message = if absolute_index >= incoming_start && absolute_index < incoming_end
        {
            incoming.messages.get(absolute_index - incoming_start)
        } else {
            None
        };

        if existing_message.is_some() || incoming_message.is_some() {
            merged.push(merge_message_pair(existing_message, incoming_message));
        }
    }

    (
        merged,
        union_start,
        union_end,
        incoming_end <= existing_start || existing_end <= incoming_start,
    )
}

fn should_replace_thread_window(existing: &AgentThread, incoming: &AgentThread) -> bool {
    incoming.total_message_count < existing.total_message_count
}

fn trim_thread_to_latest_page(thread: &mut AgentThread, page_size: usize) -> usize {
    normalize_thread_window(thread);
    if thread.messages.len() <= page_size {
        thread.history_window_expanded = false;
        thread.collapse_deadline_tick = None;
        return 0;
    }

    let drop_count = thread.messages.len().saturating_sub(page_size);
    thread.messages.drain(0..drop_count);
    thread.loaded_message_start = thread.loaded_message_start.saturating_add(drop_count);
    thread.loaded_message_end = thread.loaded_message_start + thread.messages.len();
    thread.history_window_expanded = false;
    thread.collapse_deadline_tick = None;
    drop_count
}

fn append_message_to_thread(thread: &mut AgentThread, message: AgentMessage, page_size: usize) {
    normalize_thread_window(thread);
    thread.messages.push(message);
    thread.total_message_count = thread.total_message_count.saturating_add(1);
    thread.loaded_message_end = thread.total_message_count;
    thread.loaded_message_start = thread
        .loaded_message_end
        .saturating_sub(thread.messages.len());
    if !thread.history_window_expanded && thread.messages.len() > page_size {
        trim_thread_to_latest_page(thread, page_size);
    } else {
        normalize_thread_window(thread);
    }
}

fn stored_message_ref(thread: &AgentThread, index: usize) -> Option<StoredMessageRef> {
    let message = thread.messages.get(index)?;
    Some(StoredMessageRef {
        thread_id: thread.id.clone(),
        message_id: message.id.as_ref().filter(|id| !id.is_empty()).cloned(),
        absolute_index: thread.loaded_message_start.saturating_add(index),
    })
}

fn resolve_message_ref(thread: &AgentThread, message_ref: &StoredMessageRef) -> Option<usize> {
    if message_ref.thread_id != thread.id {
        return None;
    }

    if let Some(message_id) = message_ref.message_id.as_deref() {
        if let Some(index) = thread
            .messages
            .iter()
            .position(|message| message.id.as_deref() == Some(message_id))
        {
            return Some(index);
        }
    }

    let loaded_end = thread.loaded_message_start + thread.messages.len();
    if message_ref.absolute_index >= thread.loaded_message_start
        && message_ref.absolute_index < loaded_end
    {
        Some(message_ref.absolute_index - thread.loaded_message_start)
    } else {
        None
    }
}

fn adjust_message_ref_for_deleted_absolute(
    mut message_ref: StoredMessageRef,
    thread_id: &str,
    deleted_absolute_index: usize,
) -> Option<StoredMessageRef> {
    if message_ref.thread_id != thread_id {
        return Some(message_ref);
    }

    match message_ref.absolute_index.cmp(&deleted_absolute_index) {
        std::cmp::Ordering::Less => Some(message_ref),
        std::cmp::Ordering::Equal => None,
        std::cmp::Ordering::Greater => {
            message_ref.absolute_index -= 1;
            Some(message_ref)
        }
    }
}

impl ChatState {
    pub fn new() -> Self {
        Self {
            threads: Vec::new(),
            history_page_size: CHAT_HISTORY_PAGE_SIZE,
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

    fn message_ref_for_active_index(&self, index: usize) -> Option<StoredMessageRef> {
        let thread = self.active_thread()?;
        stored_message_ref(thread, index)
    }

    fn resolve_active_message_ref(&self, message_ref: &StoredMessageRef) -> Option<usize> {
        let thread = self.active_thread()?;
        resolve_message_ref(thread, message_ref)
    }

    fn resolve_active_message_ref_set(
        &self,
        message_refs: &std::collections::HashSet<StoredMessageRef>,
    ) -> std::collections::HashSet<usize> {
        let Some(thread) = self.active_thread() else {
            return std::collections::HashSet::new();
        };
        message_refs
            .iter()
            .filter_map(|message_ref| resolve_message_ref(thread, message_ref))
            .collect()
    }

    pub fn threads(&self) -> &[AgentThread] {
        &self.threads
    }

    pub fn set_history_page_size(&mut self, page_size: usize) {
        self.history_page_size = page_size.max(1);
        for thread in &mut self.threads {
            if !thread.history_window_expanded && thread.messages.len() > self.history_page_size {
                trim_thread_to_latest_page(thread, self.history_page_size);
            }
        }
        self.bump_render_revision();
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

    pub fn active_thread_pinned_messages(&self) -> Vec<(usize, &AgentMessage)> {
        self.active_thread()
            .map(|thread| {
                thread
                    .messages
                    .iter()
                    .enumerate()
                    .filter(|(_, message)| message.pinned_for_compaction)
                    .collect()
            })
            .unwrap_or_default()
    }

    pub fn active_thread_has_pinned_messages(&self) -> bool {
        self.active_thread().is_some_and(|thread| {
            thread
                .messages
                .iter()
                .any(|message| message.pinned_for_compaction)
        })
    }

    pub fn active_thread_mut(&mut self) -> Option<&mut AgentThread> {
        let id = self.active_thread_id.as_deref()?.to_owned();
        self.threads.iter_mut().find(|t| t.id == id)
    }

    pub fn active_thread_has_older_history(&self) -> bool {
        self.active_thread()
            .is_some_and(|thread| thread.loaded_message_start > 0)
    }

    pub fn active_thread_older_page_pending(&self) -> bool {
        self.active_thread()
            .is_some_and(|thread| thread.older_page_pending)
    }

    pub fn active_thread_next_page_offset(&self, current_tick: u64) -> Option<usize> {
        let thread = self.active_thread()?;
        let cooldown_elapsed = thread
            .older_page_request_cooldown_until_tick
            .is_none_or(|deadline| current_tick >= deadline);
        (thread.loaded_message_start > 0 && !thread.older_page_pending && cooldown_elapsed)
            .then_some(
                thread
                    .loaded_message_end
                    .saturating_sub(thread.loaded_message_start),
            )
    }

    pub fn mark_active_thread_older_page_pending(
        &mut self,
        pending: bool,
        current_tick: u64,
        debounce_ticks: u64,
    ) {
        if let Some(thread) = self.active_thread_mut() {
            thread.older_page_pending = pending;
            if pending {
                thread.collapse_deadline_tick = None;
                thread.older_page_request_cooldown_until_tick =
                    Some(current_tick.saturating_add(debounce_ticks));
            }
            self.bump_render_revision();
        }
    }

    pub fn preserve_prepend_scroll_anchor(&mut self, resolved_scroll: usize) {
        self.scroll_offset = resolved_scroll;
        self.scroll_locked = true;
        self.bump_render_revision();
    }

    pub fn schedule_history_collapse(&mut self, current_tick: u64, delay_ticks: u64) {
        let history_page_size = self.history_page_size;
        if let Some(thread) = self.active_thread_mut() {
            if thread.history_window_expanded && thread.messages.len() > history_page_size {
                thread.collapse_deadline_tick = Some(current_tick.saturating_add(delay_ticks));
            }
        }
    }

    pub fn maybe_collapse_history(&mut self, current_tick: u64) {
        if self.scroll_offset != 0 {
            if let Some(thread) = self.active_thread_mut() {
                thread.collapse_deadline_tick = None;
            }
            return;
        }

        let mut dropped = 0usize;
        let history_page_size = self.history_page_size;
        if let Some(thread) = self.active_thread_mut() {
            let should_collapse = thread
                .collapse_deadline_tick
                .is_some_and(|deadline| current_tick >= deadline)
                && thread.history_window_expanded;
            if should_collapse {
                dropped = trim_thread_to_latest_page(thread, history_page_size);
            }
        }

        if dropped > 0 {
            self.bump_render_revision();
        }
    }

    pub fn delete_active_message(&mut self, index: usize) {
        let mut removed = false;
        let mut deleted_absolute_index = None;
        let mut deleted_thread_id = None;
        if let Some(thread) = self.active_thread_mut() {
            if index < thread.messages.len() {
                deleted_absolute_index = Some(thread.loaded_message_start + index);
                deleted_thread_id = Some(thread.id.clone());
                thread.messages.remove(index);
                thread.total_message_count = thread.total_message_count.saturating_sub(1);
                thread.loaded_message_end = thread.loaded_message_start + thread.messages.len();
                normalize_thread_window(thread);
                removed = true;
            }
        }

        if removed {
            let deleted_absolute_index =
                deleted_absolute_index.expect("removed message should have absolute index");
            let deleted_thread_id =
                deleted_thread_id.expect("removed message should have thread id");
            self.selected_message = self.selected_message.take().and_then(|message_ref| {
                adjust_message_ref_for_deleted_absolute(
                    message_ref,
                    &deleted_thread_id,
                    deleted_absolute_index,
                )
            });
            self.expanded_reasoning = self
                .expanded_reasoning
                .iter()
                .cloned()
                .filter_map(|message_ref| {
                    adjust_message_ref_for_deleted_absolute(
                        message_ref,
                        &deleted_thread_id,
                        deleted_absolute_index,
                    )
                })
                .collect();
            self.expanded_tools = self
                .expanded_tools
                .iter()
                .cloned()
                .filter_map(|message_ref| {
                    adjust_message_ref_for_deleted_absolute(
                        message_ref,
                        &deleted_thread_id,
                        deleted_absolute_index,
                    )
                })
                .collect();
            self.pinned_message_top = self.pinned_message_top.take().and_then(|message_ref| {
                adjust_message_ref_for_deleted_absolute(
                    message_ref,
                    &deleted_thread_id,
                    deleted_absolute_index,
                )
            });
            self.copied_message_feedback =
                self.copied_message_feedback.take().and_then(|feedback| {
                    adjust_message_ref_for_deleted_absolute(
                        feedback.message_ref,
                        &deleted_thread_id,
                        deleted_absolute_index,
                    )
                    .map(|message_ref| CopiedMessageFeedback {
                        message_ref,
                        expires_at_tick: feedback.expires_at_tick,
                    })
                });
            self.bump_render_revision();
        }
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

    pub fn resolve_operator_question_answer(&mut self, question_id: &str, answer: String) -> bool {
        let mut updated = false;
        for thread in &mut self.threads {
            if let Some(message) = thread
                .messages
                .iter_mut()
                .find(|message| message.operator_question_id.as_deref() == Some(question_id))
            {
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
            .as_ref()
            .and_then(|message_ref| self.resolve_active_message_ref(message_ref))
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
                        append_message_to_thread(
                            thread,
                            AgentMessage {
                                role: MessageRole::Assistant,
                                content,
                                reasoning,
                                ..Default::default()
                            },
                            self.history_page_size,
                        );
                    }
                }

                // Push tool call as a TOOL message immediately (running status)
                if let Some(thread) = self.threads.iter_mut().find(|t| t.id == thread_id) {
                    append_message_to_thread(
                        thread,
                        AgentMessage {
                            role: MessageRole::Tool,
                            tool_name: Some(name.clone()),
                            tool_call_id: Some(call_id.clone()),
                            tool_arguments: Some(args),
                            tool_status: Some("running".to_string()),
                            weles_review: weles_review.clone(),
                            ..Default::default()
                        },
                        self.history_page_size,
                    );
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
                        append_message_to_thread(thread, msg, self.history_page_size);
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
                        if let Some(existing) = existing_threads
                            .iter()
                            .find(|thread| thread.id == incoming.id)
                        {
                            if incoming.messages.is_empty() {
                                incoming.messages = existing.messages.clone();
                                incoming.total_message_count = existing.total_message_count;
                                incoming.loaded_message_start = existing.loaded_message_start;
                                incoming.loaded_message_end = existing.loaded_message_end;
                                incoming.older_page_pending = existing.older_page_pending;
                                incoming.older_page_request_cooldown_until_tick =
                                    existing.older_page_request_cooldown_until_tick;
                                incoming.history_window_expanded = existing.history_window_expanded;
                                incoming.collapse_deadline_tick = existing.collapse_deadline_tick;
                            }
                            if incoming.thread_participants.is_empty() {
                                incoming.thread_participants = existing.thread_participants.clone();
                            }
                            if incoming.queued_participant_suggestions.is_empty() {
                                incoming.queued_participant_suggestions =
                                    existing.queued_participant_suggestions.clone();
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
                let mut incoming = incoming;
                normalize_thread_window(&mut incoming);
                if let Some(existing) = self.threads.iter_mut().find(|t| t.id == incoming.id) {
                    normalize_thread_window(existing);
                    let replace_existing_window = should_replace_thread_window(existing, &incoming);
                    let (merged, merged_start, merged_end, disjoint) =
                        if replace_existing_window || existing.messages.is_empty() {
                            (
                                incoming.messages.clone(),
                                incoming.loaded_message_start,
                                incoming.loaded_message_end,
                                false,
                            )
                        } else {
                            merge_thread_window(existing, &incoming)
                        };
                    existing.messages = merged;
                    existing.total_message_count = if replace_existing_window {
                        incoming.total_message_count
                    } else {
                        incoming
                            .total_message_count
                            .max(existing.total_message_count)
                    };
                    existing.loaded_message_start = merged_start;
                    existing.loaded_message_end = merged_end.max(existing.total_message_count);
                    existing.older_page_pending = false;
                    existing.older_page_request_cooldown_until_tick = existing
                        .older_page_request_cooldown_until_tick
                        .max(incoming.older_page_request_cooldown_until_tick);
                    existing.history_window_expanded =
                        existing.messages.len() > self.history_page_size;
                    if disjoint && incoming.loaded_message_end <= existing.loaded_message_end {
                        existing.collapse_deadline_tick = None;
                    }
                    existing.total_input_tokens =
                        incoming.total_input_tokens.max(existing.total_input_tokens);
                    existing.total_output_tokens = incoming
                        .total_output_tokens
                        .max(existing.total_output_tokens);
                    existing.thread_participants = incoming.thread_participants;
                    existing.queued_participant_suggestions =
                        incoming.queued_participant_suggestions;
                    if incoming.agent_name.is_some() {
                        existing.agent_name = incoming.agent_name;
                    }
                    if !incoming.title.is_empty() {
                        existing.title = incoming.title;
                    }
                    normalize_thread_window(existing);
                } else {
                    incoming.history_window_expanded =
                        incoming.messages.len() > self.history_page_size;
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
                    existing.total_message_count = existing.messages.len();
                    existing.loaded_message_start = 0;
                    existing.loaded_message_end = existing.messages.len();
                    existing.history_window_expanded = false;
                    existing.older_page_request_cooldown_until_tick = None;
                    existing.collapse_deadline_tick = None;
                } else {
                    let local_message_count = local_messages.len();
                    let thread = AgentThread {
                        id: thread_id.clone(),
                        agent_name: None,
                        title,
                        messages: local_messages,
                        total_message_count: local_message_count,
                        loaded_message_start: 0,
                        loaded_message_end: local_message_count,
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
                    thread.total_message_count = 0;
                    thread.loaded_message_start = 0;
                    thread.loaded_message_end = 0;
                    thread.older_page_pending = false;
                    thread.older_page_request_cooldown_until_tick = None;
                    thread.history_window_expanded = false;
                    thread.collapse_deadline_tick = None;
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
                    append_message_to_thread(thread, message, self.history_page_size);
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
                        total_message_count: 1,
                        loaded_message_start: 0,
                        loaded_message_end: 1,
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
                    if let Some(thread) = self.active_thread_mut() {
                        thread.collapse_deadline_tick = None;
                    }
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
                self.pinned_message_top = self.message_ref_for_active_index(index);
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
                        append_message_to_thread(
                            thread,
                            AgentMessage {
                                role: MessageRole::Assistant,
                                content: stopped_content,
                                reasoning: if reasoning.is_empty() {
                                    None
                                } else {
                                    Some(reasoning)
                                },
                                ..Default::default()
                            },
                            self.history_page_size,
                        );
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
