//! Structured operational context summary for prompt injection.

use super::*;

impl AgentEngine {
    pub(super) async fn build_operational_context_summary(&self) -> Option<String> {
        let sessions = self.session_manager.list().await;
        let active_task_query = crate::history::AgentTaskListQuery {
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
            ids: Vec::new(),
            parent_task_ids: Vec::new(),
        };
        let active_tasks = match self
            .history
            .list_agent_task_operational_refs_filtered(&active_task_query)
            .await
        {
            Ok(task_refs) => task_refs,
            Err(error) => {
                tracing::warn!(
                    "failed to query persisted active task refs for operational context: {error}"
                );
                self.list_tasks_filtered(&active_task_query)
                    .await
                    .iter()
                    .map(crate::history::AgentTaskOperationalRef::from)
                    .collect()
            }
        };
        let active_goal_statuses = [
            GoalRunStatus::Queued,
            GoalRunStatus::Planning,
            GoalRunStatus::Running,
            GoalRunStatus::AwaitingApproval,
            GoalRunStatus::Paused,
        ];
        let mut active_goals = match self
            .history
            .list_goal_run_operational_refs_for_statuses_limited(&active_goal_statuses, Some(3))
            .await
        {
            Ok(goal_refs) => goal_refs,
            Err(error) => {
                tracing::warn!(
                    "failed to query persisted active goal refs for operational context: {error}"
                );
                self.history
                    .list_goal_runs_for_statuses_limited(&active_goal_statuses, Some(3))
                    .await
                    .map(|goals| {
                        goals
                            .iter()
                            .map(crate::history::GoalRunOperationalRef::from)
                            .collect()
                    })
                    .unwrap_or_default()
            }
        };
        let mut seen_goal_ids = active_goals
            .iter()
            .map(|goal_run| goal_run.id.clone())
            .collect::<std::collections::HashSet<_>>();
        {
            let goal_runs = self.goal_runs.lock().await;
            for goal_run in goal_runs
                .iter()
                .filter(|goal| active_goal_statuses.contains(&goal.status))
            {
                if seen_goal_ids.insert(goal_run.id.clone()) {
                    active_goals.push(crate::history::GoalRunOperationalRef::from(goal_run));
                }
            }
        }
        active_goals.truncate(3);

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
                goal.step_count.max(1)
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
        GoalRunStatus::Contained => "contained",
        GoalRunStatus::Compensated => "compensated",
        GoalRunStatus::PartiallyCompensated => "partially-compensated",
        GoalRunStatus::BreakGlass => "break-glass",
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

    #[tokio::test]
    async fn operational_context_includes_persisted_active_goals_after_live_queue_clear() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

        engine
            .start_goal_run(
                "goal should appear in operational context".to_string(),
                Some("Persisted visible goal".to_string()),
                None,
                None,
                Some("normal"),
                None,
                None,
                None,
            )
            .await;
        engine.goal_runs.lock().await.clear();

        let summary = engine
            .build_operational_context_summary()
            .await
            .expect("active persisted goal should produce operational context");

        assert!(summary.contains("- Active goal runs: 1"));
        assert!(summary.contains("- Goal [queued] Persisted visible goal"));
    }
}
