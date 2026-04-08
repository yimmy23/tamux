use super::support::*;
use super::*;

use std::path::{Path, PathBuf};
use std::time::Duration;

#[tokio::test]
async fn startup_reconciliation_lists_up_to_three_pages_then_stops() {
    let runner = StubRunner::with_outputs(vec![
        session_list_output(
            r#"{
	"has_more": true,
	"sessions": [{
		"status": "new",
		"source": "codex",
		"project_name": "cmux-next",
		"project_path": "/home/mkurman/gitlab/it/cmux-next",
		"session_id": "page-1-a",
		"created_at": "2026-04-07T09:00:00Z",
		"last_activity": "2026-04-07T11:30:00Z",
		"session_file": "/tmp/aline/cmux-next/page-1-a.json"
	}]
}"#,
        ),
        session_list_output(
            r#"{
	"has_more": true,
	"sessions": [{
		"status": "new",
		"source": "codex",
		"project_name": "cmux-next",
		"project_path": "/home/mkurman/gitlab/it/cmux-next",
		"session_id": "page-2-a",
		"created_at": "2026-04-07T08:30:00Z",
		"last_activity": "2026-04-07T11:00:00Z",
		"session_file": "/tmp/aline/cmux-next/page-2-a.json"
	}]
}"#,
        ),
        session_list_output(
            r#"{
	"has_more": true,
	"sessions": [{
		"status": "new",
		"source": "codex",
		"project_name": "cmux-next",
		"project_path": "/home/mkurman/gitlab/it/cmux-next",
		"session_id": "page-3-a",
		"created_at": "2026-04-07T08:00:00Z",
		"last_activity": "2026-04-07T10:30:00Z",
		"session_file": "/tmp/aline/cmux-next/page-3-a.json"
	}]
}"#,
        ),
    ]);

    let sessions = list_recent_project_sessions(
        &runner,
        Path::new("/home/mkurman/gitlab/it/cmux-next"),
        ts("2026-04-07T12:00:00Z"),
        StartupSelectionPolicy::default(),
    )
    .await
    .expect("paged list should succeed");

    assert_eq!(sessions.len(), 3);
    assert_eq!(
        runner
            .recorded_specs()
            .iter()
            .map(|spec| spec.args.clone())
            .collect::<Vec<_>>(),
        vec![
            vec![
                "watcher",
                "session",
                "list",
                "--json",
                "--page",
                "1",
                "--per-page",
                "30"
            ],
            vec![
                "watcher",
                "session",
                "list",
                "--json",
                "--page",
                "2",
                "--per-page",
                "30"
            ],
            vec![
                "watcher",
                "session",
                "list",
                "--json",
                "--page",
                "3",
                "--per-page",
                "30"
            ],
        ]
        .into_iter()
        .map(|args| args.into_iter().map(str::to_string).collect::<Vec<_>>())
        .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn startup_reconciliation_stops_when_page_reports_no_more_results() {
    let runner = StubRunner::with_outputs(vec![session_list_output(
        r#"{
	"has_more": false,
	"sessions": [{
		"status": "new",
		"source": "codex",
		"project_name": "cmux-next",
		"project_path": "/home/mkurman/gitlab/it/cmux-next",
		"session_id": "page-1-a",
		"created_at": "2026-04-07T09:00:00Z",
		"last_activity": "2026-04-07T11:30:00Z",
		"session_file": "/tmp/aline/cmux-next/page-1-a.json"
	}]
}"#,
    )]);

    let sessions = list_recent_project_sessions(
        &runner,
        Path::new("/home/mkurman/gitlab/it/cmux-next"),
        ts("2026-04-07T12:00:00Z"),
        StartupSelectionPolicy::default(),
    )
    .await
    .expect("single terminal page should succeed");

    assert_eq!(sessions.len(), 1);
    assert_eq!(runner.recorded_specs().len(), 1);
}

#[tokio::test]
async fn startup_reconciliation_merges_pages_then_sorts_by_last_activity() {
    let runner = StubRunner::with_outputs(vec![
        session_list_output(
            r#"{
	"has_more": true,
	"sessions": [
		{
			"status": "new",
			"source": "codex",
			"project_name": "cmux-next",
			"project_path": "/home/mkurman/gitlab/it/cmux-next",
			"session_id": "oldest",
			"created_at": "2026-04-07T07:00:00Z",
			"last_activity": "2026-04-07T09:00:00Z",
			"session_file": "/tmp/aline/cmux-next/oldest.json"
		},
		{
			"status": "new",
			"source": "codex",
			"project_name": "cmux-next",
			"project_path": "/home/mkurman/gitlab/it/cmux-next",
			"session_id": "middle",
			"created_at": "2026-04-07T08:00:00Z",
			"last_activity": "2026-04-07T10:00:00Z",
			"session_file": "/tmp/aline/cmux-next/middle.json"
		}
	]
}"#,
        ),
        session_list_output(
            r#"{
	"has_more": true,
	"sessions": [
		{
			"status": "new",
			"source": "codex",
			"project_name": "cmux-next",
			"project_path": "/home/mkurman/gitlab/it/cmux-next",
			"session_id": "newest",
			"created_at": "2026-04-07T09:00:00Z",
			"last_activity": "2026-04-07T11:59:00Z",
			"session_file": "/tmp/aline/cmux-next/newest.json"
		},
		{
			"status": "new",
			"source": "codex",
			"project_name": "cmux-next",
			"project_path": "/home/mkurman/gitlab/it/cmux-next",
			"session_id": "second-newest",
			"created_at": "2026-04-07T08:30:00Z",
			"last_activity": "2026-04-07T11:00:00Z",
			"session_file": "/tmp/aline/cmux-next/second-newest.json"
		}
	]
}"#,
        ),
        session_list_output(r#"{ "has_more": false, "sessions": [] }"#),
    ]);

    let sessions = list_recent_project_sessions(
        &runner,
        Path::new("/home/mkurman/gitlab/it/cmux-next"),
        ts("2026-04-07T12:00:00Z"),
        StartupSelectionPolicy {
            max_candidates: 3,
            ..StartupSelectionPolicy::default()
        },
    )
    .await
    .expect("paged list should succeed");

    assert_eq!(
        sessions
            .iter()
            .map(|session| session.session_id.as_str())
            .collect::<Vec<_>>(),
        vec!["newest", "second-newest", "middle"]
    );
}

#[tokio::test]
async fn startup_reconciliation_deduplicates_sessions_across_pages() {
    let runner = StubRunner::with_outputs(vec![
        session_list_output(
            r#"{
	"has_more": true,
	"sessions": [{
		"status": "new",
		"source": "codex",
		"project_name": "cmux-next",
		"project_path": "/home/mkurman/gitlab/it/cmux-next",
		"session_id": "duplicate",
		"created_at": "2026-04-07T09:00:00Z",
		"last_activity": "2026-04-07T11:59:00Z",
		"session_file": "/tmp/aline/cmux-next/duplicate.json"
	}]
}"#,
        ),
        session_list_output(
            r#"{
	"has_more": false,
	"sessions": [
		{
			"status": "new",
			"source": "codex",
			"project_name": "cmux-next",
			"project_path": "/home/mkurman/gitlab/it/cmux-next",
			"session_id": "duplicate",
			"created_at": "2026-04-07T09:00:00Z",
			"last_activity": "2026-04-07T11:59:00Z",
			"session_file": "/tmp/aline/cmux-next/duplicate.json"
		},
		{
			"status": "new",
			"source": "codex",
			"project_name": "cmux-next",
			"project_path": "/home/mkurman/gitlab/it/cmux-next",
			"session_id": "unique",
			"created_at": "2026-04-07T08:00:00Z",
			"last_activity": "2026-04-07T10:00:00Z",
			"session_file": "/tmp/aline/cmux-next/unique.json"
		}
	]
}"#,
        ),
    ]);

    let sessions = list_recent_project_sessions(
        &runner,
        Path::new("/home/mkurman/gitlab/it/cmux-next"),
        ts("2026-04-07T12:00:00Z"),
        StartupSelectionPolicy::default(),
    )
    .await
    .expect("deduped pages should succeed");

    assert_eq!(
        sessions
            .iter()
            .map(|session| session.session_id.as_str())
            .collect::<Vec<_>>(),
        vec!["duplicate", "unique"]
    );
}

#[tokio::test]
async fn startup_reconciliation_keeps_newest_duplicate_session_row() {
    let runner = StubRunner::with_outputs(vec![
        session_list_output(
            r#"{
	"has_more": true,
	"sessions": [{
		"status": "new",
		"source": "codex",
		"project_name": "cmux-next",
		"project_path": "/home/mkurman/gitlab/it/cmux-next",
		"session_id": "duplicate",
		"created_at": "2026-04-07T08:00:00Z",
		"last_activity": "2026-04-07T09:00:00Z",
		"session_file": "/tmp/aline/cmux-next/duplicate.json"
	}]
}"#,
        ),
        session_list_output(
            r#"{
	"has_more": false,
	"sessions": [{
		"status": "new",
		"source": "codex",
		"project_name": "cmux-next",
		"project_path": "/home/mkurman/gitlab/it/cmux-next",
		"session_id": "duplicate",
		"created_at": "2026-04-07T09:00:00Z",
		"last_activity": "2026-04-07T11:59:00Z",
		"session_file": "/tmp/aline/cmux-next/duplicate.json"
	}]
}"#,
        ),
    ]);

    let sessions = list_recent_project_sessions(
        &runner,
        Path::new("/home/mkurman/gitlab/it/cmux-next"),
        ts("2026-04-07T12:00:00Z"),
        StartupSelectionPolicy::default(),
    )
    .await
    .expect("deduped pages should succeed");

    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].created_at, "2026-04-07T09:00:00Z");
    assert_eq!(sessions[0].last_activity, "2026-04-07T11:59:00Z");
}

#[test]
fn import_commands_use_full_session_id_not_index_or_prefix() {
    let discovered = session(
        "new",
        "codex",
        "cmux-next",
        Some("/home/mkurman/gitlab/it/cmux-next"),
        FULL_SESSION_ID,
        "2026-04-07T09:00:00Z",
        "2026-04-07T11:59:00Z",
    );
    let command = build_import_command(&discovered);
    assert_eq!(command.program, "aline");
    assert_eq!(
        command.args,
        vec!["watcher", "session", "import", FULL_SESSION_ID, "--sync"]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>()
    );
    assert_eq!(command.timeout, IMPORT_TIMEOUT);
}

#[tokio::test]
async fn tokio_startup_command_runner_executes_command() {
    let runner = TokioStartupCommandRunner;
    #[cfg(unix)]
    let spec = StartupCommandSpec {
        program: "sh".to_string(),
        args: vec!["-c".to_string(), "printf ok".to_string()],
        timeout: Duration::from_secs(5),
    };
    #[cfg(windows)]
    let spec = StartupCommandSpec {
        program: "cmd".to_string(),
        args: vec!["/C".to_string(), "echo|set /p=ok".to_string()],
        timeout: Duration::from_secs(5),
    };
    let output = runner
        .run(spec)
        .await
        .expect("runner should execute command");
    assert_eq!(output.exit_code, 0);
    assert_eq!(output.stdout.trim(), "ok");
}

#[cfg(unix)]
#[tokio::test]
async fn tokio_startup_command_runner_times_out_promptly() {
    let runner = TokioStartupCommandRunner;
    let error = runner
        .run(StartupCommandSpec {
            program: "sh".to_string(),
            args: vec!["-c".to_string(), "sleep 30".to_string()],
            timeout: Duration::from_millis(10),
        })
        .await
        .expect_err("sleeping command should time out");
    assert!(error.to_string().contains("timed out"));
}

#[tokio::test]
async fn startup_reconciliation_runs_status_then_start_then_import_then_generate() {
    let harness = make_aline_startup_harness_with_runner(
        true,
        vec![
            watcher_status_output("Stopped"),
            command_output("Watcher started\n"),
            single_session_list_output("new"),
            command_output("Imported\n"),
            session_show_output(),
            command_output("Generated events\n"),
        ],
    )
    .await;
    let repo_root = harness.create_git_repo("cmux-next");

    harness
        .engine
        .run_aline_startup_reconciliation(PathBuf::from(repo_root))
        .await
        .expect("reconciliation should succeed");

    assert_eq!(
        harness.recorded_commands(),
        vec![
            vec!["watcher".to_string(), "status".to_string()],
            vec!["watcher".to_string(), "start".to_string()],
            vec![
                "watcher",
                "session",
                "list",
                "--json",
                "--page",
                "1",
                "--per-page",
                "30"
            ]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>(),
            vec!["watcher", "session", "import", FULL_SESSION_ID, "--sync"]
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>(),
            vec!["watcher", "session", "show", FULL_SESSION_ID, "--json"]
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>(),
            vec!["watcher", "event", "generate", FULL_SESSION_ID]
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>(),
        ]
    );
}

#[tokio::test]
async fn startup_reconciliation_does_not_generate_events_before_import_is_confirmed() {
    let harness = make_aline_startup_harness_with_runner(
        true,
        vec![
            watcher_status_output("Running"),
            single_session_list_output("new"),
            command_output("Imported\n"),
            session_show_missing_output(),
            session_show_output(),
            command_output("Generated events\n"),
        ],
    )
    .await;
    let repo_root = harness.create_git_repo("cmux-next");

    harness
        .engine
        .run_aline_startup_reconciliation(PathBuf::from(repo_root))
        .await
        .expect("reconciliation should succeed after tracked confirmation");

    assert_eq!(
        harness.recorded_commands(),
        vec![
            vec!["watcher".to_string(), "status".to_string()],
            vec![
                "watcher",
                "session",
                "list",
                "--json",
                "--page",
                "1",
                "--per-page",
                "30"
            ]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>(),
            vec!["watcher", "session", "import", FULL_SESSION_ID, "--sync"]
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>(),
            vec!["watcher", "session", "show", FULL_SESSION_ID, "--json"]
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>(),
            vec!["watcher", "session", "show", FULL_SESSION_ID, "--json"]
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>(),
            vec!["watcher", "event", "generate", FULL_SESSION_ID]
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>(),
        ]
    );
}

#[tokio::test]
async fn startup_reconciliation_allows_event_generation_after_partial_import_state() {
    let harness = make_aline_startup_harness_with_runner(
        true,
        vec![
            watcher_status_output("Running"),
            single_session_list_output("new"),
            command_output("Imported\n"),
            session_show_output(),
            command_output("Generated events\n"),
        ],
    )
    .await;
    let repo_root = harness.create_git_repo("cmux-next");

    harness
        .engine
        .run_aline_startup_reconciliation(PathBuf::from(repo_root))
        .await
        .expect("reconciliation should treat partial imports as ready");

    assert_eq!(
        harness.recorded_commands(),
        vec![
            vec!["watcher".to_string(), "status".to_string()],
            vec![
                "watcher",
                "session",
                "list",
                "--json",
                "--page",
                "1",
                "--per-page",
                "30"
            ]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>(),
            vec!["watcher", "session", "import", FULL_SESSION_ID, "--sync"]
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>(),
            vec!["watcher", "session", "show", FULL_SESSION_ID, "--json"]
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>(),
            vec!["watcher", "event", "generate", FULL_SESSION_ID]
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>(),
        ]
    );
}

#[tokio::test]
async fn startup_reconciliation_emits_stage_summary_fields() {
    let harness = make_aline_startup_harness_with_runner(
        true,
        vec![
            watcher_status_output("Running"),
            single_session_list_output("tracked"),
        ],
    )
    .await;
    let repo_root = harness.create_git_repo("cmux-next");

    let summary = harness
        .engine
        .run_aline_startup_reconciliation(PathBuf::from(repo_root))
        .await
        .expect("reconciliation should short-circuit cleanly");

    assert!(summary.aline_available);
    assert_eq!(summary.watcher_initial_state, Some(WatcherState::Running));
    assert!(!summary.watcher_started);
    assert_eq!(summary.discovered_count, 1);
    assert_eq!(summary.selected_count, 0);
    assert_eq!(summary.imported_count, 0);
    assert_eq!(summary.generated_count, 0);
    assert_eq!(
        summary.short_circuit_reason,
        Some(AlineStartupShortCircuitReason::NoSelectedSessions)
    );
}

#[tokio::test]
async fn startup_reconciliation_counts_import_before_confirmation_failure() {
    let harness = make_aline_startup_harness_with_runner(
        true,
        vec![
            watcher_status_output("Running"),
            single_session_list_output("new"),
            import_success_output(),
            session_show_missing_output(),
            session_show_missing_output(),
            session_show_missing_output(),
            session_show_missing_output(),
            session_show_missing_output(),
            session_show_missing_output(),
            session_show_missing_output(),
            session_show_missing_output(),
        ],
    )
    .await;
    let repo_root = harness.create_git_repo("cmux-next");

    let summary = harness
        .engine
        .run_aline_startup_reconciliation(PathBuf::from(repo_root))
        .await
        .expect("reconciliation should return summary when confirmation fails");

    assert_eq!(summary.discovered_count, 1);
    assert_eq!(summary.selected_count, 1);
    assert_eq!(summary.imported_count, 1);
    assert_eq!(summary.generated_count, 0);
    assert_eq!(
        summary.short_circuit_reason,
        Some(AlineStartupShortCircuitReason::ImportNotConfirmed)
    );
}

#[tokio::test]
async fn startup_reconciliation_does_not_persist_dedupe_on_event_generate_failure() {
    let harness = make_aline_startup_harness_with_runner(
        true,
        vec![
            watcher_status_output("Running"),
            single_session_list_output("new"),
            import_success_output(),
            session_show_output(),
            Err(anyhow::anyhow!("event generation failed")),
        ],
    )
    .await;
    let repo_root = harness.create_git_repo("cmux-next");

    let summary = harness
        .engine
        .run_aline_startup_reconciliation(PathBuf::from(repo_root))
        .await
        .expect("event generation failure should become a summary");

    assert_eq!(summary.imported_count, 1);
    assert_eq!(summary.generated_count, 0);
    assert_eq!(
        summary.recently_imported_session_ids,
        vec![FULL_SESSION_ID.to_string()]
    );
    assert_eq!(summary.failure_stage.as_deref(), Some("event_generate"));
}

#[tokio::test]
async fn startup_reconciliation_skips_recently_imported_session_ids_from_dedupe_state() {
    let harness = make_aline_startup_harness_with_runner(
        true,
        vec![
            watcher_status_output("Running"),
            session_list_with_sessions(
                false,
                &[
                    session(
                        "new",
                        "codex",
                        "cmux-next",
                        Some("/home/mkurman/gitlab/it/cmux-next"),
                        "already-imported-session",
                        "2026-04-07T09:00:00Z",
                        "2026-04-07T11:58:00Z",
                    ),
                    session(
                        "new",
                        "codex",
                        "cmux-next",
                        Some("/home/mkurman/gitlab/it/cmux-next"),
                        "fresh-session",
                        "2026-04-07T09:30:00Z",
                        "2026-04-07T11:57:00Z",
                    ),
                ],
            ),
            import_success_output(),
            session_show_output(),
            command_output("Generated events\n"),
        ],
    )
    .await;
    let repo_root = harness.create_git_repo("cmux-next");
    tokio::fs::write(
        harness.engine.data_dir.join("aline-startup-state.json"),
        serde_json::json!({
            "updated_at": "2026-04-07T12:00:00Z",
            "recently_imported_session_ids": ["already-imported-session"],
        })
        .to_string(),
    )
    .await
    .expect("write startup dedupe state");

    let summary = harness
        .engine
        .run_aline_startup_reconciliation(PathBuf::from(repo_root))
        .await
        .expect("dedupe state should not make reconciliation fail");

    assert_eq!(summary.selected_count, 1);
    assert_eq!(summary.skipped_recently_imported_count, 1);
    assert_eq!(summary.imported_count, 1);
    assert_eq!(summary.generated_count, 1);
    assert_eq!(
        harness.recorded_commands(),
        vec![
            vec!["watcher".to_string(), "status".to_string()],
            vec![
                "watcher",
                "session",
                "list",
                "--json",
                "--page",
                "1",
                "--per-page",
                "30"
            ]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>(),
            vec!["watcher", "session", "import", "fresh-session", "--sync"]
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>(),
            vec!["watcher", "session", "show", "fresh-session", "--json"]
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>(),
            vec!["watcher", "event", "generate", "fresh-session"]
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>(),
        ]
    );
}

#[tokio::test]
async fn startup_reconciliation_stops_scheduling_new_imports_when_budget_is_exhausted() {
    let harness = make_aline_startup_harness_with_responses(
        true,
        vec![
            delayed_output(
                watcher_status_output("Running"),
                Duration::from_millis(7200),
            ),
            delayed_output(
                session_list_with_sessions(
                    false,
                    &[
                        session(
                            "new",
                            "codex",
                            "cmux-next",
                            Some("/home/mkurman/gitlab/it/cmux-next"),
                            "budget-one",
                            "2026-04-07T09:00:00Z",
                            "2026-04-07T11:59:00Z",
                        ),
                        session(
                            "new",
                            "codex",
                            "cmux-next",
                            Some("/home/mkurman/gitlab/it/cmux-next"),
                            "budget-two",
                            "2026-04-07T08:50:00Z",
                            "2026-04-07T11:58:00Z",
                        ),
                        session(
                            "new",
                            "codex",
                            "cmux-next",
                            Some("/home/mkurman/gitlab/it/cmux-next"),
                            "budget-three",
                            "2026-04-07T08:40:00Z",
                            "2026-04-07T11:57:00Z",
                        ),
                    ],
                ),
                Duration::from_millis(7200),
            ),
            delayed_output(import_success_output(), Duration::from_secs(1)),
            delayed_output(session_show_output(), Duration::ZERO),
            delayed_output(command_output("Generated events\n"), Duration::ZERO),
        ],
    )
    .await;
    let repo_root = harness.create_git_repo("cmux-next");

    let summary = harness
        .engine
        .run_aline_startup_reconciliation(PathBuf::from(repo_root))
        .await
        .expect("budget exhaustion should short-circuit cleanly");

    assert_eq!(summary.selected_count, 3);
    assert_eq!(summary.imported_count, 1);
    assert_eq!(summary.generated_count, 1);
    assert_eq!(
        harness.recorded_commands(),
        vec![
            vec!["watcher".to_string(), "status".to_string()],
            vec![
                "watcher",
                "session",
                "list",
                "--json",
                "--page",
                "1",
                "--per-page",
                "30"
            ]
            .into_iter()
            .map(str::to_string)
            .collect::<Vec<_>>(),
            vec!["watcher", "session", "import", "budget-one", "--sync"]
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>(),
            vec!["watcher", "session", "show", "budget-one", "--json"]
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>(),
            vec!["watcher", "event", "generate", "budget-one"]
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>(),
        ]
    );
}

#[tokio::test]
async fn startup_reconciliation_allows_current_session_to_finish_after_confirmation_even_when_budget_is_low(
) {
    let harness = make_aline_startup_harness_with_responses(
        true,
        vec![
            delayed_output(
                watcher_status_output("Running"),
                Duration::from_millis(4500),
            ),
            delayed_output(
                session_list_with_sessions(
                    false,
                    &[
                        session(
                            "new",
                            "codex",
                            "cmux-next",
                            Some("/home/mkurman/gitlab/it/cmux-next"),
                            "budget-one",
                            "2026-04-07T09:00:00Z",
                            "2026-04-07T11:59:00Z",
                        ),
                        session(
                            "new",
                            "codex",
                            "cmux-next",
                            Some("/home/mkurman/gitlab/it/cmux-next"),
                            "budget-two",
                            "2026-04-07T08:50:00Z",
                            "2026-04-07T11:58:00Z",
                        ),
                    ],
                ),
                Duration::from_millis(4500),
            ),
            delayed_output(import_success_output(), Duration::from_millis(4500)),
            delayed_output(session_show_missing_output(), Duration::from_millis(4500)),
            delayed_output(session_show_missing_output(), Duration::from_millis(4500)),
            delayed_output(session_show_output(), Duration::from_millis(4500)),
            delayed_output(
                command_output("Generated events\n"),
                Duration::from_millis(100),
            ),
        ],
    )
    .await;
    let repo_root = harness.create_git_repo("cmux-next");

    let summary = harness
        .engine
        .run_aline_startup_reconciliation(PathBuf::from(repo_root))
        .await
        .expect("budget-limited run should return a summary");

    assert_eq!(summary.imported_count, 1);
    assert_eq!(summary.generated_count, 1);
    assert!(summary.budget_exhausted);
    assert_eq!(
        summary.short_circuit_reason,
        Some(AlineStartupShortCircuitReason::BudgetExhausted)
    );
}
