use super::*;

#[cfg(not(test))]
thread_local! {
    static SYSTEM_CLIPBOARD: std::cell::RefCell<Option<arboard::Clipboard>> =
        const { std::cell::RefCell::new(None) };
}

pub(super) fn convert_thread(t: crate::wire::AgentThread) -> chat::AgentThread {
    let derived_total_message_count = t.total_message_count.max(t.messages.len());
    let derived_loaded_message_end = if t.loaded_message_end == 0 && !t.messages.is_empty() {
        derived_total_message_count
    } else {
        t.loaded_message_end.max(t.messages.len())
    };
    let derived_loaded_message_start = if derived_loaded_message_end >= t.messages.len() {
        t.loaded_message_start
            .min(derived_loaded_message_end.saturating_sub(t.messages.len()))
    } else {
        0
    };
    chat::AgentThread {
        id: t.id,
        agent_name: t.agent_name,
        title: t.title,
        created_at: t.created_at,
        updated_at: t.updated_at,
        messages: t.messages.into_iter().map(convert_message).collect(),
        total_message_count: derived_total_message_count,
        loaded_message_start: derived_loaded_message_start,
        loaded_message_end: derived_loaded_message_end,
        active_compaction_window_start: None,
        pinned_messages: t
            .pinned_messages
            .into_iter()
            .map(|message| chat::PinnedThreadMessage {
                message_id: message.message_id,
                absolute_index: message.absolute_index,
                role: match message.role {
                    crate::wire::MessageRole::System => chat::MessageRole::System,
                    crate::wire::MessageRole::User => chat::MessageRole::User,
                    crate::wire::MessageRole::Assistant => chat::MessageRole::Assistant,
                    crate::wire::MessageRole::Tool => chat::MessageRole::Tool,
                    crate::wire::MessageRole::Unknown => chat::MessageRole::Unknown,
                },
                content: message.content,
            })
            .collect(),
        older_page_pending: false,
        older_page_request_cooldown_until_tick: None,
        history_window_expanded: false,
        collapse_deadline_tick: None,
        total_input_tokens: t.total_input_tokens,
        total_output_tokens: t.total_output_tokens,
        thread_participants: t
            .thread_participants
            .into_iter()
            .map(|participant| chat::ThreadParticipantState {
                agent_id: participant.agent_id,
                agent_name: participant.agent_name,
                instruction: participant.instruction,
                status: participant.status,
                created_at: participant.created_at,
                updated_at: participant.updated_at,
                deactivated_at: participant.deactivated_at,
                last_contribution_at: participant.last_contribution_at,
                always_auto_response: participant.always_auto_response,
            })
            .collect(),
        queued_participant_suggestions: t
            .queued_participant_suggestions
            .into_iter()
            .map(|suggestion| chat::ThreadParticipantSuggestionVm {
                id: suggestion.id,
                target_agent_id: suggestion.target_agent_id,
                target_agent_name: suggestion.target_agent_name,
                instruction: suggestion.instruction,
                suggestion_kind: suggestion.suggestion_kind,
                force_send: suggestion.force_send,
                status: suggestion.status,
                created_at: suggestion.created_at,
                updated_at: suggestion.updated_at,
                auto_send_at: suggestion.auto_send_at,
                source_message_timestamp: suggestion.source_message_timestamp,
                error: suggestion.error,
            })
            .collect(),
        runtime_provider: None,
        runtime_model: None,
        runtime_reasoning_effort: None,
    }
}

pub(super) fn convert_message(m: crate::wire::AgentMessage) -> chat::AgentMessage {
    chat::AgentMessage {
        id: m.id,
        role: match m.role {
            crate::wire::MessageRole::System => chat::MessageRole::System,
            crate::wire::MessageRole::User => chat::MessageRole::User,
            crate::wire::MessageRole::Assistant => chat::MessageRole::Assistant,
            crate::wire::MessageRole::Tool => chat::MessageRole::Tool,
            crate::wire::MessageRole::Unknown => chat::MessageRole::Unknown,
        },
        content: m.content,
        reasoning: m.reasoning,
        author_agent_id: m.author_agent_id,
        author_agent_name: m.author_agent_name,
        is_operator_question: m.is_operator_question,
        operator_question_id: m.operator_question_id,
        operator_question_answer: m.operator_question_answer,
        provider_final_result_json: m.provider_final_result_json,
        tool_name: m.tool_name,
        tool_arguments: m.tool_arguments,
        tool_call_id: m.tool_call_id,
        tool_status: m.tool_status,
        weles_review: m.weles_review.map(|review| chat::WelesReviewMetaVm {
            weles_reviewed: review.weles_reviewed,
            verdict: review.verdict,
            reasons: review.reasons,
            audit_id: review.audit_id,
            security_override_mode: review.security_override_mode,
        }),
        input_tokens: m.input_tokens,
        output_tokens: m.output_tokens,
        tps: m.tps,
        generation_ms: m.generation_ms,
        cost: m.cost,
        is_streaming: m.is_streaming,
        pinned_for_compaction: m.pinned_for_compaction,
        message_kind: m.message_kind,
        compaction_strategy: m.compaction_strategy,
        compaction_payload: m.compaction_payload,
        timestamp: m.timestamp,
        actions: Vec::new(),
        is_concierge_welcome: false,
    }
}

pub(super) fn convert_task(t: crate::wire::AgentTask) -> task::AgentTask {
    task::AgentTask {
        id: t.id,
        title: t.title,
        description: t.description,
        thread_id: t.thread_id,
        parent_task_id: t.parent_task_id,
        parent_thread_id: t.parent_thread_id,
        created_at: t.created_at,
        status: t.status.map(|s| match s {
            crate::wire::TaskStatus::Queued => task::TaskStatus::Queued,
            crate::wire::TaskStatus::InProgress => task::TaskStatus::InProgress,
            crate::wire::TaskStatus::AwaitingApproval => task::TaskStatus::AwaitingApproval,
            crate::wire::TaskStatus::Blocked => task::TaskStatus::Blocked,
            crate::wire::TaskStatus::FailedAnalyzing => task::TaskStatus::FailedAnalyzing,
            crate::wire::TaskStatus::Completed => task::TaskStatus::Completed,
            crate::wire::TaskStatus::Failed => task::TaskStatus::Failed,
            crate::wire::TaskStatus::Cancelled => task::TaskStatus::Cancelled,
        }),
        progress: t.progress,
        session_id: t.session_id,
        goal_run_id: t.goal_run_id,
        goal_step_title: t.goal_step_title,
        command: t.command,
        awaiting_approval_id: t.awaiting_approval_id,
        blocked_reason: t.blocked_reason,
    }
}

pub(super) fn convert_goal_run(r: crate::wire::GoalRun) -> task::GoalRun {
    task::GoalRun {
        id: r.id,
        title: r.title,
        thread_id: r.thread_id,
        session_id: r.session_id,
        status: r.status.map(|s| match s {
            crate::wire::GoalRunStatus::Queued => task::GoalRunStatus::Queued,
            crate::wire::GoalRunStatus::Planning => task::GoalRunStatus::Planning,
            crate::wire::GoalRunStatus::Running => task::GoalRunStatus::Running,
            crate::wire::GoalRunStatus::AwaitingApproval => task::GoalRunStatus::AwaitingApproval,
            crate::wire::GoalRunStatus::Paused => task::GoalRunStatus::Paused,
            crate::wire::GoalRunStatus::Completed => task::GoalRunStatus::Completed,
            crate::wire::GoalRunStatus::Failed => task::GoalRunStatus::Failed,
            crate::wire::GoalRunStatus::Cancelled => task::GoalRunStatus::Cancelled,
        }),
        steps: r
            .steps
            .into_iter()
            .map(|step| task::GoalRunStep {
                id: step.id,
                title: step.title,
                status: step.status.map(|s| match s {
                    crate::wire::GoalRunStepStatus::Pending => task::GoalRunStatus::Queued,
                    crate::wire::GoalRunStepStatus::InProgress => task::GoalRunStatus::Running,
                    crate::wire::GoalRunStepStatus::Completed => task::GoalRunStatus::Completed,
                    crate::wire::GoalRunStepStatus::Failed => task::GoalRunStatus::Failed,
                    crate::wire::GoalRunStepStatus::Skipped => task::GoalRunStatus::Cancelled,
                }),
                order: step.position as u32,
                instructions: step.instructions,
                kind: format!("{:?}", step.kind).to_lowercase(),
                task_id: step.task_id,
                summary: step.summary,
                error: step.error,
            })
            .collect(),
        current_step_title: r.current_step_title,
        child_task_count: r.child_task_count,
        approval_count: r.approval_count,
        last_error: r.last_error,
        goal: r.goal,
        current_step_index: r.current_step_index,
        reflection_summary: r.reflection_summary,
        memory_updates: r.memory_updates,
        generated_skill_path: r.generated_skill_path,
        child_task_ids: r.child_task_ids,
        loaded_step_start: r.loaded_step_start,
        loaded_step_end: r.loaded_step_end,
        total_step_count: r.total_step_count,
        loaded_event_start: r.loaded_event_start,
        loaded_event_end: r.loaded_event_end,
        total_event_count: r.total_event_count,
        events: r
            .events
            .into_iter()
            .map(|event| task::GoalRunEvent {
                id: event.id,
                timestamp: event.timestamp,
                phase: event.phase,
                message: event.message,
                details: event.details,
                step_index: event.step_index,
                todo_snapshot: event.todo_snapshot.into_iter().map(convert_todo).collect(),
            })
            .collect(),
        created_at: 0,
        updated_at: 0,
        older_page_pending: false,
        older_page_request_cooldown_until_tick: None,
    }
}

pub(super) fn convert_checkpoint_summary(
    checkpoint: crate::wire::CheckpointSummary,
) -> task::GoalRunCheckpointSummary {
    task::GoalRunCheckpointSummary {
        id: checkpoint.id,
        checkpoint_type: checkpoint.checkpoint_type,
        step_index: checkpoint.step_index,
        task_count: checkpoint.task_count,
        context_summary_preview: checkpoint.context_summary_preview,
        created_at: checkpoint.created_at,
    }
}

pub(super) fn convert_todo(t: crate::wire::TodoItem) -> task::TodoItem {
    task::TodoItem {
        id: t.id,
        content: t.content,
        status: t.status.map(|status| match status {
            crate::wire::TodoStatus::Pending => task::TodoStatus::Pending,
            crate::wire::TodoStatus::InProgress => task::TodoStatus::InProgress,
            crate::wire::TodoStatus::Completed => task::TodoStatus::Completed,
            crate::wire::TodoStatus::Blocked => task::TodoStatus::Blocked,
        }),
        position: t.position,
        step_index: t.step_index,
        created_at: t.created_at,
        updated_at: t.updated_at,
    }
}

pub(super) fn convert_work_context(c: crate::wire::ThreadWorkContext) -> task::ThreadWorkContext {
    task::ThreadWorkContext {
        thread_id: c.thread_id,
        entries: c
            .entries
            .into_iter()
            .map(|entry| task::WorkContextEntry {
                path: entry.path,
                previous_path: entry.previous_path,
                kind: entry.kind.map(|kind| match kind {
                    crate::wire::WorkContextEntryKind::RepoChange => {
                        task::WorkContextEntryKind::RepoChange
                    }
                    crate::wire::WorkContextEntryKind::Artifact => {
                        task::WorkContextEntryKind::Artifact
                    }
                    crate::wire::WorkContextEntryKind::GeneratedSkill => {
                        task::WorkContextEntryKind::GeneratedSkill
                    }
                }),
                source: entry.source,
                change_kind: entry.change_kind,
                repo_root: entry.repo_root,
                goal_run_id: entry.goal_run_id,
                step_index: entry.step_index,
                session_id: entry.session_id,
                is_text: entry.is_text,
                updated_at: entry.updated_at,
            })
            .collect(),
    }
}

pub(super) fn convert_heartbeat(h: crate::wire::HeartbeatItem) -> task::HeartbeatItem {
    task::HeartbeatItem {
        id: h.id,
        label: h.label,
        outcome: h.last_result.map(|r| match r {
            crate::wire::HeartbeatOutcome::Ok => task::HeartbeatOutcome::Ok,
            crate::wire::HeartbeatOutcome::Alert => task::HeartbeatOutcome::Warn,
            crate::wire::HeartbeatOutcome::Error => task::HeartbeatOutcome::Error,
        }),
        message: h.last_message,
        timestamp: 0,
    }
}

#[cfg(test)]
static LAST_COPIED_TEXT: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

#[cfg(test)]
thread_local! {
    static TEST_CLIPBOARD_OWNER_HELD: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

#[cfg(test)]
pub(super) fn reset_last_copied_text() {
    *LAST_COPIED_TEXT
        .lock()
        .expect("clipboard test mutex poisoned") = None;
    TEST_CLIPBOARD_OWNER_HELD.with(|held| held.set(false));
}

#[cfg(test)]
pub(super) fn last_copied_text() -> Option<String> {
    LAST_COPIED_TEXT
        .lock()
        .expect("clipboard test mutex poisoned")
        .clone()
}

#[cfg(test)]
fn test_clipboard_owner_held() -> bool {
    TEST_CLIPBOARD_OWNER_HELD.with(std::cell::Cell::get)
}

pub(super) fn copy_to_clipboard(text: &str) {
    #[cfg(test)]
    {
        *LAST_COPIED_TEXT
            .lock()
            .expect("clipboard test mutex poisoned") = Some(text.to_string());
        TEST_CLIPBOARD_OWNER_HELD.with(|held| held.set(true));
        return;
    }

    #[cfg(not(test))]
    {
        use base64::Engine;

        let copied = SYSTEM_CLIPBOARD.with(|cell| {
            let mut slot = cell.borrow_mut();
            if slot.is_none() {
                *slot = arboard::Clipboard::new().ok();
            }

            slot.as_mut()
                .map(|clipboard| clipboard.set_text(text.to_string()).is_ok())
                .unwrap_or(false)
        });

        if !copied {
            let encoded = base64::engine::general_purpose::STANDARD.encode(text);
            print!("\x1b]52;c;{}\x07", encoded);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn convert_thread_preserves_operator_question_metadata() {
        let thread = crate::wire::AgentThread {
            id: "thread-1".into(),
            title: "Thread".into(),
            messages: vec![crate::wire::AgentMessage {
                role: crate::wire::MessageRole::Assistant,
                content: "Approve this slice?\na - proceed".into(),
                is_operator_question: true,
                operator_question_id: Some("oq-1".into()),
                operator_question_answer: Some("a".into()),
                ..Default::default()
            }],
            ..Default::default()
        };

        let converted = convert_thread(thread);
        let message = &converted.messages[0];

        assert!(message.is_operator_question);
        assert_eq!(message.operator_question_id.as_deref(), Some("oq-1"));
        assert_eq!(message.operator_question_answer.as_deref(), Some("a"));
    }

    #[test]
    fn copy_to_clipboard_keeps_owner_alive_after_write() {
        reset_last_copied_text();

        copy_to_clipboard("hello");

        assert_eq!(last_copied_text().as_deref(), Some("hello"));
        assert!(
            test_clipboard_owner_held(),
            "clipboard owner should stay alive after copy so Linux clipboard managers can read it"
        );
    }
}
