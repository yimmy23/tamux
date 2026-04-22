use super::*;
use std::collections::HashMap;

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

fn default_unit_from_step(step: &GoalRunStep) -> GoalDeliveryUnit {
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

fn goal_role_binding_label(binding: &GoalRoleBinding) -> String {
    match binding {
        GoalRoleBinding::Builtin(value) => format!("builtin:{value}"),
        GoalRoleBinding::Subagent(value) => format!("subagent:{value}"),
    }
}

fn ensure_dossier_unit<'a>(
    goal_run: &'a mut GoalRun,
    step_id: &str,
) -> Option<&'a mut GoalDeliveryUnit> {
    let step = goal_run
        .steps
        .iter()
        .find(|step| step.id == step_id)
        .cloned()?;
    let projection_state = projection_state_for_goal_run(goal_run);
    let summary = summary_for_goal_run(goal_run);
    let dossier = goal_run.dossier.get_or_insert_with(|| GoalRunDossier {
        projection_state,
        summary: Some(summary),
        ..Default::default()
    });

    if !dossier.units.iter().any(|unit| unit.id == step_id) {
        dossier.units.push(default_unit_from_step(&step));
    }

    dossier.units.iter_mut().find(|unit| unit.id == step_id)
}

pub(super) fn set_goal_resume_decision(
    goal_run: &mut GoalRun,
    action: GoalResumeAction,
    reason_code: impl Into<String>,
    reason: Option<String>,
    details: Vec<String>,
) {
    let projection_state = projection_state_for_goal_run(goal_run);
    let summary = summary_for_goal_run(goal_run);
    let dossier = goal_run.dossier.get_or_insert_with(|| GoalRunDossier {
        projection_state,
        summary: Some(summary),
        ..Default::default()
    });
    dossier.latest_resume_decision = Some(GoalResumeDecision {
        action,
        reason_code: reason_code.into(),
        reason,
        details,
        decided_at: Some(now_millis()),
        projection_state,
    });
}

pub(super) fn set_goal_report(
    goal_run: &mut GoalRun,
    state: GoalProjectionState,
    summary: impl Into<String>,
    notes: Vec<String>,
) {
    let projection_state = projection_state_for_goal_run(goal_run);
    let dossier_summary = summary_for_goal_run(goal_run);
    let dossier = goal_run.dossier.get_or_insert_with(|| GoalRunDossier {
        projection_state,
        summary: Some(dossier_summary),
        ..Default::default()
    });
    dossier.report = Some(GoalRunReport {
        summary: summary.into(),
        state,
        notes,
        evidence: Vec::new(),
        proof_checks: Vec::new(),
        generated_at: Some(now_millis()),
    });
}

pub(super) fn set_goal_unit_report(
    goal_run: &mut GoalRun,
    step_id: &str,
    state: GoalProjectionState,
    summary: impl Into<String>,
    mut notes: Vec<String>,
) {
    let Some(unit) = ensure_dossier_unit(goal_run, step_id) else {
        return;
    };
    let summary = summary.into();
    let execution_binding = format!(
        "execution binding: {}",
        goal_role_binding_label(&unit.execution_binding)
    );
    if !notes.iter().any(|note| note == &execution_binding) {
        notes.push(execution_binding);
    }
    let verification_binding = format!(
        "verification binding: {}",
        goal_role_binding_label(&unit.verification_binding)
    );
    if !notes.iter().any(|note| note == &verification_binding) {
        notes.push(verification_binding);
    }
    unit.status = state;
    unit.summary = Some(summary.clone());
    unit.report = Some(GoalRunReport {
        summary,
        state,
        notes,
        evidence: unit.evidence.clone(),
        proof_checks: unit.proof_checks.clone(),
        generated_at: Some(now_millis()),
    });
}

pub(super) fn set_goal_unit_verification_state(
    goal_run: &mut GoalRun,
    step_id: &str,
    state: GoalProjectionState,
    summary: impl Into<String>,
    notes: Vec<String>,
    evidence_title: Option<&str>,
    evidence_summary: Option<String>,
) {
    let Some(unit) = ensure_dossier_unit(goal_run, step_id) else {
        return;
    };
    let summary = summary.into();
    let now = now_millis();
    let evidence_id = match (evidence_title, evidence_summary) {
        (Some(title), Some(evidence_summary)) => {
            let record = GoalEvidenceRecord {
                id: format!("goal_evidence_{}", Uuid::new_v4()),
                title: title.to_string(),
                source: Some("goal_verification".to_string()),
                uri: None,
                summary: Some(evidence_summary),
                captured_at: Some(now),
            };
            let evidence_id = record.id.clone();
            unit.evidence.push(record);
            Some(evidence_id)
        }
        _ => None,
    };

    for proof_check in &mut unit.proof_checks {
        proof_check.state = state;
        proof_check.summary = Some(summary.clone());
        proof_check.resolved_at = match state {
            GoalProjectionState::Completed | GoalProjectionState::Failed => Some(now),
            _ => None,
        };
        if let Some(evidence_id) = evidence_id.as_ref() {
            if !proof_check.evidence_ids.iter().any(|id| id == evidence_id) {
                proof_check.evidence_ids.push(evidence_id.clone());
            }
        }
    }

    set_goal_unit_report(goal_run, step_id, state, summary, notes);
}

pub(super) fn refresh_goal_run_dossier(goal_run: &mut GoalRun) {
    let mut dossier = goal_run.dossier.clone().unwrap_or_default();
    let existing_units = dossier
        .units
        .iter()
        .cloned()
        .map(|unit| (unit.id.clone(), unit))
        .collect::<HashMap<_, _>>();
    dossier.units = goal_run
        .steps
        .iter()
        .map(|step| {
            let mut unit = existing_units
                .get(&step.id)
                .cloned()
                .unwrap_or_else(|| default_unit_from_step(step));
            unit.id = step.id.clone();
            unit.title = step.title.clone();
            unit.status = projection_state_for_step(step.status);
            unit.summary = step.summary.clone().or(unit.summary);
            unit
        })
        .collect();
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

pub(crate) fn goal_inventory_prompt_block(data_dir: &std::path::Path, goal_run_id: &str) -> String {
    let inventory_root = projection::goal_inventory_dir(data_dir, goal_run_id);
    let specs_dir = projection::goal_inventory_specs_dir(data_dir, goal_run_id);
    let plans_dir = projection::goal_inventory_plans_dir(data_dir, goal_run_id);
    let execution_dir = projection::goal_inventory_execution_dir(data_dir, goal_run_id);

    format!(
        "## Goal Artifact Inventory\n\
         - inventory root: {inventory_root}/\n\
         - specs dir: {specs_dir}/\n\
         - plans dir: {plans_dir}/\n\
         - execution dir: {execution_dir}/\n\
         - Place durable goal-run artifacts only in this inventory subtree.\n\
         - Default ambiguous artifacts to the execution dir.",
        inventory_root = inventory_root.display(),
        specs_dir = specs_dir.display(),
        plans_dir = plans_dir.display(),
        execution_dir = execution_dir.display(),
    )
}

pub(crate) fn goal_step_completion_marker_relative_path(
    goal_run_id: &str,
    step_index: usize,
) -> std::path::PathBuf {
    projection::goal_step_completion_marker_relative_path(goal_run_id, step_index)
}

pub(crate) fn goal_inventory_dir(
    data_dir: &std::path::Path,
    goal_run_id: &str,
) -> std::path::PathBuf {
    projection::goal_inventory_dir(data_dir, goal_run_id)
}

pub(crate) fn goal_inventory_specs_dir(
    data_dir: &std::path::Path,
    goal_run_id: &str,
) -> std::path::PathBuf {
    projection::goal_inventory_specs_dir(data_dir, goal_run_id)
}

pub(crate) fn goal_inventory_plans_dir(
    data_dir: &std::path::Path,
    goal_run_id: &str,
) -> std::path::PathBuf {
    projection::goal_inventory_plans_dir(data_dir, goal_run_id)
}

pub(crate) fn goal_inventory_execution_dir(
    data_dir: &std::path::Path,
    goal_run_id: &str,
) -> std::path::PathBuf {
    projection::goal_inventory_execution_dir(data_dir, goal_run_id)
}

pub(crate) fn goal_step_completion_marker_path(
    data_dir: &std::path::Path,
    goal_run_id: &str,
    step_index: usize,
) -> std::path::PathBuf {
    projection::goal_step_completion_marker_path(data_dir, goal_run_id, step_index)
}

pub(crate) fn goal_step_completion_marker_prompt_block_for_data_dir(
    data_dir: &std::path::Path,
    goal_run: &GoalRun,
) -> Option<String> {
    if goal_run.current_step_index >= goal_run.steps.len() {
        return None;
    }
    let human_step_number = goal_run.current_step_index.saturating_add(1);
    let total_steps = goal_run.steps.len();
    let marker_path = projection::goal_step_completion_marker_path(
        data_dir,
        &goal_run.id,
        goal_run.current_step_index,
    );
    Some(format!(
        "## Goal Step Completion Marker\n\
         - Current step: Step {human_step_number} of {total_steps}\n\
         - Required completion marker: {marker_path}\n\
         - This step cannot be marked complete until that file exists.\n\
         - Before finishing, create that file with a short summary of what was completed and any outputs produced for this step.",
        marker_path = marker_path.display(),
    ))
}

#[cfg(test)]
pub(crate) fn set_goal_projection_write_delay_for_tests(
    delay: std::time::Duration,
) -> projection::GoalProjectionWriteDelayGuard {
    projection::set_goal_projection_write_delay_for_tests(delay)
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

    engine.emit_goal_run_update(&updated, None);
}
