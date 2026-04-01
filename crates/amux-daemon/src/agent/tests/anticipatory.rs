use super::*;
use crate::session_manager::SessionManager;
use tempfile::tempdir;

fn sample_task(id: &str, thread_id: Option<&str>, goal_run_id: Option<&str>) -> AgentTask {
    AgentTask {
        id: id.to_string(),
        title: id.to_string(),
        description: String::new(),
        status: TaskStatus::Queued,
        priority: TaskPriority::Normal,
        progress: 0,
        created_at: 0,
        started_at: None,
        completed_at: None,
        error: None,
        result: None,
        thread_id: thread_id.map(str::to_string),
        source: "user".to_string(),
        notify_on_complete: false,
        notify_channels: Vec::new(),
        dependencies: Vec::new(),
        command: None,
        session_id: None,
        goal_run_id: goal_run_id.map(str::to_string),
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

fn sample_goal_run(id: &str, thread_id: Option<&str>) -> GoalRun {
    GoalRun {
        id: id.to_string(),
        title: id.to_string(),
        goal: String::new(),
        client_request_id: None,
        status: GoalRunStatus::Queued,
        priority: TaskPriority::Normal,
        created_at: 0,
        updated_at: 0,
        started_at: None,
        completed_at: None,
        thread_id: thread_id.map(str::to_string),
        session_id: None,
        current_step_index: 0,
        current_step_title: None,
        current_step_kind: None,
        replan_count: 0,
        max_replans: 3,
        plan_summary: None,
        reflection_summary: None,
        memory_updates: Vec::new(),
        generated_skill_path: None,
        last_error: None,
        failure_cause: None,
        child_task_ids: Vec::new(),
        child_task_count: 0,
        approval_count: 0,
        awaiting_approval_id: None,
        active_task_id: None,
        duration_ms: None,
        steps: Vec::new(),
        events: Vec::new(),
        total_prompt_tokens: 0,
        total_completion_tokens: 0,
        estimated_cost_usd: None,
        autonomy_level: Default::default(),
        authorship_tag: None,
    }
}

#[test]
fn circular_hour_distance_wraps_across_midnight() {
    assert_eq!(circular_hour_distance(23, 1), 2);
    assert_eq!(circular_hour_distance(5, 5), 0);
}

#[test]
fn truncate_hint_shortens_long_strings() {
    let value = "x".repeat(160);
    let shortened = truncate_hint(&value);
    assert!(shortened.len() < value.len());
    assert!(shortened.ends_with('…'));
}

#[test]
fn attention_surface_suppresses_briefs_in_settings() {
    assert!(!should_surface_anticipatory_kind(
        "morning_brief",
        Some("modal:settings:provider")
    ));
    assert!(!should_surface_anticipatory_kind(
        "stuck_hint",
        Some("modal:approval")
    ));
}

#[test]
fn attention_surface_allows_task_and_conversation_contexts() {
    assert!(should_surface_anticipatory_kind(
        "morning_brief",
        Some("conversation:chat")
    ));
    assert!(should_surface_anticipatory_kind(
        "stuck_hint",
        Some("task:detail")
    ));
}

#[test]
fn task_attention_priority_prefers_goal_then_thread() {
    let attention = AttentionFocus {
        thread_id: Some("thread_1".to_string()),
        goal_run_id: Some("goal_1".to_string()),
    };
    let goal_match = sample_task("task_goal", Some("thread_9"), Some("goal_1"));
    let thread_match = sample_task("task_thread", Some("thread_1"), Some("goal_9"));

    assert_eq!(task_attention_priority(&goal_match, &attention), 2);
    assert_eq!(task_attention_priority(&thread_match, &attention), 1);
}

#[test]
fn goal_attention_priority_prefers_exact_goal_match() {
    let attention = AttentionFocus {
        thread_id: Some("thread_1".to_string()),
        goal_run_id: Some("goal_1".to_string()),
    };
    let exact = sample_goal_run("goal_1", Some("thread_2"));
    let thread_only = sample_goal_run("goal_2", Some("thread_1"));

    assert_eq!(goal_attention_priority(&exact, &attention), 2);
    assert_eq!(goal_attention_priority(&thread_only, &attention), 1);
}

#[tokio::test]
async fn anticipatory_tick_ignores_weles_owned_stuck_tasks() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.anticipatory.enabled = true;
    config.anticipatory.stuck_detection = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let mut weles_task = sample_task("task-weles", Some("thread-weles"), None);
    weles_task.title = "WELES".to_string();
    weles_task.status = TaskStatus::Blocked;
    weles_task.sub_agent_def_id = Some("weles_builtin".to_string());
    weles_task.blocked_reason = Some("waiting for lane availability: daemon-main".to_string());
    weles_task.started_at = Some(1);

    engine.tasks.lock().await.push_back(weles_task);
    engine.run_anticipatory_tick().await;

    assert!(
        engine.anticipatory.read().await.items.is_empty(),
        "WELES-owned blocked tasks should not surface as anticipatory hints"
    );
}
