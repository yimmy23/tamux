use super::*;
use serde::Serialize;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[cfg(test)]
static GOAL_PROJECTION_WRITE_DELAY_MS: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

#[derive(Debug, Clone, Serialize)]
struct GoalProofLedgerProjection {
    goal_run_id: String,
    title: String,
    goal: String,
    status: GoalRunStatus,
    updated_at: u64,
    projection_state: GoalProjectionState,
    projection_error: Option<String>,
    proof_checks: Vec<GoalProofCheck>,
    evidence: Vec<GoalEvidenceRecord>,
    latest_resume_decision: Option<GoalResumeDecision>,
}

fn goal_projection_root_dir(data_dir: &Path) -> PathBuf {
    let parent = data_dir.parent().unwrap_or(data_dir);
    if parent.file_name().and_then(|name| name.to_str()) == Some(".tamux") {
        return parent.join("goals");
    }
    parent.join(".tamux").join("goals")
}

pub(crate) fn goal_projection_relative_dir(goal_run_id: &str) -> PathBuf {
    PathBuf::from(".tamux").join("goals").join(goal_run_id)
}

pub(crate) fn goal_projection_dir(data_dir: &Path, goal_run_id: &str) -> PathBuf {
    goal_projection_root_dir(data_dir).join(goal_run_id)
}

pub(crate) fn goal_inventory_relative_dir(goal_run_id: &str) -> PathBuf {
    goal_projection_relative_dir(goal_run_id).join("inventory")
}

pub(crate) fn goal_inventory_relative_execution_dir(goal_run_id: &str) -> PathBuf {
    goal_inventory_relative_dir(goal_run_id).join("execution")
}

pub(crate) fn goal_step_completion_marker_filename(step_index: usize) -> String {
    format!("step-{}-complete.md", step_index.saturating_add(1))
}

pub(crate) fn goal_step_completion_marker_relative_path(
    goal_run_id: &str,
    step_index: usize,
) -> PathBuf {
    goal_inventory_relative_execution_dir(goal_run_id)
        .join(goal_step_completion_marker_filename(step_index))
}

pub(crate) fn goal_inventory_dir(data_dir: &Path, goal_run_id: &str) -> PathBuf {
    goal_projection_dir(data_dir, goal_run_id).join("inventory")
}

pub(crate) fn goal_inventory_specs_dir(data_dir: &Path, goal_run_id: &str) -> PathBuf {
    goal_inventory_dir(data_dir, goal_run_id).join("specs")
}

pub(crate) fn goal_inventory_plans_dir(data_dir: &Path, goal_run_id: &str) -> PathBuf {
    goal_inventory_dir(data_dir, goal_run_id).join("plans")
}

pub(crate) fn goal_inventory_execution_dir(data_dir: &Path, goal_run_id: &str) -> PathBuf {
    goal_inventory_dir(data_dir, goal_run_id).join("execution")
}

pub(crate) fn goal_step_completion_marker_path(
    data_dir: &Path,
    goal_run_id: &str,
    step_index: usize,
) -> PathBuf {
    goal_inventory_execution_dir(data_dir, goal_run_id)
        .join(goal_step_completion_marker_filename(step_index))
}

async fn write_text_file(path: &Path, contents: &str) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let temp_path = path.with_extension(format!("tmp-{}", Uuid::new_v4()));
    tokio::fs::write(&temp_path, contents).await?;
    tokio::fs::rename(&temp_path, path).await?;
    Ok(())
}

#[cfg(test)]
pub(crate) struct GoalProjectionWriteDelayGuard {
    previous_delay_ms: u64,
}

#[cfg(test)]
impl Drop for GoalProjectionWriteDelayGuard {
    fn drop(&mut self) {
        GOAL_PROJECTION_WRITE_DELAY_MS
            .store(self.previous_delay_ms, std::sync::atomic::Ordering::Relaxed);
    }
}

#[cfg(test)]
pub(crate) fn set_goal_projection_write_delay_for_tests(
    delay: std::time::Duration,
) -> GoalProjectionWriteDelayGuard {
    let previous_delay_ms = GOAL_PROJECTION_WRITE_DELAY_MS.swap(
        delay.as_millis().min(u64::MAX as u128) as u64,
        std::sync::atomic::Ordering::Relaxed,
    );
    GoalProjectionWriteDelayGuard { previous_delay_ms }
}

fn proof_checks_for_goal_run(goal_run: &GoalRun) -> Vec<GoalProofCheck> {
    goal_run
        .dossier
        .as_ref()
        .into_iter()
        .flat_map(|dossier| dossier.units.iter())
        .flat_map(|unit| unit.proof_checks.iter().cloned())
        .collect()
}

fn evidence_for_goal_run(goal_run: &GoalRun) -> Vec<GoalEvidenceRecord> {
    goal_run
        .dossier
        .as_ref()
        .into_iter()
        .flat_map(|dossier| dossier.units.iter())
        .flat_map(|unit| unit.evidence.iter().cloned())
        .collect()
}

fn goal_markdown(goal_run: &GoalRun) -> String {
    let dossier = goal_run.dossier.clone().unwrap_or_default();
    let mut lines = vec![
        format!("# {}", goal_run.title),
        String::new(),
        format!("Goal run: {}", goal_run.id),
        format!("Status: {}", goal_run_status_message(goal_run)),
        format!("Dossier state: {:?}", dossier.projection_state),
        String::new(),
        "## Goal".to_string(),
        goal_run.goal.clone(),
        String::new(),
        "## Summary".to_string(),
        dossier.summary.unwrap_or_else(|| goal_run.goal.clone()),
    ];

    if let Some(error) = dossier.projection_error {
        lines.push(String::new());
        lines.push("## Projection Error".to_string());
        lines.push(error);
    }

    lines.join("\n")
}

pub(crate) async fn write_goal_projection_files(
    data_dir: &Path,
    goal_run: &GoalRun,
) -> anyhow::Result<()> {
    #[cfg(test)]
    {
        let delay_ms = GOAL_PROJECTION_WRITE_DELAY_MS.load(std::sync::atomic::Ordering::Relaxed);
        if delay_ms > 0 {
            tokio::time::sleep(std::time::Duration::from_millis(delay_ms)).await;
        }
    }

    let projection_dir = goal_projection_dir(data_dir, &goal_run.id);
    tokio::fs::create_dir_all(&projection_dir).await?;
    tokio::fs::create_dir_all(goal_inventory_specs_dir(data_dir, &goal_run.id)).await?;
    tokio::fs::create_dir_all(goal_inventory_plans_dir(data_dir, &goal_run.id)).await?;
    tokio::fs::create_dir_all(goal_inventory_execution_dir(data_dir, &goal_run.id)).await?;

    let dossier = goal_run.dossier.clone().unwrap_or_default();
    let dossier_json = serde_json::to_string_pretty(&dossier)?;
    write_text_file(&projection_dir.join("dossier.json"), &dossier_json).await?;

    let proof_ledger = GoalProofLedgerProjection {
        goal_run_id: goal_run.id.clone(),
        title: goal_run.title.clone(),
        goal: goal_run.goal.clone(),
        status: goal_run.status,
        updated_at: goal_run.updated_at,
        projection_state: dossier.projection_state,
        projection_error: dossier.projection_error.clone(),
        proof_checks: proof_checks_for_goal_run(goal_run),
        evidence: evidence_for_goal_run(goal_run),
        latest_resume_decision: dossier.latest_resume_decision.clone(),
    };
    let proof_ledger_json = serde_json::to_string_pretty(&proof_ledger)?;
    write_text_file(
        &projection_dir.join("proof-ledger.json"),
        &proof_ledger_json,
    )
    .await?;

    write_text_file(&projection_dir.join("goal.md"), &goal_markdown(goal_run)).await?;

    Ok(())
}

pub(crate) async fn remove_goal_projection_dir(
    data_dir: &Path,
    goal_run_id: &str,
) -> anyhow::Result<()> {
    let projection_dir = goal_projection_dir(data_dir, goal_run_id);
    match tokio::fs::remove_dir_all(&projection_dir).await {
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}
