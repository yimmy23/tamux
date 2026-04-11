use super::support::*;

use crate::agent::AlineStartupShortCircuitReason;
use crate::agent::WatcherState;

#[tokio::test]
async fn hydrate_skips_aline_reconciliation_when_cli_is_unavailable() {
    let mut harness = make_aline_startup_harness().await;
    let repo_root = harness.create_git_repo("cli-unavailable-repo");
    harness.persist_repo_roots(&[repo_root.as_str()]).await;

    harness
        .engine
        .hydrate()
        .await
        .expect("hydrate should succeed");
    harness.wait_for_reconciliation().await;

    assert!(harness.recorded_commands().is_empty());
}

#[tokio::test]
async fn hydrate_skips_aline_reconciliation_when_no_repo_root_is_resolved() {
    let mut harness =
        make_aline_startup_harness_with_runner(true, vec![watcher_status_output("Running")]).await;

    harness
        .engine
        .hydrate()
        .await
        .expect("hydrate should succeed");
    harness.wait_for_reconciliation().await;

    assert!(!harness
        .engine
        .aline_startup_reconciliation_started_for_tests());
    assert_eq!(
        harness.recorded_commands(),
        vec![vec!["watcher".to_string(), "status".to_string()]]
    );
    let summary = harness
        .engine
        .aline_startup_last_summary_for_tests()
        .await
        .expect("summary should be captured for no-repo skip");
    assert!(summary.aline_available);
    assert_eq!(
        summary.short_circuit_reason,
        Some(AlineStartupShortCircuitReason::NoRepoRoots)
    );
}

#[tokio::test]
async fn hydrate_starts_watcher_even_when_no_repo_root_is_resolved() {
    let mut harness = make_aline_startup_harness_with_runner(
        true,
        vec![
            watcher_status_output("Stopped"),
            command_output("Watcher started\n"),
        ],
    )
    .await;

    harness
        .engine
        .hydrate()
        .await
        .expect("hydrate should succeed without repo roots");
    harness.wait_for_reconciliation().await;

    assert!(!harness
        .engine
        .aline_startup_reconciliation_started_for_tests());
    assert_eq!(
        harness.recorded_commands(),
        vec![
            vec!["watcher".to_string(), "status".to_string()],
            vec!["watcher".to_string(), "start".to_string()],
        ]
    );
    let summary = harness
        .engine
        .aline_startup_last_summary_for_tests()
        .await
        .expect("summary should be captured for no-repo skip");
    assert_eq!(
        summary.short_circuit_reason,
        Some(AlineStartupShortCircuitReason::NoRepoRoots)
    );
}

#[tokio::test]
async fn hydrate_does_not_wait_for_aline_watcher_bootstrap_when_no_repo_root_is_resolved() {
    let harness = make_aline_startup_harness_with_responses(
        true,
        vec![
            delayed_output(
                watcher_status_output("Stopped"),
                std::time::Duration::from_millis(200),
            ),
            delayed_output(
                command_output("Watcher started\n"),
                std::time::Duration::from_millis(200),
            ),
        ],
    )
    .await;

    tokio::time::timeout(
        std::time::Duration::from_millis(50),
        harness.engine.hydrate(),
    )
    .await
    .expect("hydrate should not wait on delayed Aline startup work")
    .expect("hydrate should still succeed while Aline startup continues in the background");
}

#[tokio::test]
async fn hydrate_skips_aline_reconciliation_when_multiple_repo_roots_are_resolved() {
    let mut harness =
        make_aline_startup_harness_with_runner(true, vec![watcher_status_output("Running")]).await;
    let repo_root_a = harness.create_git_repo("multi-root-a");
    let repo_root_b = harness.create_git_repo("multi-root-b");
    harness
        .persist_repo_roots_with_updated_at(&[
            (repo_root_a.as_str(), 10),
            (repo_root_b.as_str(), 10),
        ])
        .await;

    harness
        .engine
        .hydrate()
        .await
        .expect("hydrate should succeed");
    harness.wait_for_reconciliation().await;

    assert!(!harness
        .engine
        .aline_startup_reconciliation_started_for_tests());
    assert_eq!(
        harness.recorded_commands(),
        vec![vec!["watcher".to_string(), "status".to_string()]]
    );
    let summary = harness
        .engine
        .aline_startup_last_summary_for_tests()
        .await
        .expect("summary should be captured for multi-repo skip");
    assert!(summary.aline_available);
    assert_eq!(
        summary.short_circuit_reason,
        Some(AlineStartupShortCircuitReason::MultipleRepoRoots)
    );
}

#[tokio::test]
async fn hydrate_starts_watcher_even_when_multiple_repo_roots_are_resolved() {
    let mut harness = make_aline_startup_harness_with_runner(
        true,
        vec![
            watcher_status_output("Stopped"),
            command_output("Watcher started\n"),
        ],
    )
    .await;
    let repo_root_a = harness.create_git_repo("multi-root-start-a");
    let repo_root_b = harness.create_git_repo("multi-root-start-b");
    harness
        .persist_repo_roots_with_updated_at(&[
            (repo_root_a.as_str(), 10),
            (repo_root_b.as_str(), 10),
        ])
        .await;

    harness
        .engine
        .hydrate()
        .await
        .expect("hydrate should still start the watcher for multi-root state");
    harness.wait_for_reconciliation().await;

    assert_eq!(
        harness.recorded_commands(),
        vec![
            vec!["watcher".to_string(), "status".to_string()],
            vec!["watcher".to_string(), "start".to_string()],
        ]
    );
    let summary = harness
        .engine
        .aline_startup_last_summary_for_tests()
        .await
        .expect("summary should be captured for multi-repo skip");
    assert_eq!(
        summary.short_circuit_reason,
        Some(AlineStartupShortCircuitReason::MultipleRepoRoots)
    );
}

#[tokio::test]
async fn repo_context_refresh_starts_aline_reconciliation_after_no_repo_boot() {
    let mut harness = make_aline_startup_harness_with_runner(
        true,
        vec![
            watcher_status_output("Running"),
            session_list_output(SAMPLE_JSON),
        ],
    )
    .await;

    harness
        .engine
        .hydrate()
        .await
        .expect("hydrate should succeed without repo roots");
    tokio::task::yield_now().await;

    assert!(!harness
        .engine
        .aline_startup_reconciliation_started_for_tests());

    let repo_root = harness.create_git_repo("late-repo-context");
    let tracked_file = std::path::Path::new(&repo_root).join("README.md");
    std::fs::write(&tracked_file, "late repo context\n").expect("write tracked file");

    harness
        .engine
        .record_file_work_context(
            "thread-late-repo-context",
            None,
            "write_file",
            tracked_file.to_str().expect("utf-8 path"),
        )
        .await;
    assert_eq!(
        harness
            .engine
            .resolve_thread_repo_root("thread-late-repo-context")
            .await
            .map(|item| item.0),
        Some(repo_root.clone())
    );
    assert!(harness
        .engine
        .aline_startup_reconciliation_started_for_tests());
    harness.wait_for_reconciliation().await;

    assert!(harness
        .engine
        .aline_startup_reconciliation_started_for_tests());
    assert_eq!(
        harness.recorded_commands(),
        vec![
            vec!["watcher".to_string(), "status".to_string()],
            vec!["watcher".to_string(), "status".to_string()],
            vec![
                "watcher".to_string(),
                "session".to_string(),
                "list".to_string(),
                "--json".to_string(),
                "--page".to_string(),
                "1".to_string(),
                "--per-page".to_string(),
                "30".to_string(),
            ],
        ]
    );
}

#[tokio::test]
async fn hydrate_schedules_aline_reconciliation_for_live_session_only_single_repo() {
    let mut harness = make_aline_startup_harness_with_runner(
        true,
        vec![
            watcher_status_output("Running"),
            session_list_output(SAMPLE_JSON),
        ],
    )
    .await;
    let repo_root = harness.create_git_repo("live-session-only-repo");
    harness.spawn_live_session(&repo_root).await;

    harness
        .engine
        .hydrate()
        .await
        .expect("hydrate should succeed");
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        harness.wait_for_reconciliation(),
    )
    .await
    .expect("reconciliation should complete for a live-session-only repo root");

    assert!(harness
        .engine
        .aline_startup_reconciliation_started_for_tests());
    assert_eq!(harness.recorded_commands().len(), 2);
}

#[tokio::test]
async fn hydrate_starts_watcher_when_status_is_stopped() {
    let mut harness = make_aline_startup_harness_with_runner(
        true,
        vec![
            watcher_status_output("Stopped"),
            command_output("Watcher started\n"),
            session_list_output(SAMPLE_JSON),
        ],
    )
    .await;
    let repo_root = harness.create_git_repo("stopped-watcher-repo");
    harness.persist_repo_roots(&[repo_root.as_str()]).await;

    harness
        .engine
        .hydrate()
        .await
        .expect("hydrate should succeed when watcher starts");
    harness.wait_for_reconciliation().await;

    assert!(harness
        .engine
        .aline_startup_reconciliation_started_for_tests());
    assert_eq!(
        harness.recorded_commands(),
        vec![
            vec!["watcher".to_string(), "status".to_string()],
            vec!["watcher".to_string(), "start".to_string()],
            vec![
                "watcher".to_string(),
                "session".to_string(),
                "list".to_string(),
                "--json".to_string(),
                "--page".to_string(),
                "1".to_string(),
                "--per-page".to_string(),
                "30".to_string(),
            ],
        ]
    );
}

#[tokio::test]
async fn hydrate_prefers_live_repo_over_stale_persisted_repo_roots() {
    let mut harness =
        make_aline_startup_harness_with_runner(true, vec![session_list_output(SAMPLE_JSON)]).await;
    let persisted_repo_root = harness.create_git_repo("persisted-repo-root");
    let live_repo_root = harness.create_git_repo("live-session-repo-root");
    harness
        .persist_repo_roots(&[persisted_repo_root.as_str()])
        .await;
    harness.spawn_live_session(&live_repo_root).await;

    harness
        .engine
        .hydrate()
        .await
        .expect("hydrate should succeed");
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        harness.wait_for_reconciliation(),
    )
    .await
    .expect("reconciliation should complete for the live repo root");

    assert!(harness
        .engine
        .aline_startup_reconciliation_started_for_tests());
    assert_eq!(harness.recorded_commands().len(), 2);
    assert_eq!(harness.scheduled_repo_roots().await, vec![live_repo_root]);
}

#[tokio::test]
async fn hydrate_prefers_most_recent_persisted_repo_root_when_no_live_sessions() {
    let mut harness = make_aline_startup_harness_with_runner(
        true,
        vec![
            watcher_status_output("Running"),
            session_list_output(SAMPLE_JSON),
        ],
    )
    .await;
    let older_repo_root = harness.create_git_repo("older-persisted-repo");
    let newer_repo_root = harness.create_git_repo("newer-persisted-repo");
    harness
        .persist_repo_roots_with_updated_at(&[
            (older_repo_root.as_str(), 10),
            (newer_repo_root.as_str(), 20),
        ])
        .await;

    harness
        .engine
        .hydrate()
        .await
        .expect("hydrate should succeed");
    harness.wait_for_reconciliation().await;

    assert!(harness
        .engine
        .aline_startup_reconciliation_started_for_tests());
    assert_eq!(harness.recorded_commands().len(), 2);
    assert_eq!(harness.scheduled_repo_roots().await, vec![newer_repo_root]);
}

#[tokio::test]
async fn hydrate_canonicalizes_persisted_worktree_root_against_live_session_repo_root() {
    let mut harness = make_aline_startup_harness_with_runner(
        true,
        vec![
            watcher_status_output("Running"),
            session_list_output(SAMPLE_JSON),
        ],
    )
    .await;
    let (main_repo_root, worktree_repo_root) = harness.create_git_worktree_pair("canonical-root");
    harness
        .persist_repo_roots(&[worktree_repo_root.as_str()])
        .await;
    harness.spawn_live_session(&main_repo_root).await;

    harness
        .engine
        .hydrate()
        .await
        .expect("hydrate should succeed");
    tokio::time::timeout(
        std::time::Duration::from_secs(2),
        harness.wait_for_reconciliation(),
    )
    .await
    .expect("reconciliation should complete for canonicalized repo roots");

    assert!(harness
        .engine
        .aline_startup_reconciliation_started_for_tests());
    assert_eq!(harness.recorded_commands().len(), 2);
    assert_eq!(harness.scheduled_repo_roots().await, vec![main_repo_root]);
}

#[tokio::test]
async fn hydrate_canonicalizes_persisted_sibling_worktree_root_against_live_session_repo_root() {
    let mut harness = make_aline_startup_harness_with_runner(
        true,
        vec![
            watcher_status_output("Running"),
            session_list_output(SAMPLE_JSON),
        ],
    )
    .await;
    let (main_repo_root, worktree_repo_root) =
        harness.create_git_sibling_worktree_pair("canonical-sibling-root");
    harness
        .persist_repo_roots(&[worktree_repo_root.as_str()])
        .await;
    harness.spawn_live_session(&main_repo_root).await;

    harness
        .engine
        .hydrate()
        .await
        .expect("hydrate should succeed");
    harness.wait_for_reconciliation().await;

    assert!(harness
        .engine
        .aline_startup_reconciliation_started_for_tests());
    assert_eq!(harness.recorded_commands().len(), 2);
    assert_eq!(harness.scheduled_repo_roots().await, vec![main_repo_root]);
}

#[tokio::test]
async fn hydrate_starts_aline_reconciliation_only_once_per_boot() {
    let mut harness = make_aline_startup_harness_with_runner(
        true,
        vec![
            watcher_status_output("Running"),
            session_list_output(SAMPLE_JSON),
        ],
    )
    .await;
    let repo_root = harness.create_git_repo("once-per-boot-repo");
    harness.persist_repo_roots(&[repo_root.as_str()]).await;

    harness
        .engine
        .hydrate()
        .await
        .expect("first hydrate should succeed");
    harness.wait_for_reconciliation().await;

    harness
        .engine
        .hydrate()
        .await
        .expect("second hydrate should succeed");
    tokio::task::yield_now().await;

    assert!(harness
        .engine
        .aline_startup_reconciliation_started_for_tests());
    assert_eq!(harness.recorded_commands().len(), 2);
}

#[tokio::test]
async fn hydrate_allows_tests_to_await_reconciliation_completion() {
    let mut harness =
        make_aline_startup_harness_with_runner(true, vec![session_list_output(SAMPLE_JSON)]).await;
    let repo_root = harness.create_git_repo("await-completion-repo");
    harness.persist_repo_roots(&[repo_root.as_str()]).await;

    harness
        .engine
        .hydrate()
        .await
        .expect("hydrate should succeed");
    harness.wait_for_reconciliation().await;

    assert!(harness.reconciliation_finished());
}

#[tokio::test]
async fn hydrate_ignores_stale_persisted_repo_roots_when_live_repo_is_valid() {
    let mut harness = make_aline_startup_harness_with_runner(
        true,
        vec![
            watcher_status_output("Running"),
            session_list_output(SAMPLE_JSON),
        ],
    )
    .await;
    let live_repo_root = harness.create_git_repo("live-valid-repo");
    harness
        .persist_repo_roots(&["/tmp/definitely-missing-startup-repo"])
        .await;
    harness.spawn_live_session(&live_repo_root).await;

    harness
        .engine
        .hydrate()
        .await
        .expect("hydrate should succeed");
    harness.wait_for_reconciliation().await;

    assert!(harness
        .engine
        .aline_startup_reconciliation_started_for_tests());
    assert_eq!(harness.recorded_commands().len(), 2);
    assert_eq!(harness.scheduled_repo_roots().await, vec![live_repo_root]);
}

#[tokio::test]
async fn successful_noop_reconciliation_clears_stale_dedupe_state() {
    let harness = make_aline_startup_harness_with_runner(
        true,
        vec![
            watcher_status_output("Running"),
            single_session_list_output("tracked"),
        ],
    )
    .await;
    let repo_root = harness.create_git_repo("cmux-next");
    tokio::fs::write(
        harness.engine.data_dir.join("aline-startup-state.json"),
        serde_json::json!({
            "updated_at": "2026-04-07T12:00:00Z",
            "recently_imported_session_ids": ["stale-session"],
        })
        .to_string(),
    )
    .await
    .expect("write stale startup dedupe state");

    let summary = harness
        .engine
        .run_aline_startup_reconciliation(std::path::PathBuf::from(repo_root))
        .await
        .expect("noop reconciliation should succeed");

    assert_eq!(
        summary.short_circuit_reason,
        Some(AlineStartupShortCircuitReason::NoSelectedSessions)
    );

    let persisted =
        tokio::fs::read_to_string(harness.engine.data_dir.join("aline-startup-state.json"))
            .await
            .expect("read refreshed startup dedupe state");
    let state: serde_json::Value =
        serde_json::from_str(&persisted).expect("dedupe state should parse");
    assert!(state["recently_imported_session_ids"]
        .as_array()
        .is_none_or(|items| items.is_empty()));
}

#[tokio::test]
async fn hydrate_succeeds_when_aline_status_command_fails() {
    let mut harness =
        make_aline_startup_harness_with_runner(true, vec![Err(anyhow::anyhow!("boom"))]).await;
    let repo_root = harness.create_git_repo("status-failure-repo");
    harness.persist_repo_roots(&[repo_root.as_str()]).await;

    harness
        .engine
        .hydrate()
        .await
        .expect("hydrate should succeed when watcher status fails");
    harness.wait_for_reconciliation().await;

    let summary = harness
        .engine
        .aline_startup_last_summary_for_tests()
        .await
        .expect("summary should be captured for status failure");

    assert!(summary.aline_available);
    assert_eq!(summary.watcher_initial_state, None);
    assert_eq!(summary.discovered_count, 0);
    assert_eq!(summary.selected_count, 0);
    assert_eq!(summary.imported_count, 0);
    assert_eq!(summary.generated_count, 0);
    assert_eq!(summary.failure_stage.as_deref(), Some("watcher_status"));
    assert!(summary
        .failure_message
        .as_deref()
        .unwrap_or_default()
        .contains("boom"));
}

#[tokio::test]
async fn hydrate_succeeds_when_session_import_fails() {
    let mut harness = make_aline_startup_harness_with_runner(
        true,
        vec![
            watcher_status_output("Running"),
            single_session_list_output("new"),
            Err(anyhow::anyhow!("import failed")),
        ],
    )
    .await;
    let repo_root = harness.create_git_repo("cmux-next");
    harness.persist_repo_roots(&[repo_root.as_str()]).await;

    harness
        .engine
        .hydrate()
        .await
        .expect("hydrate should succeed when session import fails");
    harness.wait_for_reconciliation().await;

    let summary = harness
        .engine
        .aline_startup_last_summary_for_tests()
        .await
        .expect("summary should be captured for import failure");

    assert!(summary.aline_available);
    assert_eq!(summary.watcher_initial_state, Some(WatcherState::Running));
    assert_eq!(summary.selected_count, 1);
    assert_eq!(summary.imported_count, 0);
    assert_eq!(summary.generated_count, 0);
    assert_eq!(summary.failure_stage.as_deref(), Some("session_import"));
    assert!(summary
        .failure_message
        .as_deref()
        .unwrap_or_default()
        .contains("import failed"));
}
