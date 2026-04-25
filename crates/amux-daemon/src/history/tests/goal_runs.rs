use super::*;
use crate::agent::types::{
    GoalAgentAssignment, GoalDeliveryUnit, GoalProjectionState, GoalResumeAction,
    GoalResumeDecision, GoalRoleBinding, GoalRunDossier, GoalRuntimeOwnerProfile,
};
use crate::history::schema_helpers::table_has_column;

fn sample_assignment(
    role_id: &str,
    provider: &str,
    model: &str,
    reasoning_effort: Option<&str>,
) -> GoalAgentAssignment {
    GoalAgentAssignment {
        role_id: role_id.to_string(),
        enabled: true,
        provider: provider.to_string(),
        model: model.to_string(),
        reasoning_effort: reasoning_effort.map(str::to_string),
        inherit_from_main: false,
    }
}

#[tokio::test]
async fn init_schema_migrates_legacy_agent_tasks_before_goal_run_index() -> Result<()> {
    let (store, root) = make_test_store().await?;
    // Drop existing tables and recreate with legacy schema (missing columns)
    store.conn.call(|conn| {
        conn.execute_batch("DROP TABLE IF EXISTS agent_tasks")?;
        conn.execute_batch(
        "
        CREATE TABLE agent_tasks (
            id                   TEXT PRIMARY KEY,
            title                TEXT NOT NULL,
            description          TEXT NOT NULL,
            status               TEXT NOT NULL,
            priority             TEXT NOT NULL,
            progress             INTEGER NOT NULL DEFAULT 0,
            created_at           INTEGER NOT NULL,
            started_at           INTEGER,
            completed_at         INTEGER,
            error                TEXT,
            result               TEXT,
            thread_id            TEXT,
            source               TEXT NOT NULL DEFAULT 'user',
            notify_on_complete   INTEGER NOT NULL DEFAULT 0,
            notify_channels_json TEXT NOT NULL DEFAULT '[]',
            command              TEXT,
            retry_count          INTEGER NOT NULL DEFAULT 0,
            max_retries          INTEGER NOT NULL DEFAULT 3,
            next_retry_at        INTEGER,
            blocked_reason       TEXT,
            awaiting_approval_id TEXT,
            lane_id              TEXT,
            last_error           TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_agent_tasks_status ON agent_tasks(status, priority, created_at DESC);
        ",
    )?;
        Ok(())
    }).await.map_err(|e| anyhow::anyhow!("{e}"))?;

    store.init_schema().await?;

    let has_cols = store.conn.call(|conn| {
        let has_session = table_has_column(conn, "agent_tasks", "session_id")?;
        let has_scheduled = table_has_column(conn, "agent_tasks", "scheduled_at")?;
        let has_goal_run = table_has_column(conn, "agent_tasks", "goal_run_id")?;
        let has_override_provider = table_has_column(conn, "agent_tasks", "override_provider")?;
        let has_override_prompt = table_has_column(conn, "agent_tasks", "override_system_prompt")?;
        let has_sub_agent_def = table_has_column(conn, "agent_tasks", "sub_agent_def_id")?;
        let has_tool_whitelist = table_has_column(conn, "agent_tasks", "tool_whitelist_json")?;
        let has_tool_blacklist = table_has_column(conn, "agent_tasks", "tool_blacklist_json")?;
        let has_context_budget = table_has_column(conn, "agent_tasks", "context_budget_tokens")?;
        let has_context_overflow = table_has_column(conn, "agent_tasks", "context_overflow_action")?;
        let has_termination_conditions = table_has_column(conn, "agent_tasks", "termination_conditions")?;
        let has_success_criteria = table_has_column(conn, "agent_tasks", "success_criteria")?;
        let has_max_duration = table_has_column(conn, "agent_tasks", "max_duration_secs")?;
        let has_supervisor_config = table_has_column(conn, "agent_tasks", "supervisor_config_json")?;
        let index_name: Option<String> = conn
            .query_row(
                "SELECT name FROM sqlite_master WHERE type = 'index' AND name = 'idx_agent_tasks_goal_run'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        Ok((
            has_session,
            has_scheduled,
            has_goal_run,
            has_override_provider,
            has_override_prompt,
            has_sub_agent_def,
            has_tool_whitelist,
            has_tool_blacklist,
            has_context_budget,
            has_context_overflow,
            has_termination_conditions,
            has_success_criteria,
            has_max_duration,
            has_supervisor_config,
            index_name,
        ))
    }).await.map_err(|e| anyhow::anyhow!("{e}"))?;

    assert!(has_cols.0);
    assert!(has_cols.1);
    assert!(has_cols.2);
    assert!(has_cols.3);
    assert!(has_cols.4);
    assert!(has_cols.5);
    assert!(has_cols.6);
    assert!(has_cols.7);
    assert!(has_cols.8);
    assert!(has_cols.9);
    assert!(has_cols.10);
    assert!(has_cols.11);
    assert!(has_cols.12);
    assert!(has_cols.13);
    assert_eq!(has_cols.14.as_deref(), Some("idx_agent_tasks_goal_run"));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn agent_task_subagent_metadata_round_trips() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let task = AgentTask {
        id: "task-subagent-meta".to_string(),
        title: "Persist subagent metadata".to_string(),
        description: "Ensure task persistence keeps self-orchestration config".to_string(),
        status: TaskStatus::InProgress,
        priority: TaskPriority::High,
        progress: 66,
        created_at: 100,
        started_at: Some(101),
        completed_at: None,
        error: None,
        result: Some("partial".to_string()),
        thread_id: Some("thread-subagent".to_string()),
        source: "agent".to_string(),
        notify_on_complete: true,
        notify_channels: vec!["slack".to_string(), "discord".to_string()],
        dependencies: vec!["dep-1".to_string(), "dep-2".to_string()],
        command: Some("cargo test -p amux-daemon".to_string()),
        session_id: Some("session-subagent".to_string()),
        goal_run_id: Some("goal-77".to_string()),
        goal_run_title: Some("Goal title".to_string()),
        goal_step_id: Some("step-9".to_string()),
        goal_step_title: Some("Investigate failure".to_string()),
        parent_task_id: Some("parent-task".to_string()),
        parent_thread_id: Some("parent-thread".to_string()),
        runtime: "daemon".to_string(),
        retry_count: 1,
        max_retries: 4,
        next_retry_at: Some(150),
        scheduled_at: Some(140),
        blocked_reason: Some("waiting for sibling evidence".to_string()),
        awaiting_approval_id: Some("approval-9".to_string()),
        policy_fingerprint: Some("policy-9".to_string()),
        approval_expires_at: Some(999),
        containment_scope: Some("workspace".to_string()),
        compensation_status: Some("required".to_string()),
        compensation_summary: Some("rollback staged".to_string()),
        lane_id: Some("lane-7".to_string()),
        last_error: Some("previous retry timed out".to_string()),
        logs: vec![AgentTaskLogEntry {
            id: "log-1".to_string(),
            timestamp: 111,
            level: TaskLogLevel::Warn,
            phase: "supervisor".to_string(),
            message: "stuck pattern detected".to_string(),
            details: Some("tool loop A→B→A→B".to_string()),
            attempt: 1,
        }],
        tool_whitelist: Some(vec!["read_file".to_string(), "search_files".to_string()]),
        tool_blacklist: Some(vec!["bash_command".to_string()]),
        context_budget_tokens: Some(20_000),
        context_overflow_action: Some(crate::agent::types::ContextOverflowAction::Truncate),
        termination_conditions: Some("timeout(300) OR error_count(3)".to_string()),
        success_criteria: Some("All targeted tests pass".to_string()),
        max_duration_secs: Some(600),
        supervisor_config: Some(crate::agent::types::SupervisorConfig {
            check_interval_secs: 45,
            stuck_timeout_secs: 240,
            max_retries: 5,
            intervention_level: crate::agent::types::InterventionLevel::Aggressive,
        }),
        override_provider: Some("github-copilot".to_string()),
        override_model: Some("gpt-5.4".to_string()),
        override_system_prompt: Some("You are a focused subagent".to_string()),
        sub_agent_def_id: Some("weles_builtin".to_string()),
    };

    store.upsert_agent_task(&task).await?;
    let loaded = store
        .list_agent_tasks()
        .await?
        .into_iter()
        .find(|entry| entry.id == task.id)
        .expect("persisted task should be present");

    assert_eq!(loaded.dependencies, vec!["dep-1", "dep-2"]);
    assert_eq!(loaded.notify_channels, vec!["slack", "discord"]);
    assert_eq!(loaded.logs.len(), 1);
    assert_eq!(loaded.logs[0].phase, "supervisor");
    assert_eq!(
        loaded.tool_whitelist,
        Some(vec!["read_file".to_string(), "search_files".to_string()])
    );
    assert_eq!(
        loaded.tool_blacklist,
        Some(vec!["bash_command".to_string()])
    );
    assert_eq!(loaded.context_budget_tokens, Some(20_000));
    assert_eq!(
        loaded.context_overflow_action,
        Some(crate::agent::types::ContextOverflowAction::Truncate)
    );
    assert_eq!(
        loaded.termination_conditions.as_deref(),
        Some("timeout(300) OR error_count(3)")
    );
    assert_eq!(
        loaded.success_criteria.as_deref(),
        Some("All targeted tests pass")
    );
    assert_eq!(loaded.max_duration_secs, Some(600));
    let supervisor = loaded
        .supervisor_config
        .expect("supervisor config should round-trip");
    assert_eq!(supervisor.check_interval_secs, 45);
    assert_eq!(supervisor.stuck_timeout_secs, 240);
    assert_eq!(supervisor.max_retries, 5);
    assert_eq!(
        supervisor.intervention_level,
        crate::agent::types::InterventionLevel::Aggressive
    );
    assert_eq!(loaded.override_provider.as_deref(), Some("github-copilot"));
    assert_eq!(loaded.override_model.as_deref(), Some("gpt-5.4"));
    assert_eq!(loaded.sub_agent_def_id.as_deref(), Some("weles_builtin"));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn goal_run_event_todo_snapshot_round_trips() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let goal_run = GoalRun {
        id: "goal-1".to_string(),
        title: "Goal".to_string(),
        goal: "Do the thing".to_string(),
        client_request_id: None,
        status: GoalRunStatus::Running,
        priority: TaskPriority::Normal,
        created_at: 1,
        updated_at: 2,
        started_at: Some(1),
        completed_at: None,
        thread_id: Some("thread-1".to_string()),
        session_id: None,
        current_step_index: 0,
        current_step_title: Some("Inspect".to_string()),
        current_step_kind: Some(GoalRunStepKind::Research),
        planner_owner_profile: None,
        current_step_owner_profile: None,
        replan_count: 0,
        max_replans: 2,
        plan_summary: Some("Plan".to_string()),
        reflection_summary: None,
        memory_updates: Vec::new(),
        generated_skill_path: None,
        last_error: None,
        failure_cause: None,
        dossier: None,
        stopped_reason: None,
        child_task_ids: Vec::new(),
        child_task_count: 0,
        approval_count: 0,
        awaiting_approval_id: None,
        policy_fingerprint: None,
        approval_expires_at: None,
        containment_scope: None,
        compensation_status: None,
        compensation_summary: None,
        active_task_id: None,
        duration_ms: None,
        steps: vec![GoalRunStep {
            id: "step-1".to_string(),
            position: 0,
            title: "Inspect".to_string(),
            instructions: "Inspect state".to_string(),
            kind: GoalRunStepKind::Research,
            success_criteria: "Know state".to_string(),
            session_id: None,
            status: GoalRunStepStatus::InProgress,
            task_id: None,
            summary: None,
            error: None,
            started_at: Some(1),
            completed_at: None,
        }],
        events: vec![GoalRunEvent {
            id: "event-1".to_string(),
            timestamp: 3,
            phase: "todo".to_string(),
            message: "goal todo updated".to_string(),
            details: None,
            step_index: Some(0),
            todo_snapshot: vec![crate::agent::types::TodoItem {
                id: "todo-1".to_string(),
                content: "Inspect state".to_string(),
                status: crate::agent::types::TodoStatus::InProgress,
                position: 0,
                step_index: Some(0),
                created_at: 3,
                updated_at: 3,
            }],
        }],
        total_prompt_tokens: 0,
        total_completion_tokens: 0,
        estimated_cost_usd: None,
        autonomy_level: Default::default(),
        authorship_tag: None,
        launch_assignment_snapshot: Vec::new(),
        runtime_assignment_list: Vec::new(),
        root_thread_id: None,
        active_thread_id: None,
        execution_thread_ids: Vec::new(),
    };

    store.upsert_goal_run(&goal_run).await?;
    let loaded = store
        .get_goal_run("goal-1")
        .await?
        .expect("goal run should exist after upsert");

    assert_eq!(loaded.events.len(), 1);
    assert_eq!(loaded.events[0].step_index, Some(0));
    assert_eq!(loaded.events[0].todo_snapshot.len(), 1);
    assert_eq!(loaded.events[0].todo_snapshot[0].content, "Inspect state");

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn goal_run_extended_metadata_round_trips() -> Result<()> {
    let (store, root) = make_test_store().await?;

    let goal_run = GoalRun {
        id: "goal-meta".to_string(),
        title: "Goal metadata".to_string(),
        goal: "Verify all spec metadata persists".to_string(),
        client_request_id: Some("client-1".to_string()),
        status: GoalRunStatus::AwaitingApproval,
        priority: TaskPriority::High,
        created_at: 10,
        updated_at: 20,
        started_at: Some(11),
        completed_at: Some(99),
        thread_id: Some("thread-2".to_string()),
        session_id: Some("session-2".to_string()),
        current_step_index: 0,
        current_step_title: Some("Execute safely".to_string()),
        current_step_kind: Some(GoalRunStepKind::Command),
        planner_owner_profile: Some(GoalRuntimeOwnerProfile {
            agent_label: "Swarog".to_string(),
            provider: "openai".to_string(),
            model: "gpt-5.4".to_string(),
            reasoning_effort: Some("high".to_string()),
        }),
        current_step_owner_profile: Some(GoalRuntimeOwnerProfile {
            agent_label: "Android Verifier".to_string(),
            provider: "openai".to_string(),
            model: "gpt-4o-mini".to_string(),
            reasoning_effort: None,
        }),
        replan_count: 1,
        max_replans: 3,
        plan_summary: Some("Plan summary".to_string()),
        reflection_summary: Some("Reflection summary".to_string()),
        memory_updates: vec!["remember this".to_string()],
        generated_skill_path: Some("skills/generated/goal-meta.md".to_string()),
        last_error: Some("waiting for approval".to_string()),
        failure_cause: Some("policy gate".to_string()),
        stopped_reason: Some("operator requested stop".to_string()),
        child_task_ids: vec!["task-1".to_string(), "task-2".to_string()],
        child_task_count: 2,
        approval_count: 1,
        awaiting_approval_id: Some("approval-1".to_string()),
        policy_fingerprint: Some("fingerprint-1".to_string()),
        approval_expires_at: Some(12345),
        containment_scope: Some("workspace".to_string()),
        compensation_status: Some("required".to_string()),
        compensation_summary: Some("rollback pending".to_string()),
        active_task_id: Some("task-2".to_string()),
        duration_ms: Some(888),
        launch_assignment_snapshot: vec![
            sample_assignment("planner", "openai", "gpt-5.4", Some("high")),
            sample_assignment("executor", "anthropic", "claude-sonnet-4", None),
        ],
        runtime_assignment_list: vec![sample_assignment(
            "executor",
            "anthropic",
            "claude-sonnet-4",
            None,
        )],
        root_thread_id: Some("thread-root".to_string()),
        active_thread_id: Some("thread-active".to_string()),
        execution_thread_ids: vec![
            "thread-root".to_string(),
            "thread-active".to_string(),
            "thread-followup".to_string(),
        ],
        steps: vec![GoalRunStep {
            id: "step-meta".to_string(),
            position: 0,
            title: "Execute safely".to_string(),
            instructions: "Run guarded command".to_string(),
            kind: GoalRunStepKind::Command,
            success_criteria: "Command finished".to_string(),
            session_id: Some("session-2".to_string()),
            status: GoalRunStepStatus::InProgress,
            task_id: Some("task-2".to_string()),
            summary: Some("waiting".to_string()),
            error: None,
            started_at: Some(11),
            completed_at: None,
        }],
        events: vec![],
        dossier: Some(GoalRunDossier {
            units: vec![GoalDeliveryUnit {
                id: "unit-1".to_string(),
                title: "Implement guarded command".to_string(),
                status: GoalProjectionState::Completed,
                execution_binding: GoalRoleBinding::Builtin("swarog".to_string()),
                verification_binding: GoalRoleBinding::Subagent("android-verifier".to_string()),
                ..Default::default()
            }],
            latest_resume_decision: Some(GoalResumeDecision {
                action: GoalResumeAction::Stop,
                reason_code: "operator_stop".to_string(),
                projection_state: GoalProjectionState::Completed,
                ..Default::default()
            }),
            summary: Some("One unit completed and verified".to_string()),
            ..Default::default()
        }),
        total_prompt_tokens: 123,
        total_completion_tokens: 456,
        estimated_cost_usd: Some(0.42),
        autonomy_level: crate::agent::AutonomyLevel::Supervised,
        authorship_tag: Some(crate::agent::AuthorshipTag::Joint),
    };

    store.upsert_goal_run(&goal_run).await?;
    let loaded = store
        .get_goal_run("goal-meta")
        .await?
        .expect("goal run should exist after upsert");

    assert_eq!(loaded.completed_at, Some(99));
    assert_eq!(loaded.current_step_title.as_deref(), Some("Execute safely"));
    assert_eq!(loaded.current_step_kind, Some(GoalRunStepKind::Command));
    assert_eq!(loaded.failure_cause.as_deref(), Some("policy gate"));
    assert_eq!(loaded.child_task_ids, vec!["task-1", "task-2"]);
    assert_eq!(loaded.child_task_count, 2);
    assert_eq!(loaded.approval_count, 1);
    assert_eq!(loaded.awaiting_approval_id.as_deref(), Some("approval-1"));
    assert_eq!(loaded.policy_fingerprint.as_deref(), Some("fingerprint-1"));
    assert_eq!(loaded.approval_expires_at, Some(12345));
    assert_eq!(loaded.containment_scope.as_deref(), Some("workspace"));
    assert_eq!(loaded.compensation_status.as_deref(), Some("required"));
    assert_eq!(
        loaded.compensation_summary.as_deref(),
        Some("rollback pending")
    );
    assert_eq!(loaded.active_task_id.as_deref(), Some("task-2"));
    assert_eq!(loaded.duration_ms, Some(888));
    assert_eq!(
        loaded.stopped_reason.as_deref(),
        Some("operator requested stop")
    );
    let dossier = loaded.dossier.expect("dossier should round-trip");
    assert_eq!(dossier.units.len(), 1);
    assert_eq!(dossier.units[0].id, "unit-1");
    assert_eq!(
        dossier.units[0].verification_binding,
        GoalRoleBinding::Subagent("android-verifier".to_string())
    );
    assert_eq!(
        dossier
            .latest_resume_decision
            .expect("resume decision should round-trip")
            .reason_code,
        "operator_stop"
    );
    assert_eq!(loaded.total_prompt_tokens, 123);
    assert_eq!(loaded.total_completion_tokens, 456);
    assert_eq!(loaded.estimated_cost_usd, Some(0.42));
    assert_eq!(
        loaded.autonomy_level,
        crate::agent::AutonomyLevel::Supervised
    );
    assert_eq!(
        loaded.authorship_tag,
        Some(crate::agent::AuthorshipTag::Joint)
    );
    assert_eq!(
        loaded.planner_owner_profile,
        Some(GoalRuntimeOwnerProfile {
            agent_label: "Swarog".to_string(),
            provider: "openai".to_string(),
            model: "gpt-5.4".to_string(),
            reasoning_effort: Some("high".to_string()),
        })
    );
    assert_eq!(
        loaded.current_step_owner_profile,
        Some(GoalRuntimeOwnerProfile {
            agent_label: "Android Verifier".to_string(),
            provider: "openai".to_string(),
            model: "gpt-4o-mini".to_string(),
            reasoning_effort: None,
        })
    );
    assert_eq!(
        loaded.launch_assignment_snapshot,
        vec![
            sample_assignment("planner", "openai", "gpt-5.4", Some("high")),
            sample_assignment("executor", "anthropic", "claude-sonnet-4", None),
        ]
    );
    assert_eq!(
        loaded.runtime_assignment_list,
        vec![sample_assignment(
            "executor",
            "anthropic",
            "claude-sonnet-4",
            None,
        )]
    );
    assert_eq!(loaded.root_thread_id.as_deref(), Some("thread-root"));
    assert_eq!(loaded.active_thread_id.as_deref(), Some("thread-active"));
    assert_eq!(
        loaded.execution_thread_ids,
        vec![
            "thread-root".to_string(),
            "thread-active".to_string(),
            "thread-followup".to_string(),
        ]
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn init_schema_migrates_legacy_goal_runs_metadata_columns() -> Result<()> {
    let (store, root) = make_test_store().await?;
    store
        .conn
        .call(|conn| {
            conn.execute_batch("DROP TABLE IF EXISTS goal_runs")?;
            conn.execute_batch(
                "
            CREATE TABLE goal_runs (
                id                  TEXT PRIMARY KEY,
                title               TEXT NOT NULL,
                goal                TEXT NOT NULL,
                client_request_id   TEXT,
                status              TEXT NOT NULL,
                priority            TEXT NOT NULL,
                created_at          INTEGER NOT NULL,
                updated_at          INTEGER NOT NULL,
                started_at          INTEGER,
                completed_at        INTEGER,
                thread_id           TEXT,
                session_id          TEXT,
                current_step_index  INTEGER NOT NULL DEFAULT 0,
                replan_count        INTEGER NOT NULL DEFAULT 0,
                max_replans         INTEGER NOT NULL DEFAULT 2,
                plan_summary        TEXT,
                reflection_summary  TEXT,
                memory_updates_json TEXT NOT NULL DEFAULT '[]',
                generated_skill_path TEXT,
                last_error          TEXT,
                child_task_ids_json TEXT NOT NULL DEFAULT '[]'
            );
            CREATE INDEX IF NOT EXISTS idx_goal_runs_status ON goal_runs(status, updated_at DESC);
            ",
            )?;
            conn.execute(
                "INSERT INTO goal_runs (
                    id,
                    title,
                    goal,
                    client_request_id,
                    status,
                    priority,
                    created_at,
                    updated_at,
                    started_at,
                    completed_at,
                    thread_id,
                    session_id,
                    current_step_index,
                    replan_count,
                    max_replans,
                    plan_summary,
                    reflection_summary,
                    memory_updates_json,
                    generated_skill_path,
                    last_error,
                    child_task_ids_json
                ) VALUES (
                    ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21
                )",
                rusqlite::params![
                    "goal-legacy",
                    "legacy title",
                    "legacy goal",
                    Option::<String>::None,
                    "running",
                    "normal",
                    10_i64,
                    20_i64,
                    Option::<i64>::None,
                    Option::<i64>::None,
                    Some("thread-legacy"),
                    Option::<String>::None,
                    0_i64,
                    0_i64,
                    2_i64,
                    Option::<String>::None,
                    Option::<String>::None,
                    "[]",
                    Option::<String>::None,
                    Option::<String>::None,
                    "[]",
                ],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    store.init_schema().await?;

    let cols = store
        .conn
        .call(|conn| {
            Ok((
                table_has_column(conn, "goal_runs", "failure_cause")?,
                table_has_column(conn, "goal_runs", "stopped_reason")?,
                table_has_column(conn, "goal_runs", "child_task_count")?,
                table_has_column(conn, "goal_runs", "approval_count")?,
                table_has_column(conn, "goal_runs", "awaiting_approval_id")?,
                table_has_column(conn, "goal_runs", "policy_fingerprint")?,
                table_has_column(conn, "goal_runs", "approval_expires_at")?,
                table_has_column(conn, "goal_runs", "containment_scope")?,
                table_has_column(conn, "goal_runs", "compensation_status")?,
                table_has_column(conn, "goal_runs", "compensation_summary")?,
                table_has_column(conn, "goal_runs", "active_task_id")?,
                table_has_column(conn, "goal_runs", "duration_ms")?,
                table_has_column(conn, "goal_runs", "dossier_json")?,
                table_has_column(conn, "goal_runs", "total_prompt_tokens")?,
                table_has_column(conn, "goal_runs", "total_completion_tokens")?,
                table_has_column(conn, "goal_runs", "estimated_cost_usd")?,
                table_has_column(conn, "goal_runs", "autonomy_level")?,
                table_has_column(conn, "goal_runs", "authorship_tag")?,
                table_has_column(conn, "goal_runs", "planner_owner_profile_json")?,
                table_has_column(conn, "goal_runs", "current_step_owner_profile_json")?,
                table_has_column(conn, "goal_runs", "launch_assignment_snapshot_json")?,
                table_has_column(conn, "goal_runs", "runtime_assignment_list_json")?,
                table_has_column(conn, "goal_runs", "root_thread_id")?,
                table_has_column(conn, "goal_runs", "active_thread_id")?,
                table_has_column(conn, "goal_runs", "execution_thread_ids_json")?,
            ))
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;

    assert!(cols.0);
    assert!(cols.1);
    assert!(cols.2);
    assert!(cols.3);
    assert!(cols.4);
    assert!(cols.5);
    assert!(cols.6);
    assert!(cols.7);
    assert!(cols.8);
    assert!(cols.9);
    assert!(cols.10);
    assert!(cols.11);
    assert!(cols.12);
    assert!(cols.13);
    assert!(cols.14);
    assert!(cols.15);
    assert!(cols.16);
    assert!(cols.17);
    assert!(cols.18);
    assert!(cols.19);
    assert!(cols.20);
    assert!(cols.21);
    assert!(cols.22);
    assert!(cols.23);
    assert!(cols.24);

    let legacy_goal = store
        .list_goal_runs()
        .await?
        .into_iter()
        .find(|goal_run| goal_run.id == "goal-legacy")
        .expect("legacy goal should remain readable after migration");
    assert_eq!(legacy_goal.root_thread_id.as_deref(), Some("thread-legacy"));
    assert_eq!(
        legacy_goal.active_thread_id.as_deref(),
        Some("thread-legacy")
    );
    assert_eq!(
        legacy_goal.execution_thread_ids,
        vec!["thread-legacy".to_string()]
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn memory_provenance_write_round_trips() -> Result<()> {
    let (store, root) = make_test_store().await?;
    store.init_schema().await?;

    let fact_keys = vec!["shell".to_string(), "editor".to_string()];
    store
        .record_memory_provenance(&MemoryProvenanceRecord {
            id: "mem-1",
            target: "USER.md",
            mode: "append",
            source_kind: "tool",
            content: "- shell: bash",
            fact_keys: &fact_keys,
            thread_id: Some("thread-1"),
            task_id: Some("task-1"),
            goal_run_id: None,
            created_at: 42,
            sign: true,
        })
        .await?;

    let row = store.conn.call(|conn| {
        conn.query_row(
            "SELECT target, mode, source_kind, content, fact_keys_json, entry_hash, signature_scheme FROM memory_provenance WHERE id = 'mem-1'",
            [],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?, row.get::<_, String>(4)?, row.get::<_, String>(5)?, row.get::<_, Option<String>>(6)?)),
        ).map_err(Into::into)
    }).await.map_err(|e| anyhow::anyhow!("{e}"))?;
    assert_eq!(row.0, "USER.md");
    assert_eq!(row.1, "append");
    assert_eq!(row.2, "tool");
    assert_eq!(row.3, "- shell: bash");
    assert_eq!(
        serde_json::from_str::<Vec<String>>(&row.4)?,
        vec!["shell".to_string(), "editor".to_string()]
    );
    assert!(!row.5.is_empty());
    assert_eq!(row.6.as_deref(), Some("ed25519"));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn memory_provenance_report_marks_old_entries_uncertain() -> Result<()> {
    let (store, root) = make_test_store().await?;
    store.init_schema().await?;
    let recent_keys = vec!["shell".to_string()];
    let old_keys = vec!["editor".to_string()];
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    store
        .record_memory_provenance(&MemoryProvenanceRecord {
            id: "recent",
            target: "USER.md",
            mode: "append",
            source_kind: "tool",
            content: "- shell: bash",
            fact_keys: &recent_keys,
            thread_id: None,
            task_id: None,
            goal_run_id: None,
            created_at: now_ms,
            sign: true,
        })
        .await?;
    store
        .record_memory_provenance(&MemoryProvenanceRecord {
            id: "old",
            target: "MEMORY.md",
            mode: "append",
            source_kind: "goal_reflection",
            content: "- editor: helix",
            fact_keys: &old_keys,
            thread_id: None,
            task_id: None,
            goal_run_id: None,
            created_at: now_ms.saturating_sub(40 * 86_400_000),
            sign: true,
        })
        .await?;

    let report = store.memory_provenance_report(None, 10).await?;
    assert_eq!(report.total_entries, 2);
    assert_eq!(report.summary_by_status.get("active").copied(), Some(1));
    assert_eq!(report.summary_by_status.get("uncertain").copied(), Some(1));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn confirm_uncertain_memory_provenance_updates_status() -> Result<()> {
    let (store, root) = make_test_store().await?;
    store.init_schema().await?;
    let old_keys = vec!["editor".to_string()];
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    store
        .record_memory_provenance(&MemoryProvenanceRecord {
            id: "old-confirmable",
            target: "MEMORY.md",
            mode: "append",
            source_kind: "goal_reflection",
            content: "- editor: helix",
            fact_keys: &old_keys,
            thread_id: None,
            task_id: None,
            goal_run_id: None,
            created_at: now_ms.saturating_sub(40 * 86_400_000),
            sign: true,
        })
        .await?;

    let before = store.memory_provenance_report(None, 10).await?;
    assert_eq!(before.entries[0].status, "uncertain");
    assert!(before.entries[0].confidence < 0.55);

    let updated = store
        .confirm_memory_provenance_entry("old-confirmable", now_ms)
        .await?;
    assert!(updated, "expected confirmation update to touch one row");

    let after = store.memory_provenance_report(None, 10).await?;
    assert_eq!(after.entries[0].status, "confirmed");
    assert!(after.entries[0].confidence >= 0.95);
    assert_eq!(after.summary_by_status.get("confirmed").copied(), Some(1));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn retract_memory_provenance_entry_updates_status() -> Result<()> {
    let (store, root) = make_test_store().await?;
    store.init_schema().await?;
    let fact_keys = vec!["editor".to_string()];
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    store
        .record_memory_provenance(&MemoryProvenanceRecord {
            id: "retractable-memory-entry",
            target: "MEMORY.md",
            mode: "append",
            source_kind: "goal_reflection",
            content: "- editor: helix",
            fact_keys: &fact_keys,
            thread_id: None,
            task_id: None,
            goal_run_id: None,
            created_at: now_ms,
            sign: true,
        })
        .await?;

    let before = store.memory_provenance_report(None, 10).await?;
    assert_eq!(before.entries[0].status, "active");

    let updated = store
        .retract_memory_provenance_entry("retractable-memory-entry", now_ms)
        .await?;
    assert!(updated, "expected retract update to touch one row");

    let after = store.memory_provenance_report(None, 10).await?;
    assert_eq!(after.entries[0].status, "retracted");
    assert_eq!(after.summary_by_status.get("retracted").copied(), Some(1));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn remove_memory_provenance_records_retract_relationship() -> Result<()> {
    let (store, root) = make_test_store().await?;
    store.init_schema().await?;
    let fact_keys = vec!["editor".to_string()];
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    store
        .record_memory_provenance(&MemoryProvenanceRecord {
            id: "memory-editor-active",
            target: "MEMORY.md",
            mode: "append",
            source_kind: "goal_reflection",
            content: "- editor: helix",
            fact_keys: &fact_keys,
            thread_id: None,
            task_id: None,
            goal_run_id: None,
            created_at: now_ms.saturating_sub(1000),
            sign: true,
        })
        .await?;

    store
        .record_memory_provenance(&MemoryProvenanceRecord {
            id: "memory-editor-remove",
            target: "MEMORY.md",
            mode: "remove",
            source_kind: "operator_correction",
            content: "- editor: helix",
            fact_keys: &fact_keys,
            thread_id: None,
            task_id: None,
            goal_run_id: None,
            created_at: now_ms,
            sign: true,
        })
        .await?;

    let report = store
        .memory_provenance_report(Some("MEMORY.md"), 10)
        .await?;
    let remove_entry = report
        .entries
        .iter()
        .find(|entry| entry.id == "memory-editor-remove")
        .expect("remove provenance entry should exist");

    assert_eq!(remove_entry.relationships.len(), 1);
    assert_eq!(remove_entry.relationships[0].relation_type, "retracts");
    assert_eq!(
        remove_entry.relationships[0].related_entry_id,
        "memory-editor-active"
    );
    assert_eq!(
        remove_entry.relationships[0].fact_key.as_deref(),
        Some("editor")
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn confirm_memory_provenance_entry_rejects_tampered_signed_record() -> Result<()> {
    let (store, root) = make_test_store().await?;
    store.init_schema().await?;
    let fact_keys = vec!["editor".to_string()];
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;

    store
        .record_memory_provenance(&MemoryProvenanceRecord {
            id: "tampered-memory-entry",
            target: "MEMORY.md",
            mode: "append",
            source_kind: "goal_reflection",
            content: "- editor: helix",
            fact_keys: &fact_keys,
            thread_id: None,
            task_id: None,
            goal_run_id: None,
            created_at: now_ms,
            sign: true,
        })
        .await?;

    store.conn.call(|conn| {
        conn.execute(
            "UPDATE memory_provenance SET entry_hash = 'tampered' WHERE id = 'tampered-memory-entry'",
            [],
        )?;
        Ok(())
    }).await.map_err(|e| anyhow::anyhow!("{e}"))?;

    let error = store
        .confirm_memory_provenance_entry("tampered-memory-entry", now_ms.saturating_add(1))
        .await
        .expect_err("tampered signed record should be rejected");
    assert!(error.to_string().contains("integrity validation"));

    let report = store
        .memory_provenance_report(Some("MEMORY.md"), 10)
        .await?;
    let entry = report
        .entries
        .iter()
        .find(|entry| entry.id == "tampered-memory-entry")
        .expect("tampered memory entry should be visible in report");
    assert_eq!(entry.status, "invalid");
    assert!(!entry.hash_valid);
    assert!(!entry.signature_valid);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn collaboration_session_round_trips() -> Result<()> {
    let (store, root) = make_test_store().await?;
    store.init_schema().await?;
    store
        .upsert_collaboration_session(
            "task-parent",
            r#"{"id":"c1","parent_task_id":"task-parent"}"#,
            42,
        )
        .await?;

    let rows = store.list_collaboration_sessions().await?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].parent_task_id, "task-parent");
    assert_eq!(rows[0].updated_at, 42);
    assert!(rows[0].session_json.contains("\"id\":\"c1\""));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn provenance_report_validates_hash_and_signature() -> Result<()> {
    let (store, root) = make_test_store().await?;
    store.init_schema().await?;
    let first = serde_json::json!({"step": 1});
    let second = serde_json::json!({"step": 2});
    store
        .record_provenance_event(&ProvenanceEventRecord {
            event_type: "goal_created",
            summary: "goal created",
            details: &first,
            agent_id: "test-agent",
            goal_run_id: Some("goal-1"),
            task_id: None,
            thread_id: Some("thread-1"),
            approval_id: None,
            causal_trace_id: None,
            compliance_mode: "soc2",
            sign: true,
            created_at: 1_000,
        })
        .await?;
    store
        .record_provenance_event(&ProvenanceEventRecord {
            event_type: "step_completed",
            summary: "step completed",
            details: &second,
            agent_id: "test-agent",
            goal_run_id: Some("goal-1"),
            task_id: Some("task-1"),
            thread_id: Some("thread-1"),
            approval_id: None,
            causal_trace_id: None,
            compliance_mode: "soc2",
            sign: true,
            created_at: 2_000,
        })
        .await?;

    let entries = read_provenance_entries(&root.join("semantic-logs").join("provenance.jsonl"))?;
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].signature_scheme.as_deref(), Some("ed25519"));
    assert_eq!(entries[1].signature_scheme.as_deref(), Some("ed25519"));

    let report = store.provenance_report(10)?;
    assert_eq!(report.total_entries, 2);
    assert_eq!(report.signed_entries, 2);
    assert_eq!(report.valid_hash_entries, 2);
    assert_eq!(report.valid_chain_entries, 2);
    assert_eq!(report.valid_signature_entries, 2);
    assert!(report.entries.iter().all(|entry| entry.signature_present));
    assert!(report.entries.iter().all(|entry| entry.signature_valid));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn provenance_report_keeps_legacy_signature_validation() -> Result<()> {
    let (store, root) = make_test_store().await?;
    store.init_schema().await?;
    let legacy_key_path = root.join("provenance-signing.key");
    let legacy_key = "legacy-signing-key";
    fs::write(&legacy_key_path, legacy_key)?;

    let details = serde_json::json!({"legacy": true});
    let entry_hash = compute_provenance_hash(
        0,
        1_000,
        "legacy_event",
        "legacy provenance event",
        &details,
        "genesis",
        "legacy-agent",
        None,
        None,
        Some("thread-legacy"),
        None,
        None,
        "soc2",
    );
    let entry = ProvenanceLogEntry {
        sequence: 0,
        timestamp: 1_000,
        event_type: "legacy_event".to_string(),
        summary: "legacy provenance event".to_string(),
        details,
        prev_hash: "genesis".to_string(),
        entry_hash: entry_hash.clone(),
        signature: Some(sign_provenance_hash(legacy_key, &entry_hash)),
        signature_scheme: None,
        agent_id: "legacy-agent".to_string(),
        goal_run_id: None,
        task_id: None,
        thread_id: Some("thread-legacy".to_string()),
        approval_id: None,
        causal_trace_id: None,
        compliance_mode: "soc2".to_string(),
    };
    fs::write(
        root.join("semantic-logs").join("provenance.jsonl"),
        format!("{}\n", serde_json::to_string(&entry)?),
    )?;

    let report = store.provenance_report(10)?;
    assert_eq!(report.total_entries, 1);
    assert_eq!(report.signed_entries, 1);
    assert_eq!(report.valid_signature_entries, 1);
    assert_eq!(report.entries.len(), 1);
    assert!(report.entries[0].signature_present);
    assert!(report.entries[0].signature_valid);

    fs::remove_dir_all(root)?;
    Ok(())
}

#[tokio::test]
async fn provenance_report_does_not_mark_unsigned_entries_as_signature_valid() -> Result<()> {
    let (store, root) = make_test_store().await?;
    store.init_schema().await?;

    let details = serde_json::json!({"unsigned": true});
    store
        .record_provenance_event(&ProvenanceEventRecord {
            event_type: "step_completed",
            summary: "unsigned step completed",
            details: &details,
            agent_id: "test-agent",
            goal_run_id: Some("goal-unsigned"),
            task_id: Some("task-unsigned"),
            thread_id: Some("thread-unsigned"),
            approval_id: None,
            causal_trace_id: None,
            compliance_mode: "standard",
            sign: false,
            created_at: 3_000,
        })
        .await?;

    let entries = read_provenance_entries(&root.join("semantic-logs").join("provenance.jsonl"))?;
    assert_eq!(entries.len(), 1);
    assert!(entries[0].signature.is_none());
    assert!(entries[0].signature_scheme.is_none());

    let report = store.provenance_report(10)?;
    assert_eq!(report.total_entries, 1);
    assert_eq!(report.signed_entries, 0);
    assert_eq!(report.valid_hash_entries, 1);
    assert_eq!(report.valid_chain_entries, 1);
    assert_eq!(report.valid_signature_entries, 0);
    assert_eq!(report.entries.len(), 1);
    assert!(!report.entries[0].signature_present);
    assert!(!report.entries[0].signature_valid);

    fs::remove_dir_all(root)?;
    Ok(())
}
