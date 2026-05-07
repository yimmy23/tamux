//! Structured operational context summary for prompt injection.

use super::*;

impl AgentEngine {
    pub(super) async fn build_operational_context_summary(&self) -> Option<String> {
        let sessions = self.session_manager.list().await;
        let active_tasks = self
            .list_tasks_filtered(&crate::history::AgentTaskListQuery {
                id: None,
                status: None,
                statuses: Vec::new(),
                source: None,
                thread_id: None,
                thread_ids: Vec::new(),
                goal_run_id: None,
                parent_task_id: None,
                awaiting_approval_id: None,
                supervisor_config_present: false,
                exclude_terminal_statuses: true,
                order_by_recent_activity_desc: false,
                limit: Some(4),
            })
            .await;
        let active_goals = {
            let goal_runs = self.goal_runs.lock().await;
            goal_runs
                .iter()
                .filter(|goal| {
                    !matches!(
                        goal.status,
                        GoalRunStatus::Completed | GoalRunStatus::Failed | GoalRunStatus::Cancelled
                    )
                })
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
        };

        let topology_summary = self
            .session_manager
            .read_workspace_topology()
            .map(|topology| {
                let surface_count = topology
                    .workspaces
                    .iter()
                    .map(|workspace| workspace.surfaces.len())
                    .sum::<usize>();
                let pane_count = topology
                    .workspaces
                    .iter()
                    .flat_map(|workspace| workspace.surfaces.iter())
                    .map(|surface| surface.panes.len())
                    .sum::<usize>();
                format!("{surface_count} surface(s), {pane_count} pane(s)")
            });

        if sessions.is_empty()
            && active_tasks.is_empty()
            && active_goals.is_empty()
            && topology_summary.is_none()
        {
            return None;
        }

        let mut lines = Vec::new();
        lines.push("## Operational Context".to_string());
        lines.push(format!("- Active sessions: {}", sessions.len()));
        for session in sessions.iter().take(3) {
            let cwd = session.cwd.as_deref().unwrap_or("?");
            let cmd = session.active_command.as_deref().unwrap_or("idle");
            lines.push(format!("- Session {} cwd={} cmd={}", session.id, cwd, cmd));
        }

        let pending_approvals = active_tasks
            .iter()
            .filter(|task| task.awaiting_approval_id.is_some())
            .count();
        lines.push(format!(
            "- Active tasks: {} ({} awaiting approval)",
            active_tasks.len(),
            pending_approvals
        ));
        for task in &active_tasks {
            lines.push(format!(
                "- Task [{}] {} — {}%",
                task_status_label(task.status),
                task.title,
                task.progress
            ));
        }

        lines.push(format!("- Active goal runs: {}", active_goals.len()));
        for goal in &active_goals {
            lines.push(format!(
                "- Goal [{}] {} — step {}/{}",
                goal_run_status_label(goal.status),
                goal.title,
                goal.current_step_index.saturating_add(1),
                goal.steps.len().max(1)
            ));
        }

        if let Some(topology_summary) = topology_summary {
            lines.push(format!("- Workspace topology: {topology_summary}"));
        }

        Some(lines.join("\n"))
    }
}

fn task_status_label(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Queued => "queued",
        TaskStatus::InProgress => "running",
        TaskStatus::AwaitingApproval => "awaiting-approval",
        TaskStatus::BudgetExceeded => "budget-exceeded",
        TaskStatus::Completed => "completed",
        TaskStatus::Failed => "failed",
        TaskStatus::Blocked => "blocked",
        TaskStatus::FailedAnalyzing => "failed-analyzing",
        TaskStatus::Cancelled => "cancelled",
    }
}

fn goal_run_status_label(status: GoalRunStatus) -> &'static str {
    match status {
        GoalRunStatus::Queued => "queued",
        GoalRunStatus::Planning => "planning",
        GoalRunStatus::Running => "running",
        GoalRunStatus::AwaitingApproval => "awaiting-approval",
        GoalRunStatus::Paused => "paused",
        GoalRunStatus::Completed => "completed",
        GoalRunStatus::Failed => "failed",
        GoalRunStatus::Cancelled => "cancelled",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn operational_context_includes_persisted_active_tasks_after_live_queue_clear() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

        engine
            .enqueue_task(
                "Persisted visible task".to_string(),
                "task should appear in operational context".to_string(),
                "normal",
                None,
                None,
                Vec::new(),
                None,
                "test",
                None,
                None,
                Some("thread-operational-context".to_string()),
                None,
            )
            .await;
        engine.persist_tasks().await;
        engine.tasks.lock().await.clear();

        let summary = engine
            .build_operational_context_summary()
            .await
            .expect("active persisted task should produce operational context");

        assert!(summary.contains("- Active tasks: 1"));
        assert!(summary.contains("- Task [queued] Persisted visible task"));
    }
}
