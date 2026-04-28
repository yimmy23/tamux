use super::*;

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
