use super::*;

pub(super) const POLICY_TOOL_OUTCOME_HISTORY_LIMIT: usize = 6;

pub(super) fn summarize_tool_result_for_policy(
    tool_name: &str,
    result: &ToolResult,
) -> super::orchestrator_policy::PolicyToolOutcomeSummary {
    super::orchestrator_policy::PolicyToolOutcomeSummary {
        tool_name: tool_name.to_string(),
        outcome: if result.is_error {
            "failure".to_string()
        } else {
            "success".to_string()
        },
        summary: result.content.chars().take(160).collect(),
    }
}

fn summarize_policy_self_assessment(
    assessment: &super::metacognitive::self_assessment::Assessment,
) -> Option<String> {
    let reasoning = assessment.reasoning.trim();
    if reasoning.is_empty() {
        None
    } else {
        Some(reasoning.to_string())
    }
}

pub(super) fn policy_scope_for_task(
    thread_id: &str,
    task: &AgentTask,
) -> super::orchestrator_policy::PolicyDecisionScope {
    super::orchestrator_policy::PolicyDecisionScope {
        thread_id: thread_id.to_string(),
        goal_run_id: task.goal_run_id.clone(),
    }
}

fn build_runtime_self_assessment(
    goal_run: Option<&GoalRun>,
    task_retry_count: u32,
    short_term_success_rate: f64,
    awareness_stuck: bool,
    repeated_approach: bool,
) -> super::metacognitive::self_assessment::Assessment {
    let (steps_completed, steps_total) = goal_run
        .map(|goal_run| {
            let completed = goal_run
                .steps
                .iter()
                .filter(|step| step.status == GoalRunStepStatus::Completed)
                .count();
            (completed, goal_run.steps.len())
        })
        .unwrap_or((0, 0));
    let goal_distance_pct = if steps_total == 0 {
        0.0
    } else {
        (steps_completed as f64 / steps_total as f64) * 100.0
    };
    let momentum = if repeated_approach || awareness_stuck {
        -0.5
    } else if short_term_success_rate >= 0.5 {
        0.1
    } else {
        -0.1
    };

    super::metacognitive::self_assessment::SelfAssessor::default().assess(
        &super::metacognitive::self_assessment::AssessmentInput {
            progress: super::metacognitive::self_assessment::ProgressMetrics {
                goal_distance_pct,
                steps_completed,
                steps_total,
                estimated_remaining: steps_total.saturating_sub(steps_completed),
                momentum,
            },
            efficiency: super::metacognitive::self_assessment::EfficiencyMetrics {
                token_efficiency: if short_term_success_rate > 0.0 {
                    0.6
                } else {
                    0.0
                },
                tool_success_rate: short_term_success_rate,
                time_efficiency: 0.0,
                tokens_consumed: 0,
                elapsed_secs: task_retry_count as u64,
            },
            quality: super::metacognitive::self_assessment::QualityMetrics {
                error_rate: 1.0 - short_term_success_rate,
                revision_count: task_retry_count,
                user_feedback_score: None,
            },
        },
    )
}

fn has_three_recent_non_success_matches(
    tried_approaches: &[super::episodic::TriedApproach],
    current_approach_hash: &str,
) -> bool {
    let recent_non_success: Vec<_> = tried_approaches
        .iter()
        .rev()
        .filter(|approach| !matches!(approach.outcome, super::episodic::EpisodeOutcome::Success))
        .take(3)
        .collect();
    recent_non_success.len() == 3
        && recent_non_success
            .iter()
            .all(|approach| approach.approach_hash == current_approach_hash)
}

async fn build_policy_context_for_tool_result(
    engine: &AgentEngine,
    thread_id: &str,
    task: &AgentTask,
    current_approach_hash: &str,
    recent_tool_outcomes: &[super::orchestrator_policy::PolicyToolOutcomeSummary],
) -> Option<(
    super::orchestrator_policy::PolicyDecisionScope,
    super::orchestrator_policy::PolicyEvaluationContext,
)> {
    let scope = super::orchestrator_policy::PolicyDecisionScope {
        thread_id: thread_id.to_string(),
        goal_run_id: task.goal_run_id.clone(),
    };

    let repeated_approach = {
        let scope_id = crate::agent::agent_identity::current_agent_scope_id();
        let stores = engine.episodic_store.read().await;
        stores.get(&scope_id).is_some_and(|store| {
            super::episodic::counter_who::detect_repeated_approaches(
                &store.counter_who.tried_approaches,
                3,
            )
            .is_some()
                || has_three_recent_non_success_matches(
                    &store.counter_who.tried_approaches,
                    current_approach_hash,
                )
        })
    };

    let (awareness_stuck, awareness_summary, short_term_success_rate) = {
        let monitor = engine.awareness.read().await;
        let stuck = monitor.check_diminishing_returns(thread_id).is_some();
        let summary = monitor.get_window(thread_id).map(|window| {
            format!(
                "short-term success {:.0}%, same-pattern streak {}, failures {}",
                window.short_term_success_rate * 100.0,
                window.consecutive_same_pattern,
                window.total_failure_count,
            )
        });
        let rate = monitor
            .get_window(thread_id)
            .map(|window| window.short_term_success_rate)
            .unwrap_or(0.8);
        (stuck, summary, rate)
    };

    let goal_run = match task.goal_run_id.as_deref() {
        Some(goal_run_id) => engine.get_goal_run(goal_run_id).await,
        None => None,
    };
    let assessment = build_runtime_self_assessment(
        goal_run.as_ref(),
        task.retry_count,
        short_term_success_rate,
        awareness_stuck,
        repeated_approach,
    );
    let trigger_input = super::orchestrator_policy::PolicyTriggerInput {
        thread_id: thread_id.to_string(),
        goal_run_id: task.goal_run_id.clone(),
        repeated_approach,
        awareness_stuck,
        should_pivot: assessment.should_pivot,
        should_escalate: assessment.should_escalate,
    };
    let trigger = match super::orchestrator_policy::evaluate_triggers(&trigger_input) {
        super::orchestrator_policy::TriggerOutcome::NoIntervention => return None,
        super::orchestrator_policy::TriggerOutcome::EvaluatePolicy(trigger) => trigger,
    };

    let runtime_context_query = select_runtime_context_query(
        task.goal_step_title
            .as_deref()
            .or(Some(task.title.as_str())),
        goal_run.as_ref().map(|goal_run| goal_run.goal.as_str()),
        None,
    );
    let runtime_work_scope = format_runtime_work_scope_label(
        goal_run.as_ref().map(|goal_run| goal_run.title.as_str()),
        task.goal_step_title.as_deref().or(goal_run
            .as_ref()
            .and_then(|goal_run| goal_run.current_step_title.as_deref())),
        Some(task.title.as_str()),
    );
    let runtime_continuity = build_runtime_continuity_context(
        engine,
        runtime_work_scope.as_deref(),
        runtime_context_query.as_deref(),
    )
    .await;

    let counter_who_context = {
        let scope_id = crate::agent::agent_identity::current_agent_scope_id();
        let stores = engine.episodic_store.read().await;
        stores.get(&scope_id).and_then(|store| {
            let formatted =
                super::episodic::counter_who::format_counter_who_context(&store.counter_who);
            (!formatted.trim().is_empty()).then_some(formatted)
        })
    };
    let thread_context = goal_run.as_ref().map(|goal_run| {
        format!(
            "Goal: {}\nCurrent step: {}\nTask: {}",
            goal_run.goal,
            goal_run
                .current_step_title
                .as_deref()
                .unwrap_or("current step"),
            task.title,
        )
    });

    Some((
        scope,
        super::orchestrator_policy::PolicyEvaluationContext {
            trigger,
            current_retry_guard: Some(current_approach_hash.to_string()),
            recent_tool_outcomes: recent_tool_outcomes.to_vec(),
            awareness_summary,
            continuity_summary: runtime_continuity.continuity_summary,
            counter_who_context,
            negative_constraints_context: runtime_continuity.negative_constraints_context,
            self_assessment_summary: summarize_policy_self_assessment(&assessment),
            thread_context,
            recent_decision_summary: None,
        },
    ))
}

pub(super) async fn apply_post_tool_policy_checkpoint(
    engine: &AgentEngine,
    thread_id: &str,
    task_id: &str,
    task_snapshot: &AgentTask,
    current_approach_hash: &str,
    recent_tool_outcomes: &[super::orchestrator_policy::PolicyToolOutcomeSummary],
    now_epoch_secs: u64,
) -> Result<Option<super::orchestrator_policy::PolicyLoopAction>> {
    let Some((scope, policy_context)) = build_policy_context_for_tool_result(
        engine,
        thread_id,
        task_snapshot,
        current_approach_hash,
        recent_tool_outcomes,
    )
    .await
    else {
        return Ok(None);
    };

    let trigger = policy_context.trigger.clone();
    let selection = engine
        .evaluate_orchestrator_policy_turn(&scope, policy_context, now_epoch_secs)
        .await?;
    let action = engine
        .apply_orchestrator_policy_decision(
            thread_id,
            Some(task_id),
            task_snapshot.goal_run_id.as_deref(),
            &trigger,
            &selection.decision,
            now_epoch_secs,
        )
        .await?;

    Ok(Some(action))
}

pub(super) fn unexpected_stream_end_message(accumulated_content: &str) -> String {
    let trimmed = accumulated_content.trim();
    if trimmed.is_empty() {
        "Error: provider stream ended without yielding a response.".to_string()
    } else {
        accumulated_content.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::has_three_recent_non_success_matches;
    use crate::agent::episodic::{EpisodeOutcome, TriedApproach};

    fn tried(hash: &str, outcome: EpisodeOutcome, timestamp: u64) -> TriedApproach {
        TriedApproach {
            approach_hash: hash.to_string(),
            description: format!("approach:{hash}"),
            outcome,
            timestamp,
        }
    }

    #[test]
    fn repeated_approach_requires_three_recent_non_success_entries() {
        assert!(!has_three_recent_non_success_matches(
            &[
                tried("same", EpisodeOutcome::Failure, 1),
                tried("same", EpisodeOutcome::Failure, 2),
            ],
            "same",
        ));

        assert!(has_three_recent_non_success_matches(
            &[
                tried("same", EpisodeOutcome::Failure, 1),
                tried("same", EpisodeOutcome::Failure, 2),
                tried("same", EpisodeOutcome::Failure, 3),
            ],
            "same",
        ));
    }

    #[test]
    fn repeated_approach_ignores_success_entries_when_counting_recent_failures() {
        assert!(has_three_recent_non_success_matches(
            &[
                tried("same", EpisodeOutcome::Failure, 1),
                tried("same", EpisodeOutcome::Success, 2),
                tried("same", EpisodeOutcome::Failure, 3),
                tried("same", EpisodeOutcome::Failure, 4),
            ],
            "same",
        ));

        assert!(!has_three_recent_non_success_matches(
            &[
                tried("same", EpisodeOutcome::Failure, 1),
                tried("other", EpisodeOutcome::Failure, 2),
                tried("same", EpisodeOutcome::Failure, 3),
            ],
            "same",
        ));
    }
}
