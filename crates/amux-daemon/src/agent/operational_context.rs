//! Structured operational context summary for prompt injection.

use super::*;

impl AgentEngine {
    pub(super) async fn build_operational_context_summary(&self) -> Option<String> {
        let sessions = self.session_manager.list().await;
        let active_tasks = {
            let tasks = self.tasks.lock().await;
            tasks
                .iter()
                .filter(|task| {
                    !matches!(
                        task.status,
                        TaskStatus::Completed
                            | TaskStatus::BudgetExceeded
                            | TaskStatus::Failed
                            | TaskStatus::Cancelled
                    )
                })
                .take(4)
                .cloned()
                .collect::<Vec<_>>()
        };
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
