//! Checkpoint save/load/list/prune — persistent snapshots for goal run recovery.

use anyhow::{bail, Context};
use uuid::Uuid;

use super::state_layers::*;
use crate::agent::types::*;

/// Create a [`CheckpointData`] snapshot from the current goal-run state.
///
/// Copies all four state layers (goal, execution, context, runtime) into a
/// single serialisable structure with a fresh UUID-based id.
pub fn checkpoint_save(
    checkpoint_type: CheckpointType,
    goal_run: &GoalRun,
    tasks: &[AgentTask],
    thread_id: Option<&str>,
    context_summary: Option<String>,
    context_tokens: Option<u32>,
    work_context: Option<&ThreadWorkContext>,
    todos: &[TodoItem],
    now: u64,
) -> CheckpointData {
    let id = format!("cp_{}", Uuid::new_v4());
    let mut cp = CheckpointData::new(
        id,
        goal_run.id.clone(),
        checkpoint_type,
        goal_run.clone(),
        now,
    );

    // Layer 2: Execution State
    cp.tasks_snapshot = tasks.to_vec();

    // Layer 3: Context State
    cp.thread_id = thread_id.map(String::from);
    cp.context_summary = context_summary;
    cp.context_tokens = context_tokens;

    // Layer 4: Runtime State
    cp.work_context = work_context.cloned();
    cp.todos = todos.to_vec();
    cp.memory_updates = goal_run.memory_updates.clone();

    cp
}

/// Serialise a [`CheckpointData`] to JSON and persist it via the history store.
///
/// The checkpoint is written to the `agent_checkpoints` table.  The table is
/// created on first use if it does not already exist.
pub async fn checkpoint_store(
    history: &crate::history::HistoryStore,
    checkpoint: &CheckpointData,
) -> anyhow::Result<()> {
    let state_json =
        serde_json::to_string(checkpoint).context("failed to serialise checkpoint to JSON")?;

    history.upsert_checkpoint(
        &checkpoint.id,
        &checkpoint.goal_run_id,
        checkpoint.thread_id.as_deref(),
        // Derive task_id from the goal run's active task, if any.
        checkpoint.goal_run.active_task_id.as_deref(),
        checkpoint.checkpoint_type,
        &state_json,
        checkpoint.context_summary.as_deref(),
        checkpoint.created_at,
    ).await
}

/// Deserialise a [`CheckpointData`] from JSON, validating the schema version.
pub fn checkpoint_load(state_json: &str) -> anyhow::Result<CheckpointData> {
    let data: CheckpointData =
        serde_json::from_str(state_json).context("failed to deserialise checkpoint JSON")?;

    if data.version != CHECKPOINT_SCHEMA_VERSION {
        bail!(
            "unsupported checkpoint schema version {} (expected {})",
            data.version,
            CHECKPOINT_SCHEMA_VERSION
        );
    }

    Ok(data)
}

/// Parse multiple checkpoint JSON strings into [`CheckpointSummary`] values,
/// sorted by `created_at` descending (newest first).
///
/// Unparseable entries are silently skipped so that a single corrupt row does
/// not prevent listing the rest.
pub fn checkpoint_list(checkpoints_json: &[String]) -> Vec<CheckpointSummary> {
    let mut summaries: Vec<CheckpointSummary> = checkpoints_json
        .iter()
        .filter_map(|json| {
            serde_json::from_str::<CheckpointData>(json)
                .ok()
                .map(|cp| cp.to_summary())
        })
        .collect();

    summaries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    summaries
}

/// Keep only the `keep_last_n` most recent checkpoints, removing older ones.
///
/// Checkpoints are sorted by `created_at` ascending (oldest first) before
/// truncation so that the newest `keep_last_n` entries survive.
pub fn checkpoint_prune(checkpoints: &mut Vec<CheckpointData>, keep_last_n: usize) {
    if checkpoints.len() <= keep_last_n {
        return;
    }

    // Sort oldest-first, then keep only the tail.
    checkpoints.sort_by_key(|cp| cp.created_at);
    let start = checkpoints.len() - keep_last_n;
    *checkpoints = checkpoints.split_off(start);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    // -- helpers --

    fn sample_goal_run() -> GoalRun {
        GoalRun {
            id: "goal_1".into(),
            title: "Test goal".into(),
            goal: "Do stuff".into(),
            client_request_id: None,
            status: GoalRunStatus::Running,
            priority: TaskPriority::Normal,
            created_at: 100,
            updated_at: 200,
            started_at: Some(110),
            completed_at: None,
            thread_id: Some("thread_1".into()),
            session_id: None,
            current_step_index: 2,
            current_step_title: Some("Step 3".into()),
            current_step_kind: None,
            replan_count: 0,
            max_replans: 2,
            plan_summary: Some("Plan summary".into()),
            reflection_summary: None,
            memory_updates: vec!["learned X".into(), "noted Y".into()],
            generated_skill_path: None,
            last_error: None,
            failure_cause: None,
            child_task_ids: vec!["task_1".into()],
            child_task_count: 1,
            approval_count: 0,
            awaiting_approval_id: None,
            active_task_id: Some("task_1".into()),
            duration_ms: None,
            steps: vec![],
            events: vec![],
        }
    }

    fn sample_task(id: &str) -> AgentTask {
        AgentTask {
            id: id.into(),
            title: format!("Task {}", id),
            description: "A test task".into(),
            status: TaskStatus::InProgress,
            priority: TaskPriority::Normal,
            progress: 50,
            created_at: 100,
            started_at: Some(110),
            completed_at: None,
            error: None,
            result: None,
            thread_id: Some("thread_1".into()),
            source: "user".into(),
            notify_on_complete: false,
            notify_channels: vec![],
            dependencies: vec![],
            command: None,
            session_id: None,
            goal_run_id: Some("goal_1".into()),
            goal_run_title: None,
            goal_step_id: None,
            goal_step_title: None,
            parent_task_id: None,
            parent_thread_id: None,
            runtime: "daemon".into(),
            retry_count: 0,
            max_retries: 3,
            next_retry_at: None,
            scheduled_at: None,
            blocked_reason: None,
            awaiting_approval_id: None,
            lane_id: None,
            last_error: None,
            logs: vec![],
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

    fn sample_work_context() -> ThreadWorkContext {
        ThreadWorkContext {
            thread_id: "thread_1".into(),
            entries: vec![WorkContextEntry {
                path: "/tmp/test.rs".into(),
                previous_path: None,
                kind: WorkContextEntryKind::Artifact,
                source: "agent".into(),
                change_kind: Some("created".into()),
                repo_root: None,
                goal_run_id: Some("goal_1".into()),
                step_index: Some(1),
                session_id: None,
                is_text: true,
                updated_at: 200,
            }],
        }
    }

    fn sample_todos() -> Vec<TodoItem> {
        vec![
            TodoItem {
                id: "todo_1".into(),
                content: "Fix the bug".into(),
                status: TodoStatus::Pending,
                position: 0,
                step_index: Some(1),
                created_at: 100,
                updated_at: 100,
            },
            TodoItem {
                id: "todo_2".into(),
                content: "Write tests".into(),
                status: TodoStatus::InProgress,
                position: 1,
                step_index: Some(2),
                created_at: 110,
                updated_at: 120,
            },
        ]
    }

    // -- checkpoint_save tests --

    #[test]
    fn save_creates_valid_checkpoint_with_all_layers() {
        let gr = sample_goal_run();
        let tasks = vec![sample_task("t1"), sample_task("t2")];
        let wc = sample_work_context();
        let todos = sample_todos();

        let cp = checkpoint_save(
            CheckpointType::PostStep,
            &gr,
            &tasks,
            Some("thread_1"),
            Some("context summary".into()),
            Some(4096),
            Some(&wc),
            &todos,
            5000,
        );

        // Layer 1: Goal
        assert_eq!(cp.goal_run.id, "goal_1");
        assert_eq!(cp.goal_run.status, GoalRunStatus::Running);

        // Layer 2: Execution
        assert_eq!(cp.tasks_snapshot.len(), 2);
        assert_eq!(cp.tasks_snapshot[0].id, "t1");

        // Layer 3: Context
        assert_eq!(cp.thread_id.as_deref(), Some("thread_1"));
        assert_eq!(cp.context_summary.as_deref(), Some("context summary"));
        assert_eq!(cp.context_tokens, Some(4096));

        // Layer 4: Runtime
        assert!(cp.work_context.is_some());
        assert_eq!(cp.todos.len(), 2);
        assert_eq!(cp.memory_updates.len(), 2);

        // Metadata
        assert_eq!(cp.created_at, 5000);
        assert_eq!(cp.version, CHECKPOINT_SCHEMA_VERSION);
        assert_eq!(cp.checkpoint_type, CheckpointType::PostStep);
    }

    #[test]
    fn save_assigns_unique_id() {
        let gr = sample_goal_run();
        let cp1 = checkpoint_save(
            CheckpointType::PreStep,
            &gr,
            &[],
            None,
            None,
            None,
            None,
            &[],
            1000,
        );
        let cp2 = checkpoint_save(
            CheckpointType::PreStep,
            &gr,
            &[],
            None,
            None,
            None,
            None,
            &[],
            1001,
        );

        assert_ne!(cp1.id, cp2.id);
        assert!(cp1.id.starts_with("cp_"));
        assert!(cp2.id.starts_with("cp_"));
    }

    // -- checkpoint_load tests --

    #[test]
    fn load_roundtrip_save_serialize_load() {
        let gr = sample_goal_run();
        let tasks = vec![sample_task("t1")];
        let wc = sample_work_context();
        let todos = sample_todos();

        let original = checkpoint_save(
            CheckpointType::Manual,
            &gr,
            &tasks,
            Some("thread_1"),
            Some("summary text".into()),
            Some(2048),
            Some(&wc),
            &todos,
            9999,
        );

        let json = serde_json::to_string(&original).unwrap();
        let restored = checkpoint_load(&json).unwrap();

        assert_eq!(restored.id, original.id);
        assert_eq!(restored.goal_run_id, "goal_1");
        assert_eq!(restored.goal_run.current_step_index, 2);
        assert_eq!(restored.tasks_snapshot.len(), 1);
        assert_eq!(restored.tasks_snapshot[0].id, "t1");
        assert_eq!(restored.context_summary.as_deref(), Some("summary text"));
        assert_eq!(restored.context_tokens, Some(2048));
        assert_eq!(restored.todos.len(), 2);
        assert_eq!(restored.memory_updates.len(), 2);
        assert!(restored.work_context.is_some());
        assert_eq!(restored.created_at, 9999);
    }

    #[test]
    fn load_rejects_wrong_schema_version() {
        let mut cp = CheckpointData::new(
            "cp_test".into(),
            "goal_1".into(),
            CheckpointType::PreStep,
            sample_goal_run(),
            1000,
        );
        cp.version = 999;

        let json = serde_json::to_string(&cp).unwrap();
        let result = checkpoint_load(&json);

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("unsupported checkpoint schema version 999"));
    }

    // -- checkpoint_list tests --

    #[test]
    fn list_returns_sorted_summaries() {
        let gr = sample_goal_run();

        let cp1 = checkpoint_save(
            CheckpointType::PreStep,
            &gr,
            &[],
            None,
            None,
            None,
            None,
            &[],
            1000,
        );
        let cp2 = checkpoint_save(
            CheckpointType::PostStep,
            &gr,
            &[],
            None,
            None,
            None,
            None,
            &[],
            3000,
        );
        let cp3 = checkpoint_save(
            CheckpointType::Manual,
            &gr,
            &[],
            None,
            None,
            None,
            None,
            &[],
            2000,
        );

        let jsons: Vec<String> = vec![
            serde_json::to_string(&cp1).unwrap(),
            serde_json::to_string(&cp2).unwrap(),
            serde_json::to_string(&cp3).unwrap(),
        ];

        let summaries = checkpoint_list(&jsons);

        assert_eq!(summaries.len(), 3);
        // Newest first
        assert_eq!(summaries[0].created_at, 3000);
        assert_eq!(summaries[1].created_at, 2000);
        assert_eq!(summaries[2].created_at, 1000);
    }

    // -- checkpoint_prune tests --

    #[test]
    fn prune_keeps_n_most_recent() {
        let gr = sample_goal_run();
        let mut cps: Vec<CheckpointData> = (0..5)
            .map(|i| {
                checkpoint_save(
                    CheckpointType::Periodic,
                    &gr,
                    &[],
                    None,
                    None,
                    None,
                    None,
                    &[],
                    1000 + i * 100,
                )
            })
            .collect();

        checkpoint_prune(&mut cps, 2);

        assert_eq!(cps.len(), 2);
        // Should keep the two newest (created_at 1300 and 1400)
        assert_eq!(cps[0].created_at, 1300);
        assert_eq!(cps[1].created_at, 1400);
    }

    #[test]
    fn prune_with_empty_vec() {
        let mut cps: Vec<CheckpointData> = Vec::new();
        checkpoint_prune(&mut cps, 5);
        assert!(cps.is_empty());
    }

    #[test]
    fn prune_with_n_greater_than_len_does_nothing() {
        let gr = sample_goal_run();
        let mut cps: Vec<CheckpointData> = (0..3)
            .map(|i| {
                checkpoint_save(
                    CheckpointType::PreStep,
                    &gr,
                    &[],
                    None,
                    None,
                    None,
                    None,
                    &[],
                    1000 + i * 100,
                )
            })
            .collect();

        checkpoint_prune(&mut cps, 10);
        assert_eq!(cps.len(), 3);
    }

    // -- summary tests --

    #[test]
    fn checkpoint_summary_includes_step_index() {
        let mut gr = sample_goal_run();
        gr.current_step_index = 7;

        let cp = checkpoint_save(
            CheckpointType::PostStep,
            &gr,
            &[],
            None,
            None,
            None,
            None,
            &[],
            5000,
        );
        let summary = cp.to_summary();

        assert_eq!(summary.step_index, Some(7));
    }

    #[test]
    fn context_summary_truncation_in_summary() {
        let gr = sample_goal_run();
        let long_summary = "a".repeat(250);

        let cp = checkpoint_save(
            CheckpointType::PreStep,
            &gr,
            &[],
            None,
            Some(long_summary),
            None,
            None,
            &[],
            6000,
        );
        let summary = cp.to_summary();

        let preview = summary.context_summary_preview.unwrap();
        // Truncated to 119 chars + ellipsis character
        assert!(preview.len() <= 123); // 119 ASCII bytes + up to 4 bytes for the ellipsis
        assert!(preview.ends_with('\u{2026}'));
    }

    // -- same goal run, multiple checkpoints --

    #[test]
    fn multiple_checkpoints_for_same_goal_run() {
        let gr = sample_goal_run();

        let cp_pre = checkpoint_save(
            CheckpointType::PreStep,
            &gr,
            &[],
            None,
            None,
            None,
            None,
            &[],
            1000,
        );
        let cp_post = checkpoint_save(
            CheckpointType::PostStep,
            &gr,
            &[],
            None,
            None,
            None,
            None,
            &[],
            2000,
        );

        assert_eq!(cp_pre.goal_run_id, cp_post.goal_run_id);
        assert_ne!(cp_pre.id, cp_post.id);
        assert_eq!(cp_pre.goal_run_id, "goal_1");
    }

    // -- checkpoint type variants --

    #[test]
    fn pre_step_vs_post_step_vs_manual_types() {
        let gr = sample_goal_run();

        let pre = checkpoint_save(
            CheckpointType::PreStep,
            &gr,
            &[],
            None,
            None,
            None,
            None,
            &[],
            1000,
        );
        let post = checkpoint_save(
            CheckpointType::PostStep,
            &gr,
            &[],
            None,
            None,
            None,
            None,
            &[],
            2000,
        );
        let manual = checkpoint_save(
            CheckpointType::Manual,
            &gr,
            &[],
            None,
            None,
            None,
            None,
            &[],
            3000,
        );

        assert_eq!(pre.checkpoint_type, CheckpointType::PreStep);
        assert_eq!(post.checkpoint_type, CheckpointType::PostStep);
        assert_eq!(manual.checkpoint_type, CheckpointType::Manual);

        // Verify they serialise to distinct JSON values
        let pre_json = serde_json::to_value(&pre.checkpoint_type).unwrap();
        let post_json = serde_json::to_value(&post.checkpoint_type).unwrap();
        let manual_json = serde_json::to_value(&manual.checkpoint_type).unwrap();
        assert_eq!(pre_json, "pre_step");
        assert_eq!(post_json, "post_step");
        assert_eq!(manual_json, "manual");
    }

    // -- layer preservation tests --

    #[test]
    fn tasks_snapshot_preserved() {
        let gr = sample_goal_run();
        let tasks = vec![sample_task("t_alpha"), sample_task("t_beta")];

        let cp = checkpoint_save(
            CheckpointType::PostStep,
            &gr,
            &tasks,
            None,
            None,
            None,
            None,
            &[],
            7000,
        );

        assert_eq!(cp.tasks_snapshot.len(), 2);
        assert_eq!(cp.tasks_snapshot[0].id, "t_alpha");
        assert_eq!(cp.tasks_snapshot[0].title, "Task t_alpha");
        assert_eq!(cp.tasks_snapshot[1].id, "t_beta");
        assert_eq!(cp.tasks_snapshot[1].status, TaskStatus::InProgress);

        // Verify survives serialisation round-trip
        let json = serde_json::to_string(&cp).unwrap();
        let restored = checkpoint_load(&json).unwrap();
        assert_eq!(restored.tasks_snapshot.len(), 2);
        assert_eq!(restored.tasks_snapshot[0].id, "t_alpha");
    }

    #[test]
    fn work_context_preserved() {
        let gr = sample_goal_run();
        let wc = sample_work_context();

        let cp = checkpoint_save(
            CheckpointType::Manual,
            &gr,
            &[],
            None,
            None,
            None,
            Some(&wc),
            &[],
            8000,
        );

        let restored_wc = cp.work_context.as_ref().unwrap();
        assert_eq!(restored_wc.thread_id, "thread_1");
        assert_eq!(restored_wc.entries.len(), 1);
        assert_eq!(restored_wc.entries[0].path, "/tmp/test.rs");
        assert_eq!(restored_wc.entries[0].kind, WorkContextEntryKind::Artifact);

        // Verify survives serialisation round-trip
        let json = serde_json::to_string(&cp).unwrap();
        let restored = checkpoint_load(&json).unwrap();
        let wc2 = restored.work_context.as_ref().unwrap();
        assert_eq!(wc2.entries[0].path, "/tmp/test.rs");
    }

    #[test]
    fn memory_updates_preserved() {
        let gr = sample_goal_run();

        let cp = checkpoint_save(
            CheckpointType::PostStep,
            &gr,
            &[],
            None,
            None,
            None,
            None,
            &[],
            9000,
        );

        assert_eq!(cp.memory_updates.len(), 2);
        assert_eq!(cp.memory_updates[0], "learned X");
        assert_eq!(cp.memory_updates[1], "noted Y");

        // Round-trip
        let json = serde_json::to_string(&cp).unwrap();
        let restored = checkpoint_load(&json).unwrap();
        assert_eq!(restored.memory_updates, vec!["learned X", "noted Y"]);
    }

    // -- edge cases --

    #[test]
    fn save_with_no_optional_fields() {
        let mut gr = sample_goal_run();
        gr.memory_updates.clear();

        let cp = checkpoint_save(
            CheckpointType::PreStep,
            &gr,
            &[],
            None,
            None,
            None,
            None,
            &[],
            500,
        );

        assert!(cp.thread_id.is_none());
        assert!(cp.context_summary.is_none());
        assert!(cp.context_tokens.is_none());
        assert!(cp.work_context.is_none());
        assert!(cp.todos.is_empty());
        assert!(cp.memory_updates.is_empty());
        assert!(cp.note.is_none());
    }

    #[test]
    fn load_rejects_invalid_json() {
        let result = checkpoint_load("not valid json {{{");
        assert!(result.is_err());
    }

    #[test]
    fn list_skips_invalid_entries() {
        let gr = sample_goal_run();
        let cp = checkpoint_save(
            CheckpointType::PreStep,
            &gr,
            &[],
            None,
            None,
            None,
            None,
            &[],
            1000,
        );

        let jsons = vec![
            "invalid json".into(),
            serde_json::to_string(&cp).unwrap(),
            "also invalid".into(),
        ];

        let summaries = checkpoint_list(&jsons);
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].created_at, 1000);
    }
}
