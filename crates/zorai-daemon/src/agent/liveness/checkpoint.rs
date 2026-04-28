//! Checkpoint save/load/list/prune — persistent snapshots for goal run recovery.

use anyhow::{bail, Context};
use uuid::Uuid;

use super::state_layers::*;
use crate::agent::types::*;

/// Create a [`CheckpointData`] snapshot from the current goal-run state.
///
/// Copies all four state layers (goal, execution, context, runtime) into a
/// single serialisable structure with a fresh UUID-based id.
pub fn checkpoint_save(
    checkpoint_type: CheckpointType,
    goal_run: &GoalRun,
    tasks: &[AgentTask],
    thread_id: Option<&str>,
    context_summary: Option<String>,
    context_tokens: Option<u32>,
    work_context: Option<&ThreadWorkContext>,
    todos: &[TodoItem],
    now: u64,
) -> CheckpointData {
    let id = format!("cp_{}", Uuid::new_v4());
    let mut cp = CheckpointData::new(
        id,
        goal_run.id.clone(),
        checkpoint_type,
        goal_run.clone(),
        now,
    );

    // Layer 2: Execution State
    cp.tasks_snapshot = tasks.to_vec();

    // Layer 3: Context State
    cp.thread_id = thread_id.map(String::from);
    cp.context_summary = context_summary;
    cp.context_tokens = context_tokens;

    // Layer 4: Runtime State
    cp.work_context = work_context.cloned();
    cp.todos = todos.to_vec();
    cp.memory_updates = goal_run.memory_updates.clone();

    cp
}

/// Serialise a [`CheckpointData`] to JSON and persist it via the history store.
///
/// The checkpoint is written to the `agent_checkpoints` table.  The table is
/// created on first use if it does not already exist.
pub async fn checkpoint_store(
    history: &crate::history::HistoryStore,
    checkpoint: &CheckpointData,
) -> anyhow::Result<()> {
    let state_json =
        serde_json::to_string(checkpoint).context("failed to serialise checkpoint to JSON")?;

    history
        .upsert_checkpoint(
            &checkpoint.id,
            &checkpoint.goal_run_id,
            checkpoint.thread_id.as_deref(),
            // Derive task_id from the goal run's active task, if any.
            checkpoint.goal_run.active_task_id.as_deref(),
            checkpoint.checkpoint_type,
            &state_json,
            checkpoint.context_summary.as_deref(),
            checkpoint.created_at,
        )
        .await
}

/// Deserialise a [`CheckpointData`] from JSON, validating the schema version.
pub fn checkpoint_load(state_json: &str) -> anyhow::Result<CheckpointData> {
    let data: CheckpointData =
        serde_json::from_str(state_json).context("failed to deserialise checkpoint JSON")?;

    if data.version != CHECKPOINT_SCHEMA_VERSION {
        bail!(
            "unsupported checkpoint schema version {} (expected {})",
            data.version,
            CHECKPOINT_SCHEMA_VERSION
        );
    }

    Ok(data)
}

/// Parse multiple checkpoint JSON strings into [`CheckpointSummary`] values,
/// sorted by `created_at` descending (newest first).
///
/// Unparseable entries are silently skipped so that a single corrupt row does
/// not prevent listing the rest.
pub fn checkpoint_list(checkpoints_json: &[String]) -> Vec<CheckpointSummary> {
    let mut summaries: Vec<CheckpointSummary> = checkpoints_json
        .iter()
        .filter_map(|json| {
            serde_json::from_str::<CheckpointData>(json)
                .ok()
                .map(|cp| cp.to_summary())
        })
        .collect();

    summaries.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    summaries
}

/// Keep only the `keep_last_n` most recent checkpoints, removing older ones.
///
/// Checkpoints are sorted by `created_at` ascending (oldest first) before
/// truncation so that the newest `keep_last_n` entries survive.
pub fn checkpoint_prune(checkpoints: &mut Vec<CheckpointData>, keep_last_n: usize) {
    if checkpoints.len() <= keep_last_n {
        return;
    }

    // Sort oldest-first, then keep only the tail.
    checkpoints.sort_by_key(|cp| cp.created_at);
    let start = checkpoints.len() - keep_last_n;
    *checkpoints = checkpoints.split_off(start);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "tests/checkpoint/mod.rs"]
mod tests;
