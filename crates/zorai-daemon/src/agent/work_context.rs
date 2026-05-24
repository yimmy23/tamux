//! Work context tracking — TODOs, file artifacts, repo watching, and event emission.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::*;

const RAPID_REVERT_WINDOW_MS: u64 = 30_000;

#[derive(Debug, Clone)]
struct RepoMonitorScope {
    include_roots: Vec<PathBuf>,
    exclude_roots: Vec<PathBuf>,
}

fn normalize_monitor_root(base_root: &Path, value: &str) -> Option<PathBuf> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let path = Path::new(trimmed);
    let resolved = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_root.join(path)
    };
    Some(std::fs::canonicalize(&resolved).unwrap_or(resolved))
}

fn monitored_change_matches(
    repo_root: &Path,
    relative_path: &str,
    scope: &RepoMonitorScope,
) -> bool {
    let absolute = repo_root.join(relative_path);
    let included = scope
        .include_roots
        .iter()
        .any(|root| absolute.starts_with(root));
    included
        && !scope
            .exclude_roots
            .iter()
            .any(|root| absolute.starts_with(root))
}

/// Map a `GoalRunStatus` to an event-kind string for autonomy-level filtering.
fn goal_run_status_to_event_kind(status: GoalRunStatus) -> &'static str {
    match status {
        GoalRunStatus::Completed => "completed",
        GoalRunStatus::Failed | GoalRunStatus::Cancelled => "failed",
        GoalRunStatus::Planning | GoalRunStatus::Queued => "planning",
        GoalRunStatus::Running => "step_started",
        GoalRunStatus::AwaitingApproval => "step_started",
        GoalRunStatus::Paused => "paused",
        // Break-glass and compensated outcomes are terminal-ish "completed"
        // for autonomy-level filtering; the audit trail distinguishes them.
        GoalRunStatus::Compensated | GoalRunStatus::BreakGlass => "completed",
        // Contained and PartiallyCompensated landed in degraded outcomes —
        // bucket with "failed" so autonomy filters surface them as needing
        // operator follow-up.
        GoalRunStatus::Contained | GoalRunStatus::PartiallyCompensated => "failed",
    }
}

/// Default operator-response window before an awaiting-approval task is
/// considered stale enough to escalate to the external (L3) tier. Matches
/// `EscalationCriteria::user_response_timeout_secs` (300s) — so the timeout
/// watcher in the heartbeat sees the same deadline the metacognitive
/// escalation criteria assumes when computing L2→L3 transitions.
pub(super) const APPROVAL_RESPONSE_TIMEOUT_MS: u64 = 300_000;

fn mark_task_waiting_for_approval(
    task: &mut AgentTask,
    thread_id: &str,
    pending_approval: &ToolPendingApproval,
    reason: String,
) {
    task.status = TaskStatus::AwaitingApproval;
    task.thread_id = Some(thread_id.to_string());
    if task.session_id.is_none() {
        task.session_id = pending_approval.session_id.clone();
    }
    task.awaiting_approval_id = Some(pending_approval.approval_id.clone());
    // Stamp the deadline the operator has to respond before the daemon
    // escalates this approval to the external (L3) tier. The heartbeat
    // timeout watcher compares this against `now_millis()` and fires a
    // critical inbox notification for any task past its deadline.
    task.approval_expires_at = Some(now_millis().saturating_add(APPROVAL_RESPONSE_TIMEOUT_MS));
    task.blocked_reason = Some(reason.clone());
    task.error = None;
    task.last_error = None;
    task.progress = task.progress.max(35);
    task.logs.push(make_task_log_entry(
        task.retry_count,
        TaskLogLevel::Warn,
        "approval",
        "managed command paused for operator approval",
        Some(reason),
    ));
}

impl AgentEngine {
    async fn resolve_repo_monitor_scope(&self, repo_root: &str) -> Option<RepoMonitorScope> {
        let repo_root =
            std::fs::canonicalize(repo_root).unwrap_or_else(|_| PathBuf::from(repo_root));
        let settings = self
            .history
            .list_repo_monitor_workspace_settings()
            .await
            .ok()?;
        let selected = settings
            .into_iter()
            .filter_map(|settings| {
                let workspace_root = settings
                    .workspace_root
                    .as_deref()
                    .map(PathBuf::from)
                    .unwrap_or_else(|| repo_root.clone());
                let normalized_workspace_root =
                    std::fs::canonicalize(&workspace_root).unwrap_or(workspace_root);
                (repo_root.starts_with(&normalized_workspace_root)
                    || normalized_workspace_root.starts_with(&repo_root)
                    || settings.workspace_id == "main")
                    .then_some((settings, normalized_workspace_root))
            })
            .max_by_key(|(_, workspace_root)| workspace_root.components().count());

        let (settings, workspace_root) = selected?;
        let include_roots = settings
            .repo_monitor_include_dirs
            .iter()
            .filter_map(|value| normalize_monitor_root(&workspace_root, value))
            .collect::<Vec<_>>();
        if include_roots.is_empty() {
            return None;
        }
        let exclude_roots = settings
            .repo_monitor_exclude_dirs
            .iter()
            .filter_map(|value| normalize_monitor_root(&workspace_root, value))
            .collect::<Vec<_>>();

        Some(RepoMonitorScope {
            include_roots,
            exclude_roots,
        })
    }

    pub(super) async fn repo_monitor_enabled_for_repo(&self, repo_root: &str) -> bool {
        self.resolve_repo_monitor_scope(repo_root).await.is_some()
    }

    pub(super) fn emit_task_update(&self, task: &AgentTask, message: Option<String>) {
        let _ = self.event_tx.send(AgentEvent::TaskUpdate {
            task_id: task.id.clone(),
            status: task.status,
            progress: task.progress,
            message,
            task: Some(task.clone()),
        });
    }

    pub(super) fn emit_goal_run_update(&self, goal_run: &GoalRun, message: Option<String>) {
        let event_kind = goal_run_status_to_event_kind(goal_run.status);
        if !super::autonomy::should_emit_event(goal_run.autonomy_level, event_kind) {
            return;
        }
        let _ = self.event_tx.send(AgentEvent::GoalRunUpdate {
            goal_run_id: goal_run.id.clone(),
            status: goal_run.status,
            current_step_index: Some(goal_run.current_step_index),
            message,
            goal_run: Some(goal_run.clone()),
        });
    }

    pub(super) fn emit_todo_update(
        &self,
        thread_id: &str,
        goal_run_id: Option<String>,
        step_index: Option<usize>,
        items: Vec<TodoItem>,
    ) {
        let _ = self.event_tx.send(AgentEvent::TodoUpdate {
            thread_id: thread_id.to_string(),
            goal_run_id,
            step_index,
            items,
        });
    }

    pub(super) fn emit_work_context_update(&self, thread_id: &str, context: ThreadWorkContext) {
        let _ = self.event_tx.send(AgentEvent::WorkContextUpdate {
            thread_id: thread_id.to_string(),
            context,
        });
    }

    pub(super) fn emit_workflow_notice(
        &self,
        thread_id: &str,
        kind: &str,
        message: impl Into<String>,
        details: Option<String>,
    ) {
        let _ = self.event_tx.send(AgentEvent::WorkflowNotice {
            thread_id: thread_id.to_string(),
            kind: kind.to_string(),
            message: message.into(),
            details,
        });
    }

    pub async fn replace_thread_todos(
        &self,
        thread_id: &str,
        mut items: Vec<TodoItem>,
        task_id: Option<&str>,
    ) {
        let now = now_millis();
        for (index, item) in items.iter_mut().enumerate() {
            item.position = index;
            if item.created_at == 0 {
                item.created_at = now;
            }
            item.updated_at = now;
        }

        {
            let mut todos = self.thread_todos.write().await;
            todos.insert(thread_id.to_string(), items.clone());
        }
        self.persist_todos().await;

        let mut goal_run_update = None;
        let mut goal_run_id = None;
        let mut step_index = None;
        if let Some(task_id) = task_id {
            if let Some(context) = self.goal_todo_context_for_task(task_id).await {
                if context.authoritative {
                    bind_goal_todo_items_to_step(&mut items, context.current_step_index);
                    {
                        let mut todos = self.thread_todos.write().await;
                        todos.insert(thread_id.to_string(), items.clone());
                    }
                    self.persist_todos().await;

                    goal_run_update = self.record_goal_run_todo_snapshot(task_id, &items).await;
                    goal_run_id = Some(context.goal_run_id);
                    step_index = Some(context.current_step_index);
                }
            }
        }

        self.emit_todo_update(thread_id, goal_run_id, step_index, items);

        if let Some(goal_run) = goal_run_update {
            self.persist_goal_runs().await;
            self.emit_goal_run_update(&goal_run, Some("Goal todo updated".into()));
        }
    }

    pub(super) async fn capture_tool_work_context(
        &self,
        thread_id: &str,
        task_id: Option<&str>,
        tool_name: &str,
        args_json: &str,
    ) {
        match tool_name {
            zorai_protocol::tool_names::CREATE_FILE
            | zorai_protocol::tool_names::WRITE_FILE
            | zorai_protocol::tool_names::APPEND_TO_FILE
            | zorai_protocol::tool_names::REPLACE_IN_FILE
            | zorai_protocol::tool_names::APPLY_FILE_PATCH
            | zorai_protocol::tool_names::APPLY_PATCH => {
                let Ok(args) = serde_json::from_str::<serde_json::Value>(args_json) else {
                    return;
                };
                if tool_name == zorai_protocol::tool_names::APPLY_PATCH {
                    if let Some(input) = super::tool_executor::get_apply_patch_text_arg(&args) {
                        if let Ok(paths) = super::tool_executor::extract_apply_patch_paths(input) {
                            for path in paths {
                                self.record_file_work_context(thread_id, task_id, tool_name, &path)
                                    .await;
                            }
                            return;
                        }
                    }
                }
                let Some(path) = args
                    .get("path")
                    .and_then(|value| value.as_str())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                else {
                    return;
                };
                self.record_file_work_context(thread_id, task_id, tool_name, path)
                    .await;
            }
            zorai_protocol::tool_names::RUN_TERMINAL_COMMAND
            | zorai_protocol::tool_names::RUN_BASH
            | zorai_protocol::tool_names::BASH_COMMAND => {
                self.refresh_thread_repo_context(thread_id).await;
            }
            _ => {}
        }
    }

    pub(super) async fn record_generated_skill_work_context(&self, goal_run: &GoalRun) {
        let Some(path) = goal_run.generated_skill_path.as_deref() else {
            return;
        };

        let context = ThreadWorkContext {
            thread_id: goal_run.thread_id.clone().unwrap_or_default(),
            entries: vec![WorkContextEntry {
                path: path.to_string(),
                previous_path: None,
                kind: WorkContextEntryKind::GeneratedSkill,
                source: "generated_skill".to_string(),
                change_kind: None,
                repo_root: crate::git::find_git_root(path),
                goal_run_id: Some(goal_run.id.clone()),
                step_index: Some(goal_run.current_step_index),
                session_id: goal_run.session_id.clone(),
                is_text: true,
                updated_at: now_millis(),
            }],
        };
        if context.thread_id.is_empty() {
            return;
        }
        self.merge_work_context_entries(&context.thread_id, context.entries)
            .await;
    }

    pub(super) async fn record_file_work_context(
        &self,
        thread_id: &str,
        task_id: Option<&str>,
        source: &str,
        path: &str,
    ) {
        let normalized = std::fs::canonicalize(path)
            .unwrap_or_else(|_| std::path::PathBuf::from(path))
            .to_string_lossy()
            .to_string();
        let repo_root = crate::git::find_git_root(&normalized);
        let (goal_run_id, step_index, session_id) = self.goal_context_for_task(task_id).await;
        let (entry_path, kind) = if let Some(repo_root) = repo_root.as_deref() {
            let relative = std::path::Path::new(&normalized)
                .strip_prefix(repo_root)
                .ok()
                .map(|value| value.to_string_lossy().trim_start_matches('/').to_string())
                .filter(|value| !value.is_empty())
                .unwrap_or_else(|| normalized.clone());
            (relative, WorkContextEntryKind::RepoChange)
        } else {
            (normalized.clone(), WorkContextEntryKind::Artifact)
        };

        self.merge_work_context_entries(
            thread_id,
            vec![WorkContextEntry {
                path: entry_path,
                previous_path: None,
                kind,
                source: source.to_string(),
                change_kind: None,
                repo_root,
                goal_run_id,
                step_index,
                session_id,
                is_text: true,
                updated_at: now_millis(),
            }],
        )
        .await;
        self.refresh_thread_repo_context(thread_id).await;
    }

    pub(super) async fn goal_context_for_task(
        &self,
        task_id: Option<&str>,
    ) -> (Option<String>, Option<usize>, Option<String>) {
        let Some(task_id) = task_id else {
            return (None, None, None);
        };

        let task = self.task_goal_context(task_id).await;
        let Some(task) = task else {
            return (None, None, None);
        };

        let goal_context = if let Some(goal_run_id) = task.goal_run_id.as_deref() {
            let memory_context = {
                let goal_runs = self.goal_runs.lock().await;
                goal_runs
                    .iter()
                    .find(|item| item.id == goal_run_id)
                    .map(|goal_run| crate::history::GoalRunTaskContextRef {
                        current_step_index: goal_run.current_step_index,
                        session_id: goal_run.session_id.clone(),
                    })
            };
            match memory_context {
                Some(context) => Some(context),
                None => match self.history.goal_run_task_context(goal_run_id).await {
                    Ok(context) => context,
                    Err(error) => {
                        tracing::warn!(
                            goal_run_id,
                            error = %error,
                            "failed to query goal context projection"
                        );
                        self.history
                            .get_goal_run(goal_run_id)
                            .await
                            .ok()
                            .flatten()
                            .map(|goal_run| crate::history::GoalRunTaskContextRef {
                                current_step_index: goal_run.current_step_index,
                                session_id: goal_run.session_id,
                            })
                    }
                },
            }
        } else {
            None
        };
        let step_index = goal_context.as_ref().map(|item| item.current_step_index);
        (
            task.goal_run_id.clone(),
            step_index,
            task.session_id
                .clone()
                .or_else(|| goal_context.and_then(|item| item.session_id)),
        )
    }

    pub(crate) async fn goal_todo_context_for_task(
        &self,
        task_id: &str,
    ) -> Option<GoalTodoContext> {
        let task = self.task_goal_context(task_id).await?;
        let goal_run_id = task.goal_run_id.clone()?;
        let memory_goal_run = {
            let goal_runs = self.goal_runs.lock().await;
            goal_runs
                .iter()
                .find(|goal_run| goal_run.id == goal_run_id)
                .cloned()
        };
        let task_goal_step_id = task.goal_step_id.clone();
        let (step_index, goal_step_id, step_status) = match memory_goal_run {
            Some(goal_run) => {
                let step_index = task_goal_step_id
                    .as_deref()
                    .and_then(|goal_step_id| {
                        goal_run
                            .steps
                            .iter()
                            .position(|step| step.id == goal_step_id)
                    })
                    .unwrap_or(goal_run.current_step_index);
                (
                    step_index,
                    task_goal_step_id
                        .or_else(|| goal_run.steps.get(step_index).map(|step| step.id.clone())),
                    goal_run.steps.get(step_index).map(|step| step.status),
                )
            }
            None => {
                let context = match self
                    .history
                    .goal_run_todo_context(&goal_run_id, task_goal_step_id.as_deref())
                    .await
                {
                    Ok(context) => context,
                    Err(error) => {
                        tracing::warn!(
                            goal_run_id,
                            error = %error,
                            "failed to query persisted goal todo context"
                        );
                        None
                    }
                }?;
                (
                    context.step_index,
                    task_goal_step_id.or(context.step_id),
                    context.step_status,
                )
            }
        };

        Some(GoalTodoContext {
            goal_run_id,
            goal_step_id,
            current_step_index: step_index,
            step_status,
            authoritative: task.source == "goal_run" && task.parent_task_id.is_none(),
        })
    }

    async fn task_goal_context(
        &self,
        task_id: &str,
    ) -> Option<crate::history::AgentTaskGoalContext> {
        match self.history.agent_task_goal_context(task_id).await {
            Ok(Some(context)) => Some(context),
            Ok(None) | Err(_) => {
                let tasks = self.tasks.lock().await;
                tasks.iter().find(|task| task.id == task_id).map(|task| {
                    crate::history::AgentTaskGoalContext {
                        goal_run_id: task.goal_run_id.clone(),
                        goal_step_id: task.goal_step_id.clone(),
                        session_id: task.session_id.clone(),
                        source: task.source.clone(),
                        parent_task_id: task.parent_task_id.clone(),
                    }
                })
            }
        }
    }

    pub(super) async fn resolve_thread_repo_root(
        &self,
        thread_id: &str,
    ) -> Option<(String, Option<String>, Option<String>, Option<usize>)> {
        let run = match self
            .history
            .latest_goal_run_repo_context_for_thread(thread_id)
            .await
        {
            Ok(Some(run)) => Some(run),
            Ok(None) | Err(_) => {
                let goal_runs = self.goal_runs.lock().await;
                goal_runs
                    .iter()
                    .filter(|run| run.thread_id.as_deref() == Some(thread_id))
                    .max_by_key(|run| run.updated_at)
                    .map(crate::history::GoalRunRepoContextRef::from)
            }
        };

        let session_id =
            if let Some(run_session_id) = run.as_ref().and_then(|item| item.session_id.clone()) {
                Some(run_session_id)
            } else {
                match self
                    .history
                    .latest_agent_task_session_for_thread(thread_id)
                    .await
                {
                    Ok(Some(session_id)) => Some(session_id),
                    Ok(None) | Err(_) => {
                        let tasks = self.tasks.lock().await;
                        tasks
                            .iter()
                            .filter(|task| task.thread_id.as_deref() == Some(thread_id))
                            .find_map(|task| task.session_id.clone())
                    }
                }
            };

        if let Some(session_id) = session_id.as_deref() {
            if let Some(cwd) = self
                .session_manager
                .list()
                .await
                .into_iter()
                .find(|session| session.id.to_string() == session_id)
                .and_then(|session| session.cwd)
            {
                if let Some(repo_root) = crate::git::find_git_root(&cwd) {
                    return Some((
                        repo_root,
                        run.as_ref().map(|item| item.id.clone()),
                        Some(session_id.to_string()),
                        run.as_ref().map(|item| item.current_step_index),
                    ));
                }
            }
        }

        let existing = self.thread_work_contexts.read().await;
        let repo_root = existing.get(thread_id).and_then(|context| {
            context
                .entries
                .iter()
                .find_map(|entry| entry.repo_root.clone())
        });
        let goal_run_id = run.as_ref().map(|item| item.id.clone());
        let step_index = run.as_ref().map(|item| item.current_step_index);
        repo_root.map(|root| (root, goal_run_id, session_id, step_index))
    }

    pub(super) async fn refresh_thread_repo_context(&self, thread_id: &str) {
        self.refresh_thread_repo_context_with_changes(thread_id, None)
            .await;
    }

    /// Variant that accepts a pre-fetched `(repo_root → git_changes)` cache.
    /// When `cached_changes` contains the resolved repo, the per-thread
    /// `crate::git::list_git_changes(&repo_root)` scan is skipped (which is
    /// the dominant cost — git status on a large repo is hundreds of ms).
    /// The per-thread state mutations (detect_reverts, merge_repo_scan)
    /// still run unchanged.
    pub(super) async fn refresh_thread_repo_context_with_changes(
        &self,
        thread_id: &str,
        cached_changes: Option<&std::collections::HashMap<String, Vec<zorai_protocol::GitChangeEntry>>>,
    ) {
        let Some((repo_root, goal_run_id, session_id, step_index)) =
            self.resolve_thread_repo_root(thread_id).await
        else {
            self.remove_repo_watcher(thread_id).await;
            return;
        };

        let monitor_scope = self.resolve_repo_monitor_scope(&repo_root).await;
        if monitor_scope.is_some() {
            self.ensure_repo_watcher(thread_id, &repo_root).await;
        } else {
            self.remove_repo_watcher(thread_id).await;
        }
        let repo_root_path = PathBuf::from(&repo_root);
        let all_changes: Vec<zorai_protocol::GitChangeEntry> = match cached_changes
            .and_then(|cache| cache.get(&repo_root))
        {
            Some(cached) => cached.clone(),
            None => crate::git::list_git_changes(&repo_root),
        };
        let now = now_millis();
        let make_entry = |entry: zorai_protocol::GitChangeEntry| WorkContextEntry {
            path: entry.path,
            previous_path: entry.previous_path,
            kind: WorkContextEntryKind::RepoChange,
            source: "repo_scan".to_string(),
            change_kind: Some(entry.kind),
            repo_root: Some(repo_root.clone()),
            goal_run_id: goal_run_id.clone(),
            step_index,
            session_id: session_id.clone(),
            is_text: true,
            updated_at: now,
        };
        let detection_entries: Vec<WorkContextEntry> =
            all_changes.iter().cloned().map(make_entry).collect();
        self.detect_and_record_rapid_reverts(thread_id, &repo_root, &detection_entries, now)
            .await;
        if let Some(scope) = monitor_scope.as_ref() {
            let merge_entries: Vec<WorkContextEntry> = all_changes
                .into_iter()
                .filter(|entry| {
                    monitored_change_matches(&repo_root_path, &entry.path, scope)
                        || entry
                            .previous_path
                            .as_deref()
                            .map(|path| monitored_change_matches(&repo_root_path, path, scope))
                            .unwrap_or(false)
                })
                .map(make_entry)
                .collect();
            self.merge_repo_scan_entries(thread_id, &repo_root, merge_entries)
                .await;
        }
        self.maybe_run_aline_startup_reconciliation_for_repo(&repo_root)
            .await;
    }

    async fn detect_and_record_rapid_reverts(
        &self,
        thread_id: &str,
        repo_root: &str,
        fresh_entries: &[WorkContextEntry],
        detected_at: u64,
    ) {
        let context = {
            let contexts = self.thread_work_contexts.read().await;
            contexts.get(thread_id).cloned()
        };
        let Some(context) = context else {
            return;
        };

        let changed_paths = fresh_entries
            .iter()
            .map(|entry| entry.path.as_str())
            .collect::<HashSet<_>>();

        let mut already_recorded = HashSet::new();
        for signal in self
            .history
            .list_implicit_signals_by_type(thread_id, "rapid_revert", 50)
            .await
            .unwrap_or_default()
        {
            let Some(snapshot) = signal.context_snapshot_json.as_deref() else {
                continue;
            };
            let Ok(snapshot) = serde_json::from_str::<serde_json::Value>(snapshot) else {
                continue;
            };
            if let Some(path) = snapshot.get("path").and_then(|value| value.as_str()) {
                already_recorded.insert(path.to_string());
            }
        }

        for entry in context.entries.iter().filter(|entry| {
            entry.kind == WorkContextEntryKind::RepoChange
                && entry.repo_root.as_deref() == Some(repo_root)
                && entry.source != "repo_scan"
        }) {
            if detected_at.saturating_sub(entry.updated_at) > RAPID_REVERT_WINDOW_MS {
                continue;
            }
            if changed_paths.contains(entry.path.as_str()) {
                continue;
            }
            if !already_recorded.insert(entry.path.clone()) {
                continue;
            }
            if let Err(error) = self
                .record_rapid_revert_feedback(
                    thread_id,
                    &entry.path,
                    &entry.source,
                    entry.repo_root.as_deref(),
                    entry.updated_at,
                    detected_at,
                )
                .await
            {
                tracing::warn!(
                    thread_id = %thread_id,
                    path = %entry.path,
                    error = %error,
                    "failed to record rapid revert implicit feedback"
                );
            }
        }
    }

    async fn merge_repo_scan_entries(
        &self,
        thread_id: &str,
        repo_root: &str,
        fresh_entries: Vec<WorkContextEntry>,
    ) {
        let mut contexts = self.thread_work_contexts.write().await;
        let context = contexts
            .entry(thread_id.to_string())
            .or_insert_with(|| ThreadWorkContext {
                thread_id: thread_id.to_string(),
                entries: Vec::new(),
            });

        context.entries.retain(|entry| {
            !(entry.repo_root.as_deref() == Some(repo_root) && entry.source == "repo_scan")
        });

        for fresh in fresh_entries {
            if let Some(existing) = context
                .entries
                .iter_mut()
                .find(|entry| entry.path == fresh.path && entry.repo_root == fresh.repo_root)
            {
                existing.change_kind = fresh.change_kind.clone();
                existing.previous_path = fresh.previous_path.clone();
                existing.updated_at = fresh.updated_at;
                if existing.source == "repo_scan" {
                    *existing = fresh.clone();
                }
            } else {
                context.entries.push(fresh);
            }
        }
        context
            .entries
            .sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        let snapshot = context.clone();
        drop(contexts);

        self.persist_work_context().await;
        self.emit_work_context_update(thread_id, snapshot);
    }

    async fn merge_work_context_entries(
        &self,
        thread_id: &str,
        fresh_entries: Vec<WorkContextEntry>,
    ) {
        let mut contexts = self.thread_work_contexts.write().await;
        let context = contexts
            .entry(thread_id.to_string())
            .or_insert_with(|| ThreadWorkContext {
                thread_id: thread_id.to_string(),
                entries: Vec::new(),
            });

        for fresh in fresh_entries {
            if let Some(existing) = context
                .entries
                .iter_mut()
                .find(|entry| entry.path == fresh.path && entry.repo_root == fresh.repo_root)
            {
                *existing = fresh;
            } else {
                context.entries.push(fresh);
            }
        }
        context
            .entries
            .sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        let snapshot = context.clone();
        drop(contexts);

        self.persist_work_context().await;
        self.emit_work_context_update(thread_id, snapshot);
    }

    pub(super) async fn record_goal_run_todo_snapshot(
        &self,
        task_id: &str,
        items: &[TodoItem],
    ) -> Option<GoalRun> {
        let goal_run_id = match self.history.agent_task_goal_context(task_id).await {
            Ok(Some(context)) => context.goal_run_id,
            Ok(None) => None,
            Err(error) => {
                tracing::warn!(
                    task_id,
                    %error,
                    "failed to query task goal context for todo snapshot"
                );
                self.list_tasks_filtered(&crate::history::AgentTaskListQuery {
                    id: Some(task_id.to_string()),
                    status: None,
                    statuses: Vec::new(),
                    source: None,
                    thread_id: None,
                    thread_ids: Vec::new(),
                    goal_run_id: None,
                    parent_task_id: None,
                    awaiting_approval_id: None,
                    supervisor_config_present: false,
                    exclude_terminal_statuses: false,
                    order_by_recent_activity_desc: false,
                    limit: Some(1),
                    ids: Vec::new(),
                    parent_task_ids: Vec::new(),
                })
                .await
                .into_iter()
                .next()
                .and_then(|task| task.goal_run_id)
            }
        }?;

        let needs_persisted_goal = {
            let goal_runs = self.goal_runs.lock().await;
            !goal_runs.iter().any(|goal_run| goal_run.id == goal_run_id)
        };
        let persisted_goal = if needs_persisted_goal {
            self.history.get_goal_run(&goal_run_id).await.ok().flatten()
        } else {
            None
        };

        let mut goal_runs = self.goal_runs.lock().await;
        if !goal_runs.iter().any(|goal_run| goal_run.id == goal_run_id) {
            if let Some(goal_run) = persisted_goal {
                goal_runs.push_back(goal_run);
            }
        }
        let goal_run = goal_runs
            .iter_mut()
            .find(|goal_run| goal_run.id == goal_run_id)?;
        goal_run.updated_at = now_millis();
        goal_run.events.push(make_goal_run_event_with_todos(
            "todo",
            "goal todo updated",
            None,
            Some(goal_run.current_step_index),
            items.to_vec(),
        ));
        Some(goal_run.clone())
    }

    pub(super) async fn mark_task_awaiting_approval(
        &self,
        task_id: &str,
        thread_id: &str,
        pending_approval: &ToolPendingApproval,
    ) {
        let reason = format!(
            "waiting for operator approval: {}",
            pending_approval.command
        );
        let live_updated = {
            let mut tasks = self.tasks.lock().await;
            tasks
                .iter_mut()
                .find(|entry| entry.id == task_id)
                .map(|task| {
                    mark_task_waiting_for_approval(
                        task,
                        thread_id,
                        pending_approval,
                        reason.clone(),
                    );
                    task.clone()
                })
        };
        let updated = if let Some(updated) = live_updated {
            self.persist_tasks().await;
            updated
        } else {
            let Some(mut task) = self
                .list_tasks_filtered(&crate::history::AgentTaskListQuery {
                    id: Some(task_id.to_string()),
                    status: None,
                    statuses: Vec::new(),
                    source: None,
                    thread_id: None,
                    thread_ids: Vec::new(),
                    goal_run_id: None,
                    parent_task_id: None,
                    awaiting_approval_id: None,
                    supervisor_config_present: false,
                    exclude_terminal_statuses: false,
                    order_by_recent_activity_desc: false,
                    limit: Some(1),
                    ids: Vec::new(),
                    parent_task_ids: Vec::new(),
                })
                .await
                .into_iter()
                .next()
            else {
                return;
            };
            mark_task_waiting_for_approval(&mut task, thread_id, pending_approval, reason);
            if let Err(error) = self.history.upsert_agent_task(&task).await {
                tracing::warn!(
                    task_id = %task.id,
                    %error,
                    "failed to persist task awaiting approval state"
                );
                return;
            }
            task
        };

        self.emit_task_update(&updated, Some("Task awaiting approval".into()));
        if let Some(thread_id) = updated.thread_id.as_deref() {
            let _ = self
                .maybe_send_gateway_thread_approval_request(thread_id, pending_approval)
                .await;
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct GoalTodoContext {
    pub(crate) goal_run_id: String,
    pub(crate) goal_step_id: Option<String>,
    pub(crate) current_step_index: usize,
    pub(crate) step_status: Option<GoalRunStepStatus>,
    pub(crate) authoritative: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_manager::SessionManager;
    use tempfile::tempdir;

    #[tokio::test]
    async fn replace_thread_todos_records_goal_snapshot_for_persisted_goal_after_live_queue_clear()
    {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;
        let goal_run = engine
            .start_goal_run(
                "Persisted todo snapshot goal".to_string(),
                Some("Persisted todo snapshot goal".to_string()),
                Some("thread-persisted-todo-snapshot".to_string()),
                None,
                None,
                None,
                None,
                None,
            )
            .await;
        let task = engine
            .enqueue_task(
                "Persisted todo snapshot task".to_string(),
                "Task owns authoritative goal todos.".to_string(),
                "normal",
                None,
                None,
                Vec::new(),
                None,
                "goal_run",
                Some(goal_run.id.clone()),
                None,
                goal_run.thread_id.clone(),
                Some("daemon".to_string()),
            )
            .await;
        engine.persist_tasks().await;
        engine.tasks.lock().await.clear();
        engine.goal_runs.lock().await.clear();

        engine
            .replace_thread_todos(
                "thread-persisted-todo-snapshot",
                vec![TodoItem {
                    id: "todo-persisted-goal".to_string(),
                    content: "Keep persisted goal todos authoritative".to_string(),
                    status: TodoStatus::InProgress,
                    position: 0,
                    step_index: None,
                    created_at: 0,
                    updated_at: 0,
                }],
                Some(&task.id),
            )
            .await;

        let persisted = engine
            .history
            .get_goal_run(&goal_run.id)
            .await
            .expect("goal query should succeed")
            .expect("goal should remain persisted");
        let todo_event = persisted
            .events
            .iter()
            .find(|event| event.phase == "todo")
            .expect("persisted goal should record a todo snapshot event");
        assert_eq!(todo_event.step_index, Some(goal_run.current_step_index));
        assert!(
            todo_event
                .todo_snapshot
                .iter()
                .any(|item| item.id == "todo-persisted-goal"
                    && item.step_index == Some(goal_run.current_step_index)),
            "todo snapshot should be bound to the active goal step"
        );
    }
}

fn bind_goal_todo_items_to_step(items: &mut [TodoItem], current_step_index: usize) {
    for item in items {
        item.step_index = Some(current_step_index);
    }
}
