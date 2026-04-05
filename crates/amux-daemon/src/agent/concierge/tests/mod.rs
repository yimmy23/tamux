use super::*;
use crate::agent::engine::AgentEngine;
use crate::session_manager::SessionManager;
use std::collections::HashMap;
use tempfile::tempdir;
use tokio::sync::Mutex;

mod basic;
mod context;
mod runtime;

fn test_now_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn concierge_thread(messages: Vec<AgentMessage>) -> AgentThread {
    AgentThread {
        id: CONCIERGE_THREAD_ID.to_string(),
        agent_name: None,
        title: "Concierge".to_string(),
        created_at: 1,
        updated_at: 1,
        messages,
        pinned: true,
        upstream_thread_id: None,
        upstream_transport: None,
        upstream_provider: None,
        upstream_model: None,
        upstream_assistant_id: None,
        total_input_tokens: 0,
        total_output_tokens: 0,
    }
}

fn assistant_message(content: &str, timestamp: u64) -> AgentMessage {
    AgentMessage {
        id: format!("assistant-{timestamp}"),
        role: MessageRole::Assistant,
        content: content.to_string(),
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
        reasoning: None,
        message_kind: AgentMessageKind::Normal,
        compaction_strategy: None,
        compaction_payload: None,
        timestamp,
    }
}

fn user_message(content: &str, timestamp: u64) -> AgentMessage {
    AgentMessage::user(content, timestamp)
}

fn thread_with_messages(
    id: &str,
    title: &str,
    updated_at: u64,
    messages: Vec<AgentMessage>,
) -> AgentThread {
    AgentThread {
        id: id.to_string(),
        agent_name: None,
        title: title.to_string(),
        created_at: 1,
        updated_at,
        pinned: false,
        upstream_thread_id: None,
        upstream_transport: None,
        upstream_provider: None,
        upstream_model: None,
        upstream_assistant_id: None,
        total_input_tokens: 0,
        total_output_tokens: 0,
        messages,
    }
}

fn sample_task(id: &str, title: &str, created_at: u64) -> AgentTask {
    AgentTask {
        id: id.to_string(),
        title: title.to_string(),
        description: title.to_string(),
        status: TaskStatus::InProgress,
        priority: TaskPriority::Normal,
        progress: 0,
        created_at,
        started_at: None,
        completed_at: None,
        error: None,
        result: None,
        thread_id: None,
        source: "user".to_string(),
        notify_on_complete: false,
        notify_channels: Vec::new(),
        dependencies: Vec::new(),
        command: None,
        session_id: None,
        goal_run_id: None,
        goal_run_title: None,
        goal_step_id: None,
        goal_step_title: None,
        parent_task_id: None,
        parent_thread_id: None,
        runtime: "daemon".to_string(),
        retry_count: 0,
        max_retries: 3,
        next_retry_at: None,
        scheduled_at: None,
        blocked_reason: None,
        awaiting_approval_id: None,
        lane_id: None,
        last_error: None,
        logs: Vec::new(),
        tool_whitelist: None,
        tool_blacklist: None,
        context_budget_tokens: None,
        context_overflow_action: None,
        termination_conditions: None,
        success_criteria: None,
        max_duration_secs: None,
        supervisor_config: None,
        override_provider: None,
        override_model: None,
        override_system_prompt: None,
        sub_agent_def_id: None,
    }
}

fn sample_task_for_thread(id: &str, title: &str, created_at: u64, thread_id: &str) -> AgentTask {
    let mut task = sample_task(id, title, created_at);
    task.thread_id = Some(thread_id.to_string());
    task
}
