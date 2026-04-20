use super::*;
use serde::Serialize;
use std::path::{Path, PathBuf};
use uuid::Uuid;

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

fn goal_projection_dir(data_dir: &Path, goal_run_id: &str) -> PathBuf {
    goal_projection_root_dir(data_dir).join(goal_run_id)
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
    let projection_dir = goal_projection_dir(data_dir, &goal_run.id);
    tokio::fs::create_dir_all(&projection_dir).await?;

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
