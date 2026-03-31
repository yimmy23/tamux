use super::*;

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
