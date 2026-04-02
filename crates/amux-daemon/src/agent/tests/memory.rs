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
    let err = validate_memory_size(MemoryTarget::Soul, &"x".repeat(1_501)).unwrap_err();
    assert!(err.to_string().contains("SOUL.md would exceed its limit"));
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
