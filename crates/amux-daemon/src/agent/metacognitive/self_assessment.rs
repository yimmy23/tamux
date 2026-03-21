//! Self-assessment — metrics collection and autonomous evaluation of agent progress.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Metrics structs
// ---------------------------------------------------------------------------

/// How close the agent is to completing its goal and at what pace.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressMetrics {
    /// 0-100, how close to goal completion.
    pub goal_distance_pct: f64,
    pub steps_completed: usize,
    pub steps_total: usize,
    pub estimated_remaining: usize,
    /// Positive = accelerating, negative = decelerating.
    pub momentum: f64,
}

/// Resource-usage efficiency indicators.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EfficiencyMetrics {
    /// Useful output per token consumed.
    pub token_efficiency: f64,
    /// 0.0-1.0
    pub tool_success_rate: f64,
    /// Steps completed per minute.
    pub time_efficiency: f64,
    pub tokens_consumed: u32,
    pub elapsed_secs: u64,
}

/// Quality indicators for the work produced so far.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityMetrics {
    /// 0.0-1.0
    pub error_rate: f64,
    /// How many times steps were retried.
    pub revision_count: u32,
    /// Derived from approval / rejection patterns.
    pub user_feedback_score: Option<f64>,
}

/// All metrics bundled together as input to the assessor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssessmentInput {
    pub progress: ProgressMetrics,
    pub efficiency: EfficiencyMetrics,
    pub quality: QualityMetrics,
}

// ---------------------------------------------------------------------------
// Assessment output
// ---------------------------------------------------------------------------

/// The result of a self-assessment pass.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Assessment {
    pub making_progress: bool,
    pub approach_optimal: bool,
    pub should_escalate: bool,
    pub should_pivot: bool,
    pub should_terminate: bool,
    /// 0.0-1.0
    pub confidence: f64,
    /// Human-readable explanation of the assessment.
    pub reasoning: String,
    /// Actionable suggestions.
    pub recommendations: Vec<String>,
}

// ---------------------------------------------------------------------------
// SelfAssessor
// ---------------------------------------------------------------------------

/// Configurable assessor that evaluates agent state against thresholds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SelfAssessor {
    /// Minimum expected progress rate (default: 0.1 = 10% per 5 min).
    pub min_progress_rate: f64,
    /// Maximum tolerable error rate (default: 0.5).
    pub max_error_rate: f64,
    /// Minimum acceptable tool success rate (default: 0.3).
    pub min_tool_success_rate: f64,
    /// Escalate when confidence drops below this (default: 0.7).
    pub escalation_threshold: f64,
}

impl Default for SelfAssessor {
    fn default() -> Self {
        Self {
            min_progress_rate: 0.1,
            max_error_rate: 0.5,
            min_tool_success_rate: 0.3,
            escalation_threshold: 0.7,
        }
    }
}

impl SelfAssessor {
    /// Evaluate the current agent state and produce an [`Assessment`].
    pub fn assess(&self, input: &AssessmentInput) -> Assessment {
        let p = &input.progress;
        let e = &input.efficiency;
        let q = &input.quality;

        // --- individual verdicts -------------------------------------------

        let making_progress =
            p.goal_distance_pct > 10.0 && p.momentum >= 0.0 && p.steps_completed > 0;

        let approach_optimal = e.tool_success_rate > self.min_tool_success_rate
            && q.error_rate < self.max_error_rate
            && e.token_efficiency > 0.5;

        let should_escalate = !making_progress && q.error_rate > self.max_error_rate;

        let should_pivot = p.momentum < -0.3 && !making_progress;

        let should_terminate = p.goal_distance_pct >= 95.0
            || (p.steps_completed >= p.steps_total && p.steps_total > 0);

        // --- confidence: weighted average of positive signals ---------------

        // Progress component (weight 0.4)
        let progress_signal = if p.steps_total > 0 {
            p.goal_distance_pct / 100.0
        } else {
            0.0
        };

        // Efficiency component (weight 0.3)
        let efficiency_signal = e.tool_success_rate;

        // Quality component (weight 0.3)
        let quality_signal = 1.0 - q.error_rate;

        let confidence = (0.4 * progress_signal + 0.3 * efficiency_signal + 0.3 * quality_signal)
            .clamp(0.0, 1.0);

        // --- reasoning -----------------------------------------------------

        let mut reasons: Vec<&str> = Vec::new();
        if making_progress {
            reasons.push("Agent is making progress toward the goal");
        } else {
            reasons.push("Agent is NOT making meaningful progress");
        }
        if approach_optimal {
            reasons.push("current approach appears optimal");
        } else {
            reasons.push("current approach may be sub-optimal");
        }
        if should_escalate {
            reasons.push("escalation recommended due to high error rate with no progress");
        }
        if should_pivot {
            reasons.push("pivot recommended due to negative momentum and stalled progress");
        }
        if should_terminate {
            reasons.push("goal is nearly or fully complete — termination appropriate");
        }
        let reasoning = reasons.join("; ");

        // --- recommendations -----------------------------------------------

        let mut recommendations: Vec<String> = Vec::new();

        if !making_progress {
            recommendations
                .push("Re-evaluate the current plan and consider alternative approaches".into());
        }
        if q.error_rate > self.max_error_rate {
            recommendations.push(format!(
                "Error rate ({:.0}%) exceeds threshold ({:.0}%) — investigate recurring failures",
                q.error_rate * 100.0,
                self.max_error_rate * 100.0,
            ));
        }
        if e.tool_success_rate < self.min_tool_success_rate {
            recommendations.push(format!(
                "Tool success rate ({:.0}%) is below minimum ({:.0}%) — review tool usage patterns",
                e.tool_success_rate * 100.0,
                self.min_tool_success_rate * 100.0,
            ));
        }
        if should_pivot {
            recommendations
                .push("Momentum is negative — consider a fundamentally different strategy".into());
        }
        if should_escalate {
            recommendations
                .push("Escalate to the user or a higher-level agent for guidance".into());
        }
        if confidence < self.escalation_threshold && !should_escalate {
            recommendations.push(format!(
                "Confidence ({:.2}) is below escalation threshold ({:.2}) — consider requesting help",
                confidence, self.escalation_threshold,
            ));
        }

        Assessment {
            making_progress,
            approach_optimal,
            should_escalate,
            should_pivot,
            should_terminate,
            confidence,
            reasoning,
            recommendations,
        }
    }
}

// ---------------------------------------------------------------------------
// Helper function
// ---------------------------------------------------------------------------

/// Compute momentum (acceleration) from a series of step-completion timestamps.
///
/// Returns positive if steps are completing faster (accelerating), negative if
/// slower (decelerating), and 0.0 if stable or insufficient data.
pub fn compute_momentum(recent_step_times: &[u64]) -> f64 {
    // Need at least 3 timestamps to compute two intervals and their difference.
    if recent_step_times.len() < 3 {
        return 0.0;
    }

    let intervals: Vec<f64> = recent_step_times
        .windows(2)
        .map(|w| (w[1] as f64) - (w[0] as f64))
        .collect();

    // Average change in interval length.  Negative delta means intervals are
    // *shrinking* (steps completing faster), which is positive momentum.
    let deltas: Vec<f64> = intervals.windows(2).map(|w| w[1] - w[0]).collect();

    if deltas.is_empty() {
        return 0.0;
    }

    let avg_delta: f64 = deltas.iter().sum::<f64>() / deltas.len() as f64;

    // Invert sign: shrinking intervals => positive momentum.
    -avg_delta
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Convenience builder for test inputs.
    fn make_input(
        goal_distance_pct: f64,
        steps_completed: usize,
        steps_total: usize,
        momentum: f64,
        token_efficiency: f64,
        tool_success_rate: f64,
        error_rate: f64,
    ) -> AssessmentInput {
        AssessmentInput {
            progress: ProgressMetrics {
                goal_distance_pct,
                steps_completed,
                steps_total,
                estimated_remaining: steps_total.saturating_sub(steps_completed),
                momentum,
            },
            efficiency: EfficiencyMetrics {
                token_efficiency,
                tool_success_rate,
                time_efficiency: 1.0,
                tokens_consumed: 500,
                elapsed_secs: 60,
            },
            quality: QualityMetrics {
                error_rate,
                revision_count: 0,
                user_feedback_score: None,
            },
        }
    }

    // 1. Default assessor has reasonable thresholds
    #[test]
    fn default_assessor_has_reasonable_thresholds() {
        let a = SelfAssessor::default();
        assert!((a.min_progress_rate - 0.1).abs() < f64::EPSILON);
        assert!((a.max_error_rate - 0.5).abs() < f64::EPSILON);
        assert!((a.min_tool_success_rate - 0.3).abs() < f64::EPSILON);
        assert!((a.escalation_threshold - 0.7).abs() < f64::EPSILON);
    }

    // 2. Making progress when goal_distance > 10% with positive momentum
    #[test]
    fn making_progress_positive_momentum() {
        let assessor = SelfAssessor::default();
        let input = make_input(50.0, 5, 10, 0.5, 0.8, 0.9, 0.1);
        let result = assessor.assess(&input);
        assert!(result.making_progress);
    }

    // 3. Not making progress when goal_distance is 0
    #[test]
    fn not_making_progress_zero_distance() {
        let assessor = SelfAssessor::default();
        let input = make_input(0.0, 0, 10, 0.0, 0.8, 0.9, 0.1);
        let result = assessor.assess(&input);
        assert!(!result.making_progress);
    }

    // 4. Approach optimal with high success rate and low errors
    #[test]
    fn approach_optimal_high_success_low_errors() {
        let assessor = SelfAssessor::default();
        let input = make_input(50.0, 5, 10, 0.5, 0.8, 0.9, 0.1);
        let result = assessor.assess(&input);
        assert!(result.approach_optimal);
    }

    // 5. Should escalate with high error rate and no progress
    #[test]
    fn should_escalate_high_errors_no_progress() {
        let assessor = SelfAssessor::default();
        let input = make_input(5.0, 0, 10, -0.1, 0.2, 0.2, 0.8);
        let result = assessor.assess(&input);
        assert!(result.should_escalate);
    }

    // 6. Should pivot with negative momentum
    #[test]
    fn should_pivot_negative_momentum() {
        let assessor = SelfAssessor::default();
        let input = make_input(5.0, 1, 10, -0.5, 0.3, 0.5, 0.3);
        let result = assessor.assess(&input);
        // goal_distance <= 10 AND momentum < 0 => not making_progress
        // momentum < -0.3 AND not making_progress => should_pivot
        assert!(result.should_pivot);
    }

    // 7. Should terminate when goal_distance >= 95
    #[test]
    fn should_terminate_goal_nearly_complete() {
        let assessor = SelfAssessor::default();
        let input = make_input(95.0, 9, 10, 0.1, 0.8, 0.9, 0.05);
        let result = assessor.assess(&input);
        assert!(result.should_terminate);
    }

    // 8. Should terminate when all steps completed
    #[test]
    fn should_terminate_all_steps_done() {
        let assessor = SelfAssessor::default();
        let input = make_input(80.0, 10, 10, 0.0, 0.8, 0.9, 0.05);
        let result = assessor.assess(&input);
        assert!(result.should_terminate);
    }

    // 9. Confidence reflects quality of metrics
    #[test]
    fn confidence_reflects_metric_quality() {
        let assessor = SelfAssessor::default();

        // Good metrics => high confidence
        let good = make_input(80.0, 8, 10, 0.5, 0.9, 0.95, 0.05);
        let good_result = assessor.assess(&good);

        // Poor metrics => low confidence
        let poor = make_input(10.0, 1, 10, -0.5, 0.1, 0.1, 0.9);
        let poor_result = assessor.assess(&poor);

        assert!(
            good_result.confidence > poor_result.confidence,
            "Good metrics ({:.2}) should yield higher confidence than poor ({:.2})",
            good_result.confidence,
            poor_result.confidence,
        );
    }

    // 10. Recommendations are generated for each concern
    #[test]
    fn recommendations_generated_for_concerns() {
        let assessor = SelfAssessor::default();
        // No progress, high errors, low tool success, negative momentum
        let input = make_input(5.0, 1, 10, -0.5, 0.2, 0.1, 0.8);
        let result = assessor.assess(&input);
        assert!(
            !result.recommendations.is_empty(),
            "Should generate recommendations when problems are detected"
        );
        // Expect at least recommendations for: no progress, high error rate,
        // low tool success rate, pivot, escalate
        assert!(
            result.recommendations.len() >= 4,
            "Expected >= 4 recommendations, got {}",
            result.recommendations.len(),
        );
    }

    // 11. compute_momentum accelerating
    #[test]
    fn compute_momentum_accelerating() {
        // Intervals shrinking: 10, 8, 6 => deltas -2, -2 => avg -2 => momentum +2
        let times = vec![0, 10, 18, 24];
        let m = compute_momentum(&times);
        assert!(
            m > 0.0,
            "Shrinking intervals should yield positive momentum, got {m}"
        );
    }

    // 12. compute_momentum decelerating
    #[test]
    fn compute_momentum_decelerating() {
        // Intervals growing: 5, 10, 15 => deltas +5, +5 => avg +5 => momentum -5
        let times = vec![0, 5, 15, 30];
        let m = compute_momentum(&times);
        assert!(
            m < 0.0,
            "Growing intervals should yield negative momentum, got {m}"
        );
    }

    // 13. compute_momentum stable / empty
    #[test]
    fn compute_momentum_stable_or_empty() {
        // Empty
        assert!((compute_momentum(&[]) - 0.0).abs() < f64::EPSILON);

        // Single timestamp
        assert!((compute_momentum(&[42]) - 0.0).abs() < f64::EPSILON);

        // Two timestamps (not enough for two intervals)
        assert!((compute_momentum(&[0, 10]) - 0.0).abs() < f64::EPSILON);

        // Equal intervals => 0 momentum
        let times = vec![0, 10, 20, 30];
        assert!(
            compute_momentum(&times).abs() < f64::EPSILON,
            "Equal intervals should yield zero momentum"
        );
    }
}
