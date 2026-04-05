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

#[tokio::test]
async fn session_start_prewarm_hydrates_active_attention_thread() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.anticipatory.enabled = true;
    config.anticipatory.morning_brief = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    engine
        .record_operator_attention("conversation:chat", Some("thread-prewarm"), None)
        .await
        .unwrap();
    engine.mark_operator_present("test").await;
    engine.run_anticipatory_tick().await;

    assert!(
        engine
            .anticipatory
            .read()
            .await
            .hydration_by_thread
            .contains_key("thread-prewarm"),
        "session-start prewarm should hydrate the active attention thread before surfacing items"
    );
}

#[tokio::test]
async fn anticipatory_tick_routes_stuck_hint_to_thread_surface_with_idle_signal() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.anticipatory.enabled = true;
    config.anticipatory.stuck_detection = true;
    config.anticipatory.stuck_detection_delay_seconds = 1;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let now = now_millis();
    let mut stale_task = sample_task("task-stale", Some("thread-surface"), None);
    stale_task.title = "Review failing command".to_string();
    stale_task.status = TaskStatus::InProgress;
    stale_task.started_at = Some(now.saturating_sub(30_000));
    stale_task.last_error = Some("command timed out while waiting for output".to_string());
    engine.tasks.lock().await.push_back(stale_task);

    engine
        .set_thread_client_surface("thread-surface", amux_protocol::ClientSurface::Electron)
        .await;
    engine
        .record_operator_attention("task:detail", Some("thread-surface"), None)
        .await
        .unwrap();
    {
        let mut runtime = engine.anticipatory.write().await;
        runtime.last_presence_at = Some(now.saturating_sub(60_000));
        runtime.active_attention_updated_at = Some(now.saturating_sub(60_000));
    }

    engine.run_anticipatory_tick().await;

    let items = engine.anticipatory.read().await.items.clone();
    let item = items
        .into_iter()
        .find(|candidate| candidate.kind == "stuck_hint")
        .expect("expected a stuck hint for the stale task");
    assert_eq!(item.preferred_client_surface.as_deref(), Some("electron"));
    assert_eq!(
        item.preferred_attention_surface.as_deref(),
        Some("task:detail")
    );
    assert!(
        item.bullets
            .iter()
            .any(|bullet| bullet.contains("Operator attention has been idle")),
        "idle-aware heuristics should be surfaced in the stuck hint bullets"
    );
}

#[tokio::test]
async fn morning_brief_inherits_route_from_top_goal_surface() {
    let root = tempdir().unwrap();
    let manager = SessionManager::new_test(root.path()).await;
    let mut config = AgentConfig::default();
    config.anticipatory.enabled = true;
    config.anticipatory.morning_brief = true;
    let engine = AgentEngine::new_test(manager, config, root.path()).await;

    let now = now_millis();
    let mut goal = sample_goal_run("goal-route", Some("thread-route"));
    goal.title = "Resume release work".to_string();
    goal.status = GoalRunStatus::Running;
    goal.updated_at = now;
    goal.current_step_title = Some("publish the package".to_string());
    engine.goal_runs.lock().await.push_back(goal);
    engine
        .set_goal_run_client_surface("goal-route", amux_protocol::ClientSurface::Tui)
        .await;
    engine.mark_operator_present("test").await;

    engine.run_anticipatory_tick().await;

    let items = engine.anticipatory.read().await.items.clone();
    let item = items
        .into_iter()
        .find(|candidate| candidate.kind == "morning_brief")
        .expect("expected a morning brief");
    assert_eq!(item.preferred_client_surface.as_deref(), Some("tui"));
}
