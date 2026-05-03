//! Embodied metadata module: scalar dimensions that give the agent a "felt sense"
//! of each action's texture.
//!
//! Five dimensions are computed from structural signals only (no LLM input):
//! - **Difficulty** (0.0-1.0): how hard this action is (error rate, retries)
//! - **Familiarity** (0.0-1.0): how often similar work has been seen (episodic FTS5 hits)
//! - **Trajectory** (-1.0 to 1.0): converging toward or diverging from goal
//! - **Temperature** (0.0-1.0): operator urgency from message frequency/pacing
//! - **Weight** (0.0-1.0): conceptual mass / blast radius of the action
//!
//! These feed into uncertainty scoring (Plan 03) where unfamiliar + difficult = lower confidence.

pub mod dimensions;

use serde::{Deserialize, Serialize};

/// Aggregate embodied metadata for a single action or plan step (EMBD-04).
///
/// All dimensions are structural signals -- no LLM input.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EmbodiedMetadata {
    /// How hard this action is (0.0 = trivial, 1.0 = maximum difficulty).
    pub difficulty: f64,
    /// How familiar this work is from episodic memory (0.0 = novel, 1.0 = very familiar).
    pub familiarity: f64,
    /// Direction of progress (-1.0 = diverging, 0.0 = stalled, 1.0 = converging).
    pub trajectory: f64,
    /// Operator urgency level (0.0 = calm, 1.0 = urgent/rapid-fire messages).
    pub temperature: f64,
    /// Conceptual mass / blast radius (0.0 = read-only, 1.0 = destructive/heavy).
    pub weight: f64,
}

/// Input signals for computing embodied metadata.
///
/// Collected from various daemon subsystems (awareness monitor, episodic store,
/// operator message history) and passed to `compute_embodied_metadata`.
#[derive(Debug, Clone, Default)]
pub struct EmbodiedSignals {
    /// Fraction of recent tool calls that errored (0.0..=1.0).
    pub error_rate: f64,
    /// Number of retries attempted for the current action.
    pub retry_count: u32,
    /// Number of FTS5 hits from episodic memory for similar work.
    pub episodic_hit_count: usize,
    /// Total progress events from awareness window.
    pub progress_count: u32,
    /// Total failure events from awareness window.
    pub failure_count: u32,
    /// Operator messages in the last 5 minutes.
    pub recent_operator_messages: u32,
    /// Average seconds between operator messages (0 if only 1 message).
    pub avg_message_gap_secs: u64,
    /// Name of the tool being invoked.
    pub tool_name: String,
}

/// Compute all 5 embodied dimensions from structural signals.
///
/// Pure function: no I/O, no async, no side effects.
pub fn compute_embodied_metadata(signals: &EmbodiedSignals) -> EmbodiedMetadata {
    EmbodiedMetadata {
        difficulty: dimensions::compute_difficulty(signals.error_rate, signals.retry_count),
        familiarity: dimensions::compute_familiarity(signals.episodic_hit_count),
        trajectory: dimensions::compute_trajectory_score(
            signals.progress_count,
            signals.failure_count,
        ),
        temperature: dimensions::compute_temperature(
            signals.recent_operator_messages,
            signals.avg_message_gap_secs,
        ),
        weight: dimensions::compute_weight(&signals.tool_name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embodied_metadata_default_is_all_zeros() {
        let meta = EmbodiedMetadata::default();
        assert_eq!(meta.difficulty, 0.0);
        assert_eq!(meta.familiarity, 0.0);
        assert_eq!(meta.trajectory, 0.0);
        assert_eq!(meta.temperature, 0.0);
        assert_eq!(meta.weight, 0.0);
    }

    #[test]
    fn compute_embodied_metadata_with_known_inputs() {
        let signals = EmbodiedSignals {
            error_rate: 0.5,
            retry_count: 2,
            episodic_hit_count: 3,
            progress_count: 5,
            failure_count: 0,
            recent_operator_messages: 0,
            avg_message_gap_secs: 0,
            tool_name: zorai_protocol::tool_names::READ_FILE.to_string(),
        };

        let meta = compute_embodied_metadata(&signals);

        // difficulty: 0.6 * 0.5 + 0.4 * (2/5) = 0.3 + 0.16 = 0.46
        assert!(
            (meta.difficulty - 0.46).abs() < 0.001,
            "difficulty: expected ~0.46, got {}",
            meta.difficulty
        );

        // familiarity: 3/5 = 0.6
        assert_eq!(meta.familiarity, 0.6);

        // trajectory: all progress, no failure -> 1.0
        assert_eq!(meta.trajectory, 1.0);

        // temperature: 0 messages -> 0.0
        assert_eq!(meta.temperature, 0.0);

        // weight: read_file -> 0.2
        assert_eq!(meta.weight, 0.2);
    }
}
