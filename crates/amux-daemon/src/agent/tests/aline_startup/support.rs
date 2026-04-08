use super::*;

use crate::agent::{
    AgentConfig, AgentEngine, ThreadWorkContext, WorkContextEntry, WorkContextEntryKind,
};
use crate::session_manager::SessionManager;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::watch;
use tokio::time::{timeout, Duration as TokioDuration};

pub(super) const SAMPLE_JSON: &str = r#"
{
    "page": 1,
    "per_page": 30,
    "total_pages": 1,
  "sessions": [
    {
      "status": "new",
      "source": "codex",
      "project_name": "cmux-next",
      "session_id": "rollout-2026-04-07-001",
            "created_at": "2026-04-07T09:15:00.000000",
            "last_activity": "2026-04-07T10:15:00.000000",
      "session_file": "/tmp/aline/cmux-next/session-1.json"
    }
  ]
}
"#;

pub(super) const FULL_CURRENT_CLI_PAYLOAD_JSON: &str = r#"
{
    "total": 1,
    "page": 1,
    "per_page": 30,
    "total_pages": 1,
  "sessions": [
    {
      "status": "new",
      "source": "codex",
      "project_name": "cmux-next",
      "session_id": "rollout-2026-04-07-002",
            "created_at": "2026-04-07T09:00:00.000000",
            "last_activity": "2026-04-07T11:45:00.000000",
      "session_file": "/home/mkurman/.codex/sessions/2026/04/07/rollout-2026-04-07-002.jsonl",
      "project_path": "/home/mkurman/gitlab/it/cmux-next",
      "branch": "main",
      "message_count": 42,
      "tracked": false,
      "watcher_state": "new",
            "updated_at": "2026-04-07T11:45:00.000000",
      "title": "Aline startup candidate"
        }
    ]
}
"#;

pub(super) const FULL_SESSION_ID: &str = "rollout-2026-04-07-abc123-full-id";

pub(super) fn ts(input: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(input)
        .expect("valid RFC3339 timestamp")
        .with_timezone(&Utc)
}

pub(super) fn session(
    status: &str,
    source: &str,
    project_name: &str,
    project_path: Option<&str>,
    session_id: &str,
    created_at: &str,
    last_activity: &str,
) -> AlineDiscoveredSession {
    AlineDiscoveredSession {
        status: status.to_string(),
        source: source.to_string(),
        project_name: project_name.to_string(),
        project_path: project_path.map(str::to_string),
        session_id: session_id.to_string(),
        created_at: created_at.to_string(),
        last_activity: last_activity.to_string(),
        session_file: format!("/tmp/aline/{project_name}/{session_id}.json"),
    }
}

#[derive(Debug, Clone)]
pub(super) struct RecordedSpec {
    pub(super) program: String,
    pub(super) args: Vec<String>,
    pub(super) timeout: Duration,
}

#[derive(Debug, Clone)]
pub(super) struct StubRunner {
    inner: Arc<Mutex<StubRunnerState>>,
}

#[derive(Debug)]
pub(super) struct StubCommandResponse {
    pub(super) output: Result<StartupCommandOutput>,
    pub(super) delay: Duration,
}

#[derive(Debug)]
struct StubRunnerState {
    outputs: VecDeque<StubCommandResponse>,
    specs: Vec<RecordedSpec>,
}

impl StubRunner {
    pub(super) fn with_outputs(outputs: Vec<Result<StartupCommandOutput>>) -> Self {
        let responses = outputs
            .into_iter()
            .map(|output| StubCommandResponse {
                output,
                delay: Duration::ZERO,
            })
            .collect();
        Self::with_responses(responses)
    }

    pub(super) fn with_responses(responses: Vec<StubCommandResponse>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(StubRunnerState {
                outputs: responses.into(),
                specs: Vec::new(),
            })),
        }
    }

    pub(super) fn recorded_specs(&self) -> Vec<RecordedSpec> {
        self.inner
            .lock()
            .expect("stub runner mutex should not be poisoned")
            .specs
            .clone()
    }
}

#[async_trait]
impl StartupCommandRunner for StubRunner {
    async fn run(&self, spec: StartupCommandSpec) -> Result<StartupCommandOutput> {
        let response = {
            let mut state = self
                .inner
                .lock()
                .expect("stub runner mutex should not be poisoned");
            state.specs.push(RecordedSpec {
                program: spec.program,
                args: spec.args,
                timeout: spec.timeout,
            });
            state.outputs.pop_front()
        }
        .unwrap_or_else(|| StubCommandResponse {
            output: Err(anyhow!("unexpected startup command")),
            delay: Duration::ZERO,
        });

        if !response.delay.is_zero() {
            tokio::time::sleep(response.delay).await;
        }

        response.output
    }
}

pub(super) fn delayed_output(
    output: Result<StartupCommandOutput>,
    delay: Duration,
) -> StubCommandResponse {
    StubCommandResponse { output, delay }
}

pub(super) fn session_list_output(json: &str) -> Result<StartupCommandOutput> {
    Ok(StartupCommandOutput {
        stdout: json.to_string(),
        stderr: String::new(),
        exit_code: 0,
    })
}

pub(super) fn command_output(stdout: &str) -> Result<StartupCommandOutput> {
    Ok(StartupCommandOutput {
        stdout: stdout.to_string(),
        stderr: String::new(),
        exit_code: 0,
    })
}

pub(super) fn watcher_status_output(state: &str) -> Result<StartupCommandOutput> {
    command_output(&format!(
        "Watcher Status: {state}\nMode: Standalone (SQLite)\n"
    ))
}

pub(super) fn import_success_output() -> Result<StartupCommandOutput> {
    command_output("Imported 1 session(s).\n")
}

pub(super) fn session_show_output() -> Result<StartupCommandOutput> {
    command_output("{\"ok\":true}\n")
}

pub(super) fn session_show_missing_output() -> Result<StartupCommandOutput> {
    Ok(StartupCommandOutput {
        stdout: String::new(),
        stderr: "session not found".to_string(),
        exit_code: 1,
    })
}

pub(super) fn single_session_list_output(status: &str) -> Result<StartupCommandOutput> {
    session_list_output(&format!(
        r#"{{
    "page": 1,
    "per_page": 30,
    "total_pages": 1,
    "sessions": [
        {{
            "status": "{status}",
            "source": "codex",
            "project_name": "cmux-next",
            "session_id": "{FULL_SESSION_ID}",
            "created_at": "2026-04-07T09:00:00.000000",
            "last_activity": "2026-04-07T11:59:00.000000",
            "session_file": "/tmp/aline/cmux-next/{FULL_SESSION_ID}.json"
        }}
    ]
}}"#
    ))
}

pub(super) fn session_list_with_sessions(
    has_more: bool,
    sessions: &[AlineDiscoveredSession],
) -> Result<StartupCommandOutput> {
    let sessions_json = sessions
        .iter()
        .map(|session| {
            serde_json::json!({
                "status": session.status,
                "source": session.source,
                "project_name": session.project_name,
                "session_id": session.session_id,
                "created_at": session.created_at,
                "last_activity": session.last_activity,
                "session_file": session.session_file,
            })
        })
        .collect::<Vec<_>>();
    session_list_output(
        &serde_json::to_string(&serde_json::json!({
            "has_more": has_more,
            "sessions": sessions_json,
        }))
        .expect("serialize session list fixture"),
    )
}

pub(super) struct AlineStartupHarness {
    pub(super) engine: Arc<AgentEngine>,
    root: TempDir,
    completion_rx: watch::Receiver<bool>,
    runner: StubRunner,
}

impl AlineStartupHarness {
    pub(super) fn root_path(&self) -> &std::path::Path {
        self.root.path()
    }

    pub(super) async fn persist_repo_roots(&self, repo_roots: &[&str]) {
        self.persist_repo_roots_with_updated_at(
            &repo_roots
                .iter()
                .enumerate()
                .map(|(index, repo_root)| (*repo_root, index as u64))
                .collect::<Vec<_>>(),
        )
        .await;
    }

    pub(super) async fn persist_repo_roots_with_updated_at(&self, repo_roots: &[(&str, u64)]) {
        let contexts = repo_roots
            .iter()
            .enumerate()
            .map(|(index, (repo_root, updated_at))| {
                let thread_id = format!("thread-{index}");
                (
                    thread_id.clone(),
                    ThreadWorkContext {
                        thread_id,
                        entries: vec![WorkContextEntry {
                            path: format!("tracked-{index}.rs"),
                            previous_path: None,
                            kind: WorkContextEntryKind::RepoChange,
                            source: "startup_test".to_string(),
                            change_kind: None,
                            repo_root: Some((*repo_root).to_string()),
                            goal_run_id: None,
                            step_index: None,
                            session_id: None,
                            is_text: true,
                            updated_at: *updated_at,
                        }],
                    },
                )
            })
            .collect::<HashMap<_, _>>();
        let raw = serde_json::to_string_pretty(&contexts).expect("serialize work contexts");
        tokio::fs::write(self.engine.data_dir.join("work-context.json"), raw)
            .await
            .expect("write work-context.json");
    }

    pub(super) fn create_git_repo(&self, name: &str) -> String {
        let repo_dir = self.root.path().join(name);
        std::fs::create_dir_all(&repo_dir).expect("create repo dir");
        run_git(&repo_dir, &["init"]);
        repo_dir
            .to_str()
            .expect("repo path should be valid utf-8")
            .to_string()
    }

    pub(super) fn create_git_worktree_pair(&self, name: &str) -> (String, String) {
        let repo_dir = self.root.path().join(format!("{name}-main"));
        std::fs::create_dir_all(&repo_dir).expect("create main repo dir");
        run_git(&repo_dir, &["init"]);
        run_git(&repo_dir, &["config", "user.name", "Aline Startup Tests"]);
        run_git(
            &repo_dir,
            &["config", "user.email", "aline-startup-tests@example.com"],
        );
        std::fs::write(repo_dir.join("README.md"), "fixture\n").expect("write fixture file");
        run_git(&repo_dir, &["add", "README.md"]);
        run_git(&repo_dir, &["commit", "-m", "initial fixture"]);

        let worktree_dir = repo_dir.join(".worktrees").join("startup-test-worktree");
        std::fs::create_dir_all(worktree_dir.parent().expect("worktree parent should exist"))
            .expect("create worktree parent dir");
        run_git(
            &repo_dir,
            &[
                "worktree",
                "add",
                "-b",
                "startup-test-worktree",
                worktree_dir
                    .to_str()
                    .expect("worktree path should be valid utf-8"),
            ],
        );

        (
            repo_dir
                .to_str()
                .expect("repo path should be valid utf-8")
                .to_string(),
            worktree_dir
                .to_str()
                .expect("worktree path should be valid utf-8")
                .to_string(),
        )
    }

    pub(super) fn create_git_sibling_worktree_pair(&self, name: &str) -> (String, String) {
        let repo_dir = self.root.path().join(format!("{name}-main"));
        std::fs::create_dir_all(&repo_dir).expect("create main repo dir");
        run_git(&repo_dir, &["init"]);
        run_git(&repo_dir, &["config", "user.name", "Aline Startup Tests"]);
        run_git(
            &repo_dir,
            &["config", "user.email", "aline-startup-tests@example.com"],
        );
        std::fs::write(repo_dir.join("README.md"), "fixture\n").expect("write fixture file");
        run_git(&repo_dir, &["add", "README.md"]);
        run_git(&repo_dir, &["commit", "-m", "initial fixture"]);

        let worktree_dir = self.root.path().join(format!("{name}-sibling-worktree"));
        run_git(
            &repo_dir,
            &[
                "worktree",
                "add",
                "-b",
                "startup-test-sibling-worktree",
                worktree_dir
                    .to_str()
                    .expect("worktree path should be valid utf-8"),
            ],
        );

        (
            repo_dir
                .to_str()
                .expect("repo path should be valid utf-8")
                .to_string(),
            worktree_dir
                .to_str()
                .expect("worktree path should be valid utf-8")
                .to_string(),
        )
    }

    pub(super) async fn spawn_live_session(&self, cwd: &str) {
        self.engine
            .session_manager
            .spawn(
                Some(test_shell().to_string()),
                Some(cwd.to_string()),
                None,
                None,
                80,
                24,
            )
            .await
            .expect("spawn live session");
    }

    pub(super) fn recorded_commands(&self) -> Vec<Vec<String>> {
        self.runner
            .recorded_specs()
            .into_iter()
            .map(|spec| spec.args)
            .collect()
    }

    pub(super) async fn wait_for_reconciliation(&mut self) {
        if *self.completion_rx.borrow() {
            return;
        }
        timeout(TokioDuration::from_secs(2), self.completion_rx.changed())
            .await
            .expect("startup reconciliation completion should be signaled promptly")
            .expect("startup reconciliation watch channel should remain open");
    }

    pub(super) fn reconciliation_finished(&self) -> bool {
        *self.completion_rx.borrow()
    }

    pub(super) async fn scheduled_repo_roots(&self) -> Vec<String> {
        self.engine
            .aline_startup_repo_roots_for_tests()
            .await
            .into_iter()
            .map(|path| path.to_string_lossy().to_string())
            .collect()
    }
}

pub(super) async fn make_aline_startup_harness() -> AlineStartupHarness {
    make_aline_startup_harness_with_runner(false, Vec::new()).await
}

pub(super) async fn make_aline_startup_harness_with_runner(
    aline_available: bool,
    outputs: Vec<Result<StartupCommandOutput>>,
) -> AlineStartupHarness {
    let responses = outputs
        .into_iter()
        .map(|output| StubCommandResponse {
            output,
            delay: Duration::ZERO,
        })
        .collect();
    make_aline_startup_harness_with_responses(aline_available, responses).await
}

pub(super) async fn make_aline_startup_harness_with_responses(
    aline_available: bool,
    responses: Vec<StubCommandResponse>,
) -> AlineStartupHarness {
    let root = TempDir::new().expect("tempdir");
    let manager = SessionManager::new_test(root.path()).await;
    let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
    let runner = StubRunner::with_responses(responses);
    engine.set_aline_startup_test_availability(aline_available);
    engine.set_aline_startup_test_runner(Arc::new(runner.clone()));
    let completion_rx = engine.install_aline_startup_test_completion();

    AlineStartupHarness {
        engine,
        root,
        completion_rx,
        runner,
    }
}

#[cfg(windows)]
fn test_shell() -> &'static str {
    "cmd.exe"
}

#[cfg(not(windows))]
fn test_shell() -> &'static str {
    "/bin/sh"
}

fn run_git(repo_dir: &std::path::Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(repo_dir)
        .output()
        .expect("git command should spawn");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}
