//! Counter-who self-model: tracks what the agent is currently doing,
//! what approaches have been tried, detects repeated failing patterns,
//! and records operator corrections.

use super::{CorrectionPattern, CounterWhoState, EpisodeOutcome, TriedApproach};
use crate::agent::engine::AgentEngine;
use crate::agent::types::AgentEvent;

use anyhow::Result;
use rusqlite::OptionalExtension;
use sha2::{Digest, Sha256};

// ---------------------------------------------------------------------------
// Pure functions (no AgentEngine dependency, easy to test)
// ---------------------------------------------------------------------------

/// Compute a stable hash for a tool+args combination.
/// Returns the first 16 hex characters of SHA-256("{tool_name}:{args_summary}").
pub fn compute_approach_hash(tool_name: &str, args_summary: &str) -> String {
    let input = format!("{tool_name}:{args_summary}");
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let digest = format!("{:x}", hasher.finalize());
    digest[..16].to_string()
}

/// Detect repeated failing approaches. Groups tried_approaches by hash,
/// counts failures per group, and returns a suggestion if any group
/// meets the threshold.
pub fn detect_repeated_approaches(tried: &[TriedApproach], threshold: usize) -> Option<String> {
    use std::collections::HashMap;

    let mut failure_counts: HashMap<&str, (u32, &str)> = HashMap::new();
    for approach in tried {
        if approach.outcome == EpisodeOutcome::Failure {
            let entry = failure_counts
                .entry(&approach.approach_hash)
                .or_insert((0, &approach.description));
            entry.0 += 1;
        }
    }

    let mut worst: Option<(u32, &str)> = None;
    for (_hash, (count, description)) in &failure_counts {
        if (*count as usize) >= threshold {
            match worst {
                None => worst = Some((*count, description)),
                Some((prev_count, _)) if *count > prev_count => {
                    worst = Some((*count, description));
                }
                _ => {}
            }
        }
    }

    worst.map(|(count, desc)| {
        format!(
            "Repeated failure detected: {desc} has failed {count} times. Consider a different approach."
        )
    })
}

/// Record an operator correction in the counter-who state.
/// Increments count for existing pattern, creates new entry otherwise.
pub fn record_correction(state: &mut CounterWhoState, pattern: &str, now_ms: u64) {
    if let Some(existing) = state
        .correction_patterns
        .iter_mut()
        .find(|cp| cp.pattern == pattern)
    {
        existing.correction_count += 1;
        existing.last_correction_at = now_ms;
    } else {
        state.correction_patterns.push(CorrectionPattern {
            pattern: pattern.to_string(),
            correction_count: 1,
            last_correction_at: now_ms,
        });
    }
}

/// Prune old approaches: remove entries older than max_age_days, cap at max_entries.
pub fn prune_old_approaches(
    state: &mut CounterWhoState,
    now_ms: u64,
    max_age_days: u64,
    max_entries: usize,
) {
    let max_age_ms = max_age_days * 86400 * 1000;
    state
        .tried_approaches
        .retain(|a| now_ms.saturating_sub(a.timestamp) <= max_age_ms);
    if state.tried_approaches.len() > max_entries {
        // Sort by timestamp descending, keep most recent
        state
            .tried_approaches
            .sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        state.tried_approaches.truncate(max_entries);
    }
    // Cap recent_changes at 50 entries
    if state.recent_changes.len() > 50 {
        let drain_count = state.recent_changes.len() - 50;
        state.recent_changes.drain(..drain_count);
    }
}

/// Format the counter-who state as text for system prompt injection.
/// Returns empty string if state has no meaningful content.
pub fn format_counter_who_context(state: &CounterWhoState) -> String {
    let has_focus = state.current_focus.is_some();
    let has_approaches = !state.tried_approaches.is_empty();
    let has_corrections = !state.correction_patterns.is_empty();

    if !has_focus && !has_approaches && !has_corrections {
        return String::new();
    }

    let mut out = String::new();
    out.push_str("## Self-Awareness (Counter-Who)\n");

    let focus = state.current_focus.as_deref().unwrap_or("none");
    out.push_str(&format!("Current focus: {focus}\n"));

    if has_approaches {
        out.push_str(&format!(
            "\nApproaches tried ({}):\n",
            state.tried_approaches.len()
        ));
        for approach in state.tried_approaches.iter().rev().take(10) {
            let outcome_str = match approach.outcome {
                EpisodeOutcome::Success => "success",
                EpisodeOutcome::Failure => "failure",
                EpisodeOutcome::Partial => "partial",
                EpisodeOutcome::Abandoned => "abandoned",
            };
            out.push_str(&format!("- {} -> {}\n", approach.description, outcome_str));
        }
    }

    if has_corrections {
        out.push_str(&format!(
            "\nOperator corrections ({}):\n",
            state.correction_patterns.len()
        ));
        for correction in &state.correction_patterns {
            out.push_str(&format!(
                "- {} (corrected {} times)\n",
                correction.pattern, correction.correction_count
            ));
        }
    }

    // Cap total output at 2000 chars
    if out.len() > 2000 {
        out.truncate(2000);
        out.push_str("\n...(truncated)\n");
    }

    out
}

// ---------------------------------------------------------------------------
// AgentEngine integration methods
// ---------------------------------------------------------------------------

impl AgentEngine {
    /// Update counter-who state after a tool call completes (CWHO-01).
    /// Tracks the tool result, detects repeated failures (CWHO-02),
    /// and emits a CounterWhoAlert if threshold is met.
    pub(crate) async fn update_counter_who_on_tool_result(
        &self,
        thread_id: &str,
        tool_name: &str,
        args_summary: &str,
        success: bool,
    ) {
        let now_ms = super::super::now_millis();
        let hash = compute_approach_hash(tool_name, args_summary);
        let description = format!(
            "{tool_name}({})",
            args_summary.chars().take(100).collect::<String>()
        );
        let outcome = if success {
            EpisodeOutcome::Success
        } else {
            EpisodeOutcome::Failure
        };

        let approach = TriedApproach {
            approach_hash: hash,
            description,
            outcome,
            timestamp: now_ms,
        };

        let scope_id = crate::agent::agent_identity::current_agent_scope_id();
        let mut stores = self.episodic_store.write().await;
        let store = stores.entry(scope_id).or_default();
        store.counter_who.tried_approaches.push(approach);
        store.counter_who.thread_id = Some(thread_id.to_string());
        store.counter_who.current_focus = Some(format!("Tool: {tool_name}"));
        store.counter_who.recent_changes.push(format!(
            "{tool_name} -> {}",
            if success { "success" } else { "failure" }
        ));
        store.counter_who.updated_at = now_ms;

        prune_old_approaches(&mut store.counter_who, now_ms, 7, 20);

        // Check for repeated approaches (CWHO-02)
        let repeated_alert = detect_repeated_approaches(&store.counter_who.tried_approaches, 3)
            .map(|suggestion| {
                let attempt_count = store
                    .counter_who
                    .tried_approaches
                    .iter()
                    .filter(|a| a.outcome == EpisodeOutcome::Failure)
                    .count() as u32;
                (suggestion.clone(), attempt_count, suggestion)
            });
        drop(stores);

        if let Err(error) = self.persist_counter_who().await {
            tracing::warn!(thread_id = %thread_id, error = %error, "failed to persist counter-who state after tool result");
        }

        if let Some((pattern, attempt_count, suggestion)) = repeated_alert {
            let _ = self.event_tx.send(AgentEvent::CounterWhoAlert {
                thread_id: thread_id.to_string(),
                pattern,
                attempt_count,
                suggestion,
            });
        }
    }

    /// Track an operator correction in counter-who state (CWHO-03).
    /// Emits CounterWhoAlert if the same correction has occurred 2+ times.
    pub(crate) async fn update_counter_who_on_correction(
        &self,
        thread_id: &str,
        correction_pattern: &str,
    ) {
        let now_ms = super::super::now_millis();
        let scope_id = crate::agent::agent_identity::current_agent_scope_id();
        let mut stores = self.episodic_store.write().await;
        let store = stores.entry(scope_id).or_default();
        store.counter_who.thread_id = Some(thread_id.to_string());
        record_correction(&mut store.counter_who, correction_pattern, now_ms);
        store.counter_who.updated_at = now_ms;

        // Check if this pattern is persistent
        let correction_count = store
            .counter_who
            .correction_patterns
            .iter()
            .find(|cp| cp.pattern == correction_pattern)
            .map(|cp| cp.correction_count)
            .unwrap_or(0);

        let persistent_alert = (correction_count >= 2).then(|| {
            let pattern = format!(
                "Persistent correction: {correction_pattern} (corrected {correction_count} times)"
            );
            let suggestion = pattern.clone();
            (pattern, suggestion)
        });
        drop(stores);

        if let Err(error) = self.persist_counter_who().await {
            tracing::warn!(thread_id = %thread_id, error = %error, "failed to persist counter-who state after operator correction");
        }

        if let Some((pattern, suggestion)) = persistent_alert {
            let _ = self.event_tx.send(AgentEvent::CounterWhoAlert {
                thread_id: thread_id.to_string(),
                pattern,
                attempt_count: correction_count,
                suggestion,
            });
        }
    }

    /// Persist counter-who state to SQLite (CWHO-04).
    pub(crate) async fn persist_counter_who(&self) -> Result<()> {
        let scope_id = crate::agent::agent_identity::current_agent_scope_id();
        let stores = self.episodic_store.read().await;
        let state = stores
            .get(&scope_id)
            .map(|store| store.counter_who.clone())
            .unwrap_or_default();
        drop(stores);

        let id = state.goal_run_id.as_deref().unwrap_or("global").to_string();
        let agent_id = scope_id;
        let goal_run_id = state.goal_run_id.clone();
        let thread_id = state.thread_id.clone();
        let state_json =
            serde_json::to_string(&state).map_err(|e| anyhow::anyhow!("serialize: {e}"))?;
        let updated_at = state.updated_at as i64;

        self.history
            .conn
            .call(move |conn| {
                conn.execute(
                    "INSERT OR REPLACE INTO counter_who_state (id, agent_id, goal_run_id, thread_id, state_json, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    rusqlite::params![id, agent_id, goal_run_id, thread_id, state_json, updated_at],
                )?;
                Ok(())
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    /// Restore counter-who state from SQLite (CWHO-04).
    pub(crate) async fn restore_counter_who(&self, goal_run_id: Option<&str>) -> Result<()> {
        let gid = goal_run_id.unwrap_or("global").to_string();
        let agent_id = crate::agent::agent_identity::current_agent_scope_id();
        let include_legacy = crate::agent::is_main_agent_scope(&agent_id) as i64;

        let state_json: Option<String> = self
            .history
            .conn
            .call(move |conn| {
                let result = conn
                    .query_row(
                        "SELECT state_json FROM counter_who_state WHERE id = ?1 AND (agent_id = ?2 OR (?3 = 1 AND agent_id IS NULL))",
                        rusqlite::params![gid, agent_id, include_legacy],
                        |row| row.get(0),
                    )
                    .optional()?;
                Ok(result)
            })
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;

        if let Some(json) = state_json {
            let state: CounterWhoState =
                serde_json::from_str(&json).map_err(|e| anyhow::anyhow!("deserialize: {e}"))?;
            let scope_id = crate::agent::agent_identity::current_agent_scope_id();
            let mut stores = self.episodic_store.write().await;
            let store = stores.entry(scope_id).or_default();
            store.counter_who = state;
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "counter_who/tests.rs"]
mod tests;
