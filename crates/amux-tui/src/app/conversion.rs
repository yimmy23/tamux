use super::*;

pub(super) fn convert_thread(t: crate::wire::AgentThread) -> chat::AgentThread {
    chat::AgentThread {
        id: t.id,
        title: t.title,
        created_at: t.created_at,
        updated_at: t.updated_at,
        messages: t.messages.into_iter().map(convert_message).collect(),
        total_input_tokens: t.total_input_tokens,
        total_output_tokens: t.total_output_tokens,
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
        tool_name: m.tool_name,
        tool_arguments: m.tool_arguments,
        tool_call_id: m.tool_call_id,
        tool_status: m.tool_status,
        input_tokens: m.input_tokens,
        output_tokens: m.output_tokens,
        tps: m.tps,
        generation_ms: m.generation_ms,
        cost: m.cost,
        is_streaming: m.is_streaming,
        timestamp: m.timestamp,
    }
}

pub(super) fn convert_task(t: crate::wire::AgentTask) -> task::AgentTask {
    task::AgentTask {
        id: t.id,
        title: t.title,
        thread_id: t.thread_id,
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
            crate::wire::GoalRunStatus::Queued => task::GoalRunStatus::Pending,
            crate::wire::GoalRunStatus::Planning => task::GoalRunStatus::Pending,
            crate::wire::GoalRunStatus::Running => task::GoalRunStatus::Running,
            crate::wire::GoalRunStatus::AwaitingApproval => task::GoalRunStatus::Pending,
            crate::wire::GoalRunStatus::Paused => task::GoalRunStatus::Pending,
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
                    crate::wire::GoalRunStepStatus::Pending => task::GoalRunStatus::Pending,
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

pub(super) fn copy_to_clipboard(text: &str) {
    use base64::Engine;

    let encoded = base64::engine::general_purpose::STANDARD.encode(text);
    print!("\x1b]52;c;{}\x07", encoded);
}
