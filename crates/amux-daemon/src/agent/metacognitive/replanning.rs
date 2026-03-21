//! Dynamic re-planning — strategy selection and execution for recovering from degraded states.

use serde::{Deserialize, Serialize};

use crate::agent::types::StuckReason;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// High-level strategy the metacognitive loop can select when a step is stuck
/// or performance is degraded.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReplanStrategy {
    /// Compress context and retry the current step.
    CompressRetry,
    /// Spawn a specialized sub-agent for the stuck step.
    SpawnExpert { expertise: String },
    /// Request human input via approval workflow.
    UserGuidance { question: String },
    /// Rotate tool availability to force a different approach.
    AlternativeTools { disable_tools: Vec<String> },
    /// Decompose the stuck step into parallel sub-tasks.
    Parallelize { sub_tasks: Vec<String> },
    /// Trigger a full goal re-plan.
    GoalRevision { reason: String },
}

/// Snapshot of context available to the replanning decision function.
#[derive(Debug, Clone)]
pub struct ReplanContext {
    pub current_step_index: usize,
    pub step_title: String,
    pub stuck_reason: Option<StuckReason>,
    pub attempt_count: u32,
    pub error_rate: f64,
    pub tool_success_rate: f64,
    pub context_utilization_pct: u32,
    pub has_checkpoint: bool,
    pub recent_tool_names: Vec<String>,
}

/// The output of the strategy-selection function: a primary strategy, the
/// reasoning behind it, a confidence score, and an optional fallback.
#[derive(Debug, Clone)]
pub struct ReplanDecision {
    pub strategy: ReplanStrategy,
    pub reasoning: String,
    pub confidence: f64,
    pub fallback: Option<ReplanStrategy>,
}

// ---------------------------------------------------------------------------
// Strategy selection
// ---------------------------------------------------------------------------

/// Analyse the current [`ReplanContext`] and select the most appropriate
/// [`ReplanStrategy`], together with a fallback.
pub fn select_replan_strategy(ctx: &ReplanContext) -> ReplanDecision {
    // 1. High error rate overrides everything except resource exhaustion.
    if ctx.error_rate > 0.7 && ctx.stuck_reason != Some(StuckReason::ResourceExhaustion) {
        return ReplanDecision {
            strategy: ReplanStrategy::GoalRevision {
                reason: format!(
                    "Error rate {:.0}% is critically high — the current plan is unlikely to succeed",
                    ctx.error_rate * 100.0,
                ),
            },
            reasoning: "Error rate exceeds 70%; the goal or approach itself likely needs revision."
                .into(),
            confidence: 0.85,
            fallback: Some(ReplanStrategy::UserGuidance {
                question: format!(
                    "Step '{}' has a {:.0}% error rate. Should we revise the goal?",
                    ctx.step_title,
                    ctx.error_rate * 100.0,
                ),
            }),
        };
    }

    match ctx.stuck_reason {
        // ── No stuck reason (preventive) ─────────────────────────────
        None => ReplanDecision {
            strategy: ReplanStrategy::CompressRetry,
            reasoning: "No stuck signal detected; compressing context as a preventive measure."
                .into(),
            confidence: 0.6,
            fallback: Some(ReplanStrategy::GoalRevision {
                reason: "Preventive compression did not help".into(),
            }),
        },

        // ── NoProgress ───────────────────────────────────────────────
        Some(StuckReason::NoProgress) => match ctx.attempt_count {
            0 => ReplanDecision {
                strategy: ReplanStrategy::CompressRetry,
                reasoning: "First attempt at a stalled step — compressing context may unblock it."
                    .into(),
                confidence: 0.7,
                fallback: Some(ReplanStrategy::SpawnExpert {
                    expertise: infer_expertise(&ctx.step_title),
                }),
            },
            1 => ReplanDecision {
                strategy: ReplanStrategy::SpawnExpert {
                    expertise: infer_expertise(&ctx.step_title),
                },
                reasoning: "Second attempt with no progress — delegating to a specialist sub-agent."
                    .into(),
                confidence: 0.75,
                fallback: Some(ReplanStrategy::UserGuidance {
                    question: format!(
                        "Step '{}' is stuck after two attempts. Can you provide guidance?",
                        ctx.step_title,
                    ),
                }),
            },
            _ => ReplanDecision {
                strategy: ReplanStrategy::UserGuidance {
                    question: format!(
                        "Step '{}' has failed {} attempts with no progress. How should we proceed?",
                        ctx.step_title, ctx.attempt_count,
                    ),
                },
                reasoning: format!(
                    "After {} attempts with no progress, human guidance is needed.",
                    ctx.attempt_count,
                ),
                confidence: 0.9,
                fallback: Some(ReplanStrategy::GoalRevision {
                    reason: format!(
                        "Step '{}' is stuck after {} attempts",
                        ctx.step_title, ctx.attempt_count,
                    ),
                }),
            },
        },

        // ── ErrorLoop ────────────────────────────────────────────────
        Some(StuckReason::ErrorLoop) => {
            let most_used = most_used_tool(&ctx.recent_tool_names);
            ReplanDecision {
                strategy: ReplanStrategy::AlternativeTools {
                    disable_tools: vec![most_used.clone()],
                },
                reasoning: format!(
                    "Repeated errors indicate a tool problem — disabling '{}' to force a different approach.",
                    most_used,
                ),
                confidence: 0.75,
                fallback: Some(ReplanStrategy::SpawnExpert {
                    expertise: infer_expertise(&ctx.step_title),
                }),
            }
        }

        // ── ToolCallLoop ─────────────────────────────────────────────
        Some(StuckReason::ToolCallLoop) => ReplanDecision {
            strategy: ReplanStrategy::GoalRevision {
                reason: format!(
                    "Tool-call loop detected on step '{}' — the plan itself needs restructuring",
                    ctx.step_title,
                ),
            },
            reasoning: "A→B→A→B tool loop suggests the current plan cannot converge.".into(),
            confidence: 0.8,
            fallback: Some(ReplanStrategy::UserGuidance {
                question: format!(
                    "Step '{}' is stuck in a tool-call loop. Should we take a different approach?",
                    ctx.step_title,
                ),
            }),
        },

        // ── ResourceExhaustion ───────────────────────────────────────
        Some(StuckReason::ResourceExhaustion) => ReplanDecision {
            strategy: ReplanStrategy::CompressRetry,
            reasoning: format!(
                "Context utilization at {}% — compression is the direct remedy.",
                ctx.context_utilization_pct,
            ),
            confidence: 0.95,
            fallback: Some(ReplanStrategy::Parallelize {
                sub_tasks: vec![format!("Complete: {}", ctx.step_title)],
            }),
        },

        // ── Timeout ──────────────────────────────────────────────────
        Some(StuckReason::Timeout) => match ctx.attempt_count {
            0 => ReplanDecision {
                strategy: ReplanStrategy::Parallelize {
                    sub_tasks: vec![
                        format!("Part A of: {}", ctx.step_title),
                        format!("Part B of: {}", ctx.step_title),
                    ],
                },
                reasoning: "First timeout — attempting to parallelise the step into sub-tasks."
                    .into(),
                confidence: 0.65,
                fallback: Some(ReplanStrategy::UserGuidance {
                    question: format!(
                        "Step '{}' timed out. Can it be broken down differently?",
                        ctx.step_title,
                    ),
                }),
            },
            _ => ReplanDecision {
                strategy: ReplanStrategy::UserGuidance {
                    question: format!(
                        "Step '{}' has timed out {} time(s). Should we simplify or skip it?",
                        ctx.step_title, ctx.attempt_count,
                    ),
                },
                reasoning: format!(
                    "Repeated timeouts ({} attempts) — asking the user for guidance.",
                    ctx.attempt_count,
                ),
                confidence: 0.85,
                fallback: Some(ReplanStrategy::GoalRevision {
                    reason: format!("Step '{}' keeps timing out", ctx.step_title),
                }),
            },
        },
    }
}

// ---------------------------------------------------------------------------
// Prompt builder
// ---------------------------------------------------------------------------

/// Build a human-readable prompt describing the replan decision, suitable for
/// injection into the agent's context window.
pub fn build_replan_prompt(decision: &ReplanDecision, step_title: &str) -> String {
    let strategy_desc = match &decision.strategy {
        ReplanStrategy::CompressRetry => {
            "Compress the current context and retry this step from the latest checkpoint.".into()
        }
        ReplanStrategy::SpawnExpert { expertise } => {
            format!("Spawn a sub-agent with expertise in '{expertise}' to handle this step.")
        }
        ReplanStrategy::UserGuidance { question } => {
            format!("Request human guidance: \"{question}\"")
        }
        ReplanStrategy::AlternativeTools { disable_tools } => {
            format!(
                "Disable the following tools and retry with alternatives: {}.",
                disable_tools.join(", "),
            )
        }
        ReplanStrategy::Parallelize { sub_tasks } => {
            format!(
                "Decompose this step into {} parallel sub-tasks:\n{}",
                sub_tasks.len(),
                sub_tasks
                    .iter()
                    .enumerate()
                    .map(|(i, t)| format!("  {}. {}", i + 1, t))
                    .collect::<Vec<_>>()
                    .join("\n"),
            )
        }
        ReplanStrategy::GoalRevision { reason } => {
            format!("Trigger a full goal revision. Reason: {reason}")
        }
    };

    let fallback_line = match &decision.fallback {
        Some(fb) => format!("\nFallback strategy: {:?}", fb),
        None => String::new(),
    };

    format!(
        "[REPLAN] Step: '{step_title}'\n\
         Strategy: {strategy_desc}\n\
         Reasoning: {reasoning}\n\
         Confidence: {confidence:.0}%{fallback_line}",
        reasoning = decision.reasoning,
        confidence = decision.confidence * 100.0,
    )
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Simple heuristic to infer a domain of expertise from a step title.
fn infer_expertise(step_title: &str) -> String {
    let lower = step_title.to_lowercase();
    if lower.contains("test") {
        "testing".into()
    } else if lower.contains("deploy") || lower.contains("ci") || lower.contains("cd") {
        "devops".into()
    } else if lower.contains("sql") || lower.contains("database") || lower.contains("db") {
        "databases".into()
    } else if lower.contains("api") || lower.contains("http") || lower.contains("rest") {
        "api-design".into()
    } else if lower.contains("ui") || lower.contains("frontend") || lower.contains("css") {
        "frontend".into()
    } else {
        "general-engineering".into()
    }
}

/// Return the most frequently occurring tool name, or `"unknown"` if empty.
fn most_used_tool(recent: &[String]) -> String {
    if recent.is_empty() {
        return "unknown".into();
    }
    let mut counts = std::collections::HashMap::<&str, usize>::new();
    for name in recent {
        *counts.entry(name.as_str()).or_default() += 1;
    }
    counts
        .into_iter()
        .max_by_key(|&(_, c)| c)
        .map(|(name, _)| name.to_string())
        .unwrap_or_else(|| "unknown".into())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to build a default context that can be tweaked per test.
    fn base_ctx() -> ReplanContext {
        ReplanContext {
            current_step_index: 0,
            step_title: "Implement feature X".into(),
            stuck_reason: None,
            attempt_count: 0,
            error_rate: 0.1,
            tool_success_rate: 0.9,
            context_utilization_pct: 40,
            has_checkpoint: true,
            recent_tool_names: vec!["code_edit".into(), "code_edit".into(), "search".into()],
        }
    }

    // ── 1. CompressRetry for NoProgress attempt 0 ────────────────────

    #[test]
    fn compress_retry_for_no_progress_attempt_0() {
        let mut ctx = base_ctx();
        ctx.stuck_reason = Some(StuckReason::NoProgress);
        ctx.attempt_count = 0;

        let decision = select_replan_strategy(&ctx);
        assert!(
            matches!(decision.strategy, ReplanStrategy::CompressRetry),
            "Expected CompressRetry, got {:?}",
            decision.strategy,
        );
    }

    // ── 2. SpawnExpert for NoProgress attempt 1 ──────────────────────

    #[test]
    fn spawn_expert_for_no_progress_attempt_1() {
        let mut ctx = base_ctx();
        ctx.stuck_reason = Some(StuckReason::NoProgress);
        ctx.attempt_count = 1;

        let decision = select_replan_strategy(&ctx);
        assert!(
            matches!(decision.strategy, ReplanStrategy::SpawnExpert { .. }),
            "Expected SpawnExpert, got {:?}",
            decision.strategy,
        );
    }

    // ── 3. UserGuidance for NoProgress attempt 2+ ────────────────────

    #[test]
    fn user_guidance_for_no_progress_attempt_2_plus() {
        let mut ctx = base_ctx();
        ctx.stuck_reason = Some(StuckReason::NoProgress);
        ctx.attempt_count = 3;

        let decision = select_replan_strategy(&ctx);
        assert!(
            matches!(decision.strategy, ReplanStrategy::UserGuidance { .. }),
            "Expected UserGuidance, got {:?}",
            decision.strategy,
        );
    }

    // ── 4. AlternativeTools for ErrorLoop ────────────────────────────

    #[test]
    fn alternative_tools_for_error_loop() {
        let mut ctx = base_ctx();
        ctx.stuck_reason = Some(StuckReason::ErrorLoop);
        ctx.error_rate = 0.5; // below the 0.7 override threshold

        let decision = select_replan_strategy(&ctx);
        assert!(
            matches!(decision.strategy, ReplanStrategy::AlternativeTools { .. }),
            "Expected AlternativeTools, got {:?}",
            decision.strategy,
        );
    }

    // ── 5. GoalRevision for ToolCallLoop ─────────────────────────────

    #[test]
    fn goal_revision_for_tool_call_loop() {
        let mut ctx = base_ctx();
        ctx.stuck_reason = Some(StuckReason::ToolCallLoop);

        let decision = select_replan_strategy(&ctx);
        assert!(
            matches!(decision.strategy, ReplanStrategy::GoalRevision { .. }),
            "Expected GoalRevision, got {:?}",
            decision.strategy,
        );
    }

    // ── 6. CompressRetry for ResourceExhaustion ──────────────────────

    #[test]
    fn compress_retry_for_resource_exhaustion() {
        let mut ctx = base_ctx();
        ctx.stuck_reason = Some(StuckReason::ResourceExhaustion);
        ctx.context_utilization_pct = 95;

        let decision = select_replan_strategy(&ctx);
        assert!(
            matches!(decision.strategy, ReplanStrategy::CompressRetry),
            "Expected CompressRetry, got {:?}",
            decision.strategy,
        );
    }

    // ── 7. Parallelize for Timeout attempt 0 ─────────────────────────

    #[test]
    fn parallelize_for_timeout_attempt_0() {
        let mut ctx = base_ctx();
        ctx.stuck_reason = Some(StuckReason::Timeout);
        ctx.attempt_count = 0;

        let decision = select_replan_strategy(&ctx);
        assert!(
            matches!(decision.strategy, ReplanStrategy::Parallelize { .. }),
            "Expected Parallelize, got {:?}",
            decision.strategy,
        );
    }

    // ── 8. UserGuidance for Timeout attempt 1+ ──────────────────────

    #[test]
    fn user_guidance_for_timeout_attempt_1_plus() {
        let mut ctx = base_ctx();
        ctx.stuck_reason = Some(StuckReason::Timeout);
        ctx.attempt_count = 2;

        let decision = select_replan_strategy(&ctx);
        assert!(
            matches!(decision.strategy, ReplanStrategy::UserGuidance { .. }),
            "Expected UserGuidance, got {:?}",
            decision.strategy,
        );
    }

    // ── 9. GoalRevision for high error rate ──────────────────────────

    #[test]
    fn goal_revision_for_high_error_rate() {
        let mut ctx = base_ctx();
        ctx.stuck_reason = Some(StuckReason::NoProgress);
        ctx.error_rate = 0.85;

        let decision = select_replan_strategy(&ctx);
        assert!(
            matches!(decision.strategy, ReplanStrategy::GoalRevision { .. }),
            "Expected GoalRevision, got {:?}",
            decision.strategy,
        );
    }

    // ── 10. CompressRetry for preventive (no stuck reason) ───────────

    #[test]
    fn compress_retry_for_preventive_no_stuck_reason() {
        let ctx = base_ctx(); // stuck_reason = None

        let decision = select_replan_strategy(&ctx);
        assert!(
            matches!(decision.strategy, ReplanStrategy::CompressRetry),
            "Expected CompressRetry, got {:?}",
            decision.strategy,
        );
    }

    // ── 11. Fallback strategy differs from primary ───────────────────

    #[test]
    fn fallback_differs_from_primary() {
        let mut ctx = base_ctx();
        ctx.stuck_reason = Some(StuckReason::NoProgress);
        ctx.attempt_count = 0;

        let decision = select_replan_strategy(&ctx);
        let fallback = decision.fallback.expect("Expected a fallback strategy");

        // Primary is CompressRetry; fallback must not be.
        assert!(
            !matches!(fallback, ReplanStrategy::CompressRetry),
            "Fallback should differ from primary CompressRetry, got {:?}",
            fallback,
        );
    }

    // ── 12. build_replan_prompt includes step title ──────────────────

    #[test]
    fn build_replan_prompt_includes_step_title() {
        let decision = ReplanDecision {
            strategy: ReplanStrategy::CompressRetry,
            reasoning: "test reasoning".into(),
            confidence: 0.75,
            fallback: None,
        };

        let prompt = build_replan_prompt(&decision, "Write unit tests");
        assert!(
            prompt.contains("Write unit tests"),
            "Prompt should contain the step title. Got:\n{prompt}",
        );
    }

    // ── 13. ResourceExhaustion ignores high error rate ───────────────

    #[test]
    fn resource_exhaustion_ignores_high_error_rate() {
        let mut ctx = base_ctx();
        ctx.stuck_reason = Some(StuckReason::ResourceExhaustion);
        ctx.error_rate = 0.9;
        ctx.context_utilization_pct = 98;

        // ResourceExhaustion should still yield CompressRetry even with
        // a high error rate, because the root cause is context exhaustion.
        let decision = select_replan_strategy(&ctx);
        assert!(
            matches!(decision.strategy, ReplanStrategy::CompressRetry),
            "Expected CompressRetry even with high error rate, got {:?}",
            decision.strategy,
        );
    }

    // ── 14. AlternativeTools disables the most-used tool ─────────────

    #[test]
    fn alternative_tools_disables_most_used() {
        let mut ctx = base_ctx();
        ctx.stuck_reason = Some(StuckReason::ErrorLoop);
        ctx.recent_tool_names = vec![
            "bash".into(),
            "code_edit".into(),
            "bash".into(),
            "bash".into(),
        ];

        let decision = select_replan_strategy(&ctx);
        if let ReplanStrategy::AlternativeTools { disable_tools } = &decision.strategy {
            assert_eq!(disable_tools, &["bash"]);
        } else {
            panic!("Expected AlternativeTools, got {:?}", decision.strategy);
        }
    }

    // ── 15. SpawnExpert infers domain-specific expertise ─────────────

    #[test]
    fn spawn_expert_infers_expertise() {
        let mut ctx = base_ctx();
        ctx.step_title = "Fix SQL migration for users table".into();
        ctx.stuck_reason = Some(StuckReason::NoProgress);
        ctx.attempt_count = 1;

        let decision = select_replan_strategy(&ctx);
        if let ReplanStrategy::SpawnExpert { expertise } = &decision.strategy {
            assert_eq!(expertise, "databases");
        } else {
            panic!("Expected SpawnExpert, got {:?}", decision.strategy);
        }
    }

    // ── 16. build_replan_prompt includes fallback when present ───────

    #[test]
    fn build_replan_prompt_includes_fallback() {
        let decision = ReplanDecision {
            strategy: ReplanStrategy::CompressRetry,
            reasoning: "context is large".into(),
            confidence: 0.8,
            fallback: Some(ReplanStrategy::UserGuidance {
                question: "Help?".into(),
            }),
        };

        let prompt = build_replan_prompt(&decision, "Deploy service");
        assert!(
            prompt.contains("Fallback strategy"),
            "Prompt should mention the fallback. Got:\n{prompt}",
        );
    }
}
