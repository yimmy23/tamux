use super::super::operator_profile::user_sync::{
    acquire_user_sync_test_guard, set_user_sync_state_for_test,
};
use super::*;
use crate::history::HistoryStore;

fn test_write_context() -> MemoryWriteContext<'static> {
    MemoryWriteContext {
        source_kind: "test",
        thread_id: None,
        task_id: None,
        goal_run_id: None,
    }
}

#[test]
fn validate_memory_size_rejects_over_limit() {
    let err = validate_memory_size(
        MemoryTarget::Soul,
        &"x".repeat(MemoryTarget::Soul.limit_chars() + 1),
    )
    .unwrap_err();
    assert!(err.to_string().contains("SOUL.md would exceed its limit"));
}

#[test]
fn memory_target_limits_match_policy_contract() {
    assert_eq!(MemoryTarget::Soul.limit_chars(), 2_000);
    assert_eq!(MemoryTarget::Memory.limit_chars(), 3_600);
    assert_eq!(MemoryTarget::User.limit_chars(), 1_800);
}

#[test]
fn append_content_separates_blocks() {
    assert_eq!(append_content("alpha", "beta"), "alpha\n\nbeta");
}

#[test]
fn remove_content_requires_exact_match() {
    let err = remove_content("alpha", "beta").unwrap_err();
    assert!(err.to_string().contains("not found"));
}

#[test]
fn extract_fact_candidates_uses_subject_before_colon() {
    let facts = extract_memory_fact_candidates("- Shell: bash\n- editor: helix");
    assert_eq!(facts.len(), 2);
    assert_eq!(facts[0].key, "editor");
    assert_eq!(facts[1].key, "shell");
}

#[test]
fn extract_fact_candidates_ignore_distilled_prefix_tags() {
    let facts = extract_memory_fact_candidates(
        "- [distilled] Shell: bash\n- [Discord — mariuszkurman] editor: helix",
    );
    assert_eq!(facts.len(), 2);
    assert_eq!(facts[0].key, "editor");
    assert_eq!(facts[0].display, "editor: helix");
    assert_eq!(facts[1].key, "shell");
    assert_eq!(facts[1].display, "Shell: bash");
}

#[test]
fn contradiction_detection_blocks_conflicting_subject_fact() {
    let err =
        validate_no_memory_contradictions(MemoryTarget::User, "- shell: bash", "- shell: zsh")
            .unwrap_err();
    assert!(err.to_string().contains("Potential contradiction detected"));
}

#[test]
fn contradiction_detection_allows_matching_fact() {
    validate_no_memory_contradictions(
        MemoryTarget::Memory,
        "- package manager: cargo",
        "- package manager: cargo",
    )
    .expect("identical facts should not conflict");
}

#[test]
fn detect_memory_contradictions_returns_pairs_for_conflicting_facts() {
    let contradictions = detect_memory_contradictions("- shell: bash", "- shell: zsh");
    assert_eq!(contradictions.len(), 1);
    assert_eq!(contradictions[0].0.key, "shell");
    assert_eq!(contradictions[0].0.display, "shell: bash");
    assert_eq!(contradictions[0].1.display, "shell: zsh");
}

#[tokio::test]
async fn conflicting_memory_update_records_conflict_provenance_relationship() -> Result<()> {
    let root = std::env::temp_dir().join(format!("tamux-memory-test-{}", Uuid::new_v4()));
    let history = HistoryStore::new_test_store(&root).await?;
    ensure_memory_files(&root).await?;
    let memory_path = active_memory_dir(&root).join(MemoryTarget::Memory.file_name());
    tokio::fs::write(&memory_path, "# Memory\n- shell: bash\n").await?;
    let baseline_fact_keys = vec!["shell".to_string()];
    history
        .record_memory_provenance(&crate::history::MemoryProvenanceRecord {
            id: "baseline-shell",
            target: "MEMORY.md",
            mode: "append",
            source_kind: "test",
            content: "- shell: bash",
            fact_keys: &baseline_fact_keys,
            thread_id: None,
            task_id: None,
            goal_run_id: None,
            created_at: 1,
        })
        .await?;

    let error = apply_memory_update(
        &root,
        &history,
        MemoryTarget::Memory,
        MemoryUpdateMode::Append,
        "- shell: zsh",
        test_write_context(),
    )
    .await
    .expect_err("conflicting append should be rejected");
    assert!(error
        .to_string()
        .contains("Potential contradiction detected"));

    let report = history
        .memory_provenance_report(Some("MEMORY.md"), 10)
        .await?;
    let conflict_entry = report
        .entries
        .iter()
        .find(|entry| entry.mode == "conflict")
        .expect("conflict provenance entry should exist");
    assert_eq!(conflict_entry.status, "contradicted");
    assert_eq!(conflict_entry.fact_keys, vec!["shell".to_string()]);
    assert_eq!(conflict_entry.relationships.len(), 1);
    assert_eq!(conflict_entry.relationships[0].relation_type, "contradicts");
    assert_eq!(
        conflict_entry.relationships[0].related_entry_id,
        "baseline-shell"
    );

    Ok(())
}

#[tokio::test]
async fn user_append_while_reconciling_stages_without_conflicting_file_write() -> Result<()> {
    let _guard = acquire_user_sync_test_guard();
    let root = std::env::temp_dir().join(format!("tamux-memory-test-{}", Uuid::new_v4()));
    let history = HistoryStore::new_test_store(&root).await?;
    ensure_memory_files(&root).await?;
    let user_path = active_memory_dir(&root).join(MemoryTarget::User.file_name());
    tokio::fs::write(&user_path, "# User\n- shell: bash\n").await?;

    set_user_sync_state_for_test(UserProfileSyncState::Reconciling);
    let _ = apply_memory_update(
        &root,
        &history,
        MemoryTarget::User,
        MemoryUpdateMode::Append,
        "- shell: zsh",
        test_write_context(),
    )
    .await?;

    let final_content = tokio::fs::read_to_string(&user_path).await?;
    assert!(final_content.contains("- shell: bash"));
    assert!(!final_content.contains("- shell: zsh"));

    let staged = history
        .get_profile_field("legacy_user_signal")
        .await?
        .expect("legacy append should be staged into profile fields");
    assert_eq!(staged.source, "legacy_append");
    Ok(())
}

#[tokio::test]
async fn user_append_is_staged_then_rerendered_from_db() -> Result<()> {
    let _guard = acquire_user_sync_test_guard();
    let root = std::env::temp_dir().join(format!("tamux-memory-test-{}", Uuid::new_v4()));
    let history = HistoryStore::new_test_store(&root).await?;
    ensure_memory_files(&root).await?;
    let user_path = active_memory_dir(&root).join(MemoryTarget::User.file_name());
    tokio::fs::write(&user_path, "# User\nlegacy note\n").await?;

    set_user_sync_state_for_test(UserProfileSyncState::Clean);
    let _ = apply_memory_update(
        &root,
        &history,
        MemoryTarget::User,
        MemoryUpdateMode::Append,
        "- prefers concise replies",
        test_write_context(),
    )
    .await?;

    let final_content = tokio::fs::read_to_string(&user_path).await?;
    assert!(
        final_content.contains("Profile summary is generated from SQLite-backed operator profile.")
    );
    assert!(final_content.contains("- legacy_user_signal: "));
    assert!(final_content.contains("- legacy_user_md: "));

    let import_done = history
        .get_profile_field("__legacy_user_import_done")
        .await?;
    assert!(
        import_done.is_some(),
        "legacy bootstrap import sentinel should be written"
    );
    Ok(())
}

#[tokio::test]
async fn persona_scope_loads_local_memory_and_shared_user() -> Result<()> {
    let root = std::env::temp_dir().join(format!("tamux-memory-test-{}", Uuid::new_v4()));
    let history = HistoryStore::new_test_store(&root).await?;
    ensure_memory_files_for_scope(&root, MAIN_AGENT_ID).await?;
    ensure_memory_files_for_scope(&root, RADOGOST_AGENT_ID).await?;

    let main_paths = memory_paths_for_scope(&root, MAIN_AGENT_ID);
    let persona_paths = memory_paths_for_scope(&root, RADOGOST_AGENT_ID);

    tokio::fs::write(&main_paths.user_path, "# User\n- prefers detailed audits\n").await?;
    tokio::fs::write(&persona_paths.soul_path, "# Identity\nRadogost persona\n").await?;
    tokio::fs::write(
        &persona_paths.memory_path,
        "# Memory\n- prefers tradeoff tables\n",
    )
    .await?;

    let loaded = load_memory_for_scope(&root, RADOGOST_AGENT_ID).await?;
    assert!(loaded.soul.contains("Radogost persona"));
    assert!(loaded.memory.contains("tradeoff tables"));
    assert!(loaded.user_profile.contains("prefers detailed audits"));

    let _ = history;
    Ok(())
}
