use super::*;

mod projection;

fn projection_state_for_goal_run(goal_run: &GoalRun) -> GoalProjectionState {
    match goal_run.status {
        GoalRunStatus::Queued => GoalProjectionState::Pending,
        GoalRunStatus::Planning | GoalRunStatus::Running => GoalProjectionState::InProgress,
        GoalRunStatus::AwaitingApproval | GoalRunStatus::Paused => GoalProjectionState::Blocked,
        GoalRunStatus::Completed => GoalProjectionState::Completed,
        GoalRunStatus::Failed | GoalRunStatus::Cancelled => GoalProjectionState::Failed,
    }
}

fn projection_state_for_step(step: GoalRunStepStatus) -> GoalProjectionState {
    match step {
        GoalRunStepStatus::Pending => GoalProjectionState::Pending,
        GoalRunStepStatus::InProgress => GoalProjectionState::InProgress,
        GoalRunStepStatus::Completed => GoalProjectionState::Completed,
        GoalRunStepStatus::Failed | GoalRunStepStatus::Skipped => GoalProjectionState::Failed,
    }
}

fn execution_binding_for_step(step: &GoalRunStep) -> GoalRoleBinding {
    match &step.kind {
        GoalRunStepKind::Specialist(role) if !role.trim().is_empty() => {
            GoalRoleBinding::Subagent(role.clone())
        }
        _ => GoalRoleBinding::Builtin(crate::agent::agent_identity::MAIN_AGENT_ID.to_string()),
    }
}

fn summary_for_goal_run(goal_run: &GoalRun) -> String {
    goal_run
        .plan_summary
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| goal_run.goal.clone())
}

fn unit_from_step(step: &GoalRunStep) -> GoalDeliveryUnit {
    GoalDeliveryUnit {
        id: step.id.clone(),
        title: step.title.clone(),
        status: projection_state_for_step(step.status),
        execution_binding: execution_binding_for_step(step),
        verification_binding: GoalRoleBinding::Builtin(
            crate::agent::agent_identity::MAIN_AGENT_ID.to_string(),
        ),
        summary: step.summary.clone(),
        proof_checks: Vec::new(),
        evidence: Vec::new(),
        report: None,
    }
}

pub(super) fn refresh_goal_run_dossier(goal_run: &mut GoalRun) {
    let mut dossier = goal_run.dossier.clone().unwrap_or_default();
    dossier.units = goal_run.steps.iter().map(unit_from_step).collect();
    dossier.projection_state = projection_state_for_goal_run(goal_run);
    dossier.summary = Some(summary_for_goal_run(goal_run));
    dossier.projection_error = None;
    goal_run.dossier = Some(dossier);
}

pub(super) async fn write_goal_run_projection(
    engine: &AgentEngine,
    goal_run: &GoalRun,
) -> anyhow::Result<()> {
    projection::write_goal_projection_files(&engine.data_dir, goal_run).await
}

pub(super) async fn remove_goal_run_projection(
    engine: &AgentEngine,
    goal_run_id: &str,
) -> anyhow::Result<()> {
    projection::remove_goal_projection_dir(&engine.data_dir, goal_run_id).await
}

pub(super) async fn record_goal_projection_failure(
    engine: &AgentEngine,
    goal_run_id: &str,
    error: String,
) {
    let updated = {
        let mut goal_runs = engine.goal_runs.lock().await;
        let Some(goal_run) = goal_runs
            .iter_mut()
            .find(|goal_run| goal_run.id == goal_run_id)
        else {
            return;
        };

        let current_error = goal_run
            .dossier
            .as_ref()
            .and_then(|dossier| dossier.projection_error.as_deref())
            .map(str::to_string);
        if current_error.as_deref() == Some(error.as_str()) {
            return;
        }

        let mut dossier = goal_run.dossier.clone().unwrap_or_default();
        dossier.projection_state = projection_state_for_goal_run(goal_run);
        dossier.summary = Some(summary_for_goal_run(goal_run));
        dossier.projection_error = Some(error.clone());
        goal_run.dossier = Some(dossier);
        goal_run.updated_at = now_millis();
        goal_run.events.push(make_goal_run_event(
            "projection",
            "goal projection refresh failed",
            Some(error.clone()),
        ));
        goal_run.clone()
    };

    if let Err(persist_error) = engine.history.upsert_goal_run(&updated).await {
        tracing::warn!(
            goal_run_id = %goal_run_id,
            error = %persist_error,
            projection_error = %error,
            "failed to persist goal projection failure"
        );
    }
}
