use super::support::*;
use super::*;

use std::path::Path;
use std::time::Duration;
use tempfile::TempDir;

#[test]
fn parse_watcher_status_detects_stopped_mode() {
    let status = parse_watcher_status("Watcher Status: Stopped\nMode: Standalone (SQLite)");
    assert_eq!(status.state, WatcherState::Stopped);
}

#[test]
fn parse_watcher_status_detects_running_mode_and_rejects_not_running() {
    let running = parse_watcher_status("Watcher Status: Running\nMode: Standalone (SQLite)");
    assert_eq!(running.state, WatcherState::Running);

    let unknown = parse_watcher_status("Watcher Status: Not Running\nMode: Standalone (SQLite)");
    assert_eq!(unknown.state, WatcherState::Unknown);
}

#[test]
fn parse_session_list_json_reads_real_cli_fields() {
    let list = parse_session_list_json(SAMPLE_JSON).expect("json should parse");
    assert_eq!(list.sessions[0].project_name, "cmux-next");
    assert!(list.sessions[0].session_id.starts_with("rollout-"));
}

#[test]
fn parse_session_list_json_accepts_full_current_cli_payload_fields() {
    let list = parse_session_list_json(FULL_CURRENT_CLI_PAYLOAD_JSON).expect("json should parse");
    assert_eq!(list.sessions.len(), 1);
    assert!(list.has_more.is_none());
    assert_eq!(list.sessions[0].source, "codex");
    assert_eq!(
        list.sessions[0].project_path.as_deref(),
        Some("/home/mkurman/gitlab/it/cmux-next")
    );
    assert_eq!(list.sessions[0].created_at, "2026-04-07T09:00:00.000000");
    assert!(list.sessions[0].session_file.ends_with(".jsonl"));
}

#[test]
fn candidate_selection_keeps_recent_new_exact_project_matches_only() {
    let now = ts("2026-04-07T12:00:00Z");
    let sessions = vec![
        session(
            "new",
            "codex",
            "cmux-next",
            Some("/home/mkurman/gitlab/it/cmux-next"),
            "recent-a",
            "2026-04-07T11:00:00Z",
            "2026-04-07T11:59:00Z",
        ),
        session(
            "new",
            "codex",
            "cmux-next",
            Some("/home/mkurman/gitlab/it/cmux-next"),
            "recent-b",
            "2026-04-07T10:30:00Z",
            "2026-04-07T11:30:00Z",
        ),
        session(
            "new",
            "codex",
            "cmux-next",
            Some("/home/mkurman/gitlab/it/cmux-next"),
            "recent-c",
            "2026-04-07T09:30:00Z",
            "2026-04-07T10:30:00Z",
        ),
        session(
            "tracked",
            "codex",
            "cmux-next",
            Some("/home/mkurman/gitlab/it/cmux-next"),
            "tracked-a",
            "2026-04-07T11:10:00Z",
            "2026-04-07T11:50:00Z",
        ),
        session(
            "new",
            "codex",
            "cmux-next-extra",
            Some("/home/mkurman/gitlab/it/cmux-next-extra"),
            "wrong-project",
            "2026-04-07T10:40:00Z",
            "2026-04-07T11:40:00Z",
        ),
        session(
            "new",
            "codex",
            "CMUX-NEXT",
            Some("/home/mkurman/gitlab/it/CMUX-NEXT"),
            "wrong-case",
            "2026-04-07T10:20:00Z",
            "2026-04-07T11:20:00Z",
        ),
        session(
            "new",
            "codex",
            "cmux-next",
            Some("/home/mkurman/gitlab/it/cmux-next"),
            "too-old",
            "2026-04-02T11:59:59Z",
            "2026-04-03T11:59:59Z",
        ),
    ];

    let selected = select_import_candidates(
        Path::new("/home/mkurman/gitlab/it/cmux-next"),
        &sessions,
        now,
        StartupSelectionPolicy::default(),
    );

    assert_eq!(selected.len(), 3);
    assert!(selected.iter().all(|item| item.status == "new"));
    assert!(selected.iter().all(|item| item.project_name == "cmux-next"));
}

#[test]
fn candidate_selection_prefers_project_path_when_present() {
    let temp = TempDir::new().expect("tempdir");
    let repo_a_parent = temp.path().join("a");
    let repo_b_parent = temp.path().join("b");
    let repo_a = repo_a_parent.join("cmux-next");
    let repo_b = repo_b_parent.join("cmux-next");
    std::fs::create_dir_all(&repo_a).expect("create repo_a");
    std::fs::create_dir_all(&repo_b).expect("create repo_b");
    for repo in [&repo_a, &repo_b] {
        let output = std::process::Command::new("git")
            .arg("init")
            .current_dir(repo)
            .output()
            .expect("git init should spawn");
        assert!(output.status.success(), "git init failed");
    }

    let now = ts("2026-04-07T12:00:00Z");
    let sessions = vec![
        session(
            "new",
            "codex",
            "same-name",
            Some(repo_a.to_str().expect("utf-8 path")),
            "path-match",
            "2026-04-07T11:00:00.000000",
            "2026-04-07T11:59:00.000000",
        ),
        session(
            "new",
            "codex",
            "cmux-next",
            Some(repo_b.to_str().expect("utf-8 path")),
            "path-mismatch",
            "2026-04-07T10:00:00.000000",
            "2026-04-07T11:58:00.000000",
        ),
    ];

    let selected =
        select_import_candidates(&repo_a, &sessions, now, StartupSelectionPolicy::default());

    assert_eq!(
        selected
            .iter()
            .map(|item| item.session_id.as_str())
            .collect::<Vec<_>>(),
        vec!["path-match"]
    );
}

#[test]
fn repo_root_basename_extracts_expected_repo_name() {
    assert_eq!(
        repo_root_basename(Path::new("/home/mkurman/gitlab/it/cmux-next")),
        Some("cmux-next")
    );
    assert_eq!(
        repo_root_basename(Path::new(
            "/home/mkurman/gitlab/it/cmux-next/.worktrees/aline-startup-reconciliation"
        )),
        Some("cmux-next")
    );
}

#[test]
fn repo_root_basename_must_match_project_name_exactly() {
    let matching = session(
        "new",
        "codex",
        "cmux-next",
        None,
        "recent-a",
        "2026-04-07T11:00:00Z",
        "2026-04-07T11:59:00Z",
    );
    let mismatched = session(
        "new",
        "codex",
        "CMUX-NEXT",
        None,
        "recent-b",
        "2026-04-07T10:30:00Z",
        "2026-04-07T11:30:00Z",
    );

    assert!(repo_root_matches_project_name(
        Path::new("/home/mkurman/gitlab/it/cmux-next"),
        &matching.project_name,
    ));
    assert!(!repo_root_matches_project_name(
        Path::new("/home/mkurman/gitlab/it/cmux-next"),
        &mismatched.project_name,
    ));
}

#[test]
fn session_matches_repo_accepts_same_repo_worktree_paths() {
    let session = session(
        "new",
        "codex",
        "cmux-next",
        Some("/home/mkurman/gitlab/it/cmux-next"),
        "recent-a",
        "2026-04-07T11:00:00Z",
        "2026-04-07T11:59:00Z",
    );

    assert!(session_matches_repo(
        Path::new("/home/mkurman/gitlab/it/cmux-next/.worktrees/aline-startup-reconciliation"),
        &session,
    ));
}

#[test]
fn session_matches_repo_falls_back_for_unresolvable_project_path() {
    let session = session(
        "new",
        "codex",
        "cmux-next",
        Some("/tmp/missing-parent/cmux-next"),
        "recent-a",
        "2026-04-07T11:00:00Z",
        "2026-04-07T11:59:00Z",
    );

    assert!(session_matches_repo(
        Path::new("/home/mkurman/gitlab/it/cmux-next"),
        &session,
    ));
}

#[test]
fn session_matches_repo_falls_back_when_project_path_is_missing() {
    let session = session(
        "new",
        "codex",
        "cmux-next",
        None,
        "recent-a",
        "2026-04-07T11:00:00Z",
        "2026-04-07T11:59:00Z",
    );

    assert!(session_matches_repo(
        Path::new("/home/mkurman/gitlab/it/cmux-next"),
        &session,
    ));
}

#[test]
fn session_matches_repo_rejects_proven_different_repo_with_same_basename() {
    let temp = TempDir::new().expect("tempdir");
    let repo_a_parent = temp.path().join("a");
    let repo_b_parent = temp.path().join("b");
    let repo_a = repo_a_parent.join("cmux-next");
    let repo_b = repo_b_parent.join("cmux-next");
    std::fs::create_dir_all(&repo_a).expect("create repo_a");
    std::fs::create_dir_all(&repo_b).expect("create repo_b");
    for repo in [&repo_a, &repo_b] {
        let output = std::process::Command::new("git")
            .arg("init")
            .current_dir(repo)
            .output()
            .expect("git init should spawn");
        assert!(output.status.success(), "git init failed");
    }

    let session = session(
        "new",
        "codex",
        "cmux-next",
        Some(repo_b.to_str().expect("utf-8 path")),
        "recent-a",
        "2026-04-07T11:00:00.000000",
        "2026-04-07T11:59:00.000000",
    );

    assert!(!session_matches_repo(&repo_a, &session));
}

#[test]
fn parse_session_list_json_rejects_missing_project_name_or_session_id() {
    let missing_project_name = r#"
    {
      "sessions": [
        {
          "status": "new",
          "source": "codex",
          "project_name": "",
          "session_id": "rollout-2026-04-07-003",
          "created_at": "2026-04-07T09:00:00Z",
          "last_activity": "2026-04-07T11:45:00Z",
          "session_file": "/tmp/aline/cmux-next/session-3.json"
        }
      ]
    }
    "#;
    let missing_session_id = r#"
    {
      "sessions": [
        {
          "status": "new",
          "source": "codex",
          "project_name": "cmux-next",
          "session_id": "",
          "created_at": "2026-04-07T09:00:00Z",
          "last_activity": "2026-04-07T11:45:00Z",
          "session_file": "/tmp/aline/cmux-next/session-4.json"
        }
      ]
    }
    "#;

    let project_error =
        parse_session_list_json(missing_project_name).expect_err("invalid rows should fail closed");
    assert!(project_error.to_string().contains("project_name"));

    let session_error =
        parse_session_list_json(missing_session_id).expect_err("invalid rows should fail closed");
    assert!(session_error.to_string().contains("session_id"));
}

#[test]
fn parse_session_list_json_rejects_missing_session_file() {
    let missing_session_file = r#"
    {
      "sessions": [
        {
          "status": "new",
          "source": "codex",
          "project_name": "cmux-next",
          "session_id": "rollout-2026-04-07-005",
          "created_at": "2026-04-07T09:00:00Z",
          "last_activity": "2026-04-07T11:45:00Z",
          "session_file": ""
        }
      ]
    }
    "#;

    let error = parse_session_list_json(missing_session_file)
        .expect_err("missing session file should fail closed");
    assert!(error.to_string().contains("session_file"));
}

#[test]
fn parse_session_list_json_rejects_blank_or_invalid_last_activity() {
    let missing_last_activity = r#"
    {
      "sessions": [
        {
          "status": "new",
          "source": "codex",
          "project_name": "cmux-next",
          "session_id": "rollout-2026-04-07-006",
          "created_at": "2026-04-07T09:00:00Z",
          "last_activity": "",
          "session_file": "/tmp/aline/cmux-next/session-6.jsonl"
        }
      ]
    }
    "#;
    let invalid_last_activity = r#"
    {
      "sessions": [
        {
          "status": "new",
          "source": "codex",
          "project_name": "cmux-next",
          "session_id": "rollout-2026-04-07-007",
          "created_at": "2026-04-07T09:00:00Z",
          "last_activity": "not-a-timestamp",
          "session_file": "/tmp/aline/cmux-next/session-7.jsonl"
        }
      ]
    }
    "#;

    let blank_error = parse_session_list_json(missing_last_activity)
        .expect_err("blank last_activity should fail closed");
    assert!(blank_error.to_string().contains("last_activity"));

    let invalid_error = parse_session_list_json(invalid_last_activity)
        .expect_err("invalid last_activity should fail closed");
    assert!(invalid_error.to_string().contains("invalid last_activity"));
}

#[test]
fn parse_session_list_json_rejects_blank_or_invalid_created_at() {
    let missing_created_at = r#"
    {
      "sessions": [
        {
          "status": "new",
          "source": "codex",
          "project_name": "cmux-next",
          "session_id": "rollout-2026-04-07-008",
          "created_at": "",
          "last_activity": "2026-04-07T11:45:00Z",
          "session_file": "/tmp/aline/cmux-next/session-8.jsonl"
        }
      ]
    }
    "#;
    let invalid_created_at = r#"
    {
      "sessions": [
        {
          "status": "new",
          "source": "codex",
          "project_name": "cmux-next",
          "session_id": "rollout-2026-04-07-009",
          "created_at": "not-a-timestamp",
          "last_activity": "2026-04-07T11:45:00Z",
          "session_file": "/tmp/aline/cmux-next/session-9.jsonl"
        }
      ]
    }
    "#;

    let blank_error = parse_session_list_json(missing_created_at)
        .expect_err("blank created_at should fail closed");
    assert!(blank_error.to_string().contains("created_at"));

    let invalid_error = parse_session_list_json(invalid_created_at)
        .expect_err("invalid created_at should fail closed");
    assert!(invalid_error.to_string().contains("invalid created_at"));
}

#[test]
fn startup_policy_uses_spec_constants() {
    let policy = StartupSelectionPolicy::default();

    assert_eq!(policy.max_candidates, 3);
    assert_eq!(policy.max_pages, 3);
    assert_eq!(policy.recency_window, Duration::from_secs(72 * 60 * 60));
    assert_eq!(WATCHER_COMMAND_TIMEOUT, Duration::from_secs(5));
    assert_eq!(IMPORT_TIMEOUT, Duration::from_secs(5));
    assert_eq!(TRACKED_POLL_INTERVAL, Duration::from_millis(250));
    assert_eq!(TRACKED_POLL_MAX_ATTEMPTS, 8);
    assert_eq!(RECONCILIATION_BUDGET, Duration::from_secs(30));
}
