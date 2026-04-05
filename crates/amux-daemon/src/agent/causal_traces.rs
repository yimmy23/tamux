//! Runtime causal-trace helpers for live decision capture.

use super::*;
use crate::history::AuditEntryRow;

#[path = "causal_traces/helpers.rs"]
mod helpers;
#[path = "causal_traces/persistence.rs"]
mod persistence;
#[path = "causal_traces/reporting.rs"]
mod reporting;
use helpers::{
    FamilyOutcomeSummary, OutcomeSummary, command_family, estimate_plan_success,
    estimated_success_probability, summarize_outcome,
};

impl AgentEngine {
    pub(super) async fn persist_skill_selection_causal_trace(
        &self,
        thread_id: &str,
        goal_run_id: Option<&str>,
        task_id: Option<&str>,
        selected_variant: &crate::history::SkillVariantRecord,
        candidate_variants: &[crate::history::SkillVariantRecord],
        context_tags: &[String],
    ) {
        let matched_tags = selected_variant
            .context_tags
            .iter()
            .filter(|tag| {
                context_tags
                    .iter()
                    .any(|active| active.eq_ignore_ascii_case(tag))
            })
            .cloned()
            .collect::<Vec<_>>();
        let mut factors = Vec::new();
        if !matched_tags.is_empty() {
            factors.push(crate::agent::learning::traces::CausalFactor {
                factor_type: crate::agent::learning::traces::FactorType::PatternMatch,
                description: format!("matched skill context tags: {}", matched_tags.join(", ")),
                weight: 0.8,
            });
        }
        if selected_variant.success_count > 0 {
            factors.push(crate::agent::learning::traces::CausalFactor {
                factor_type: crate::agent::learning::traces::FactorType::PastSuccess,
                description: format!(
                    "variant has {} prior success(es) across {} use(s)",
                    selected_variant.success_count, selected_variant.use_count
                ),
                weight: 0.6,
            });
        }
        if selected_variant.failure_count > 0 {
            factors.push(crate::agent::learning::traces::CausalFactor {
                factor_type: crate::agent::learning::traces::FactorType::PastFailure,
                description: format!(
                    "variant has {} prior failure(s) across {} use(s)",
                    selected_variant.failure_count, selected_variant.use_count
                ),
                weight: 0.4,
            });
        }
        if selected_variant.status != "active" {
            factors.push(crate::agent::learning::traces::CausalFactor {
                factor_type: crate::agent::learning::traces::FactorType::OperatorPreference,
                description: format!("selected variant status is {}", selected_variant.status),
                weight: 0.2,
            });
        }

        let selected = crate::agent::learning::traces::DecisionOption {
            option_type: selected_variant.skill_name.clone(),
            reasoning: format!(
                "selected skill variant `{}` with status `{}` and success rate {:.0}% for context [{}]",
                selected_variant.variant_name,
                selected_variant.status,
                selected_variant.success_rate() * 100.0,
                if context_tags.is_empty() {
                    "none".to_string()
                } else {
                    context_tags.join(", ")
                }
            ),
            rejection_reason: None,
            estimated_success_prob: Some(selected_variant.success_rate()),
            arguments_hash: Some(crate::agent::learning::traces::hash_context_blob(
                &selected_variant.relative_path,
            )),
        };
        let rejected_options = candidate_variants
            .iter()
            .filter(|variant| variant.variant_id != selected_variant.variant_id)
            .take(3)
            .map(|variant| crate::agent::learning::traces::DecisionOption {
                option_type: variant.skill_name.clone(),
                reasoning: format!(
                    "variant `{}` status `{}` success {:.0}%",
                    variant.variant_name,
                    variant.status,
                    variant.success_rate() * 100.0
                ),
                rejection_reason: Some(if variant.status == "archived" {
                    "archived due to low-value or stale usage".to_string()
                } else if variant.success_rate() < selected_variant.success_rate() {
                    "lower historical success rate".to_string()
                } else {
                    "weaker context match".to_string()
                }),
                estimated_success_prob: Some(variant.success_rate()),
                arguments_hash: Some(crate::agent::learning::traces::hash_context_blob(
                    &variant.relative_path,
                )),
            })
            .collect::<Vec<_>>();

        let trace = crate::agent::learning::traces::CausalTrace {
            trace_id: format!("causal_{}", uuid::Uuid::new_v4()),
            thread_id: Some(thread_id.to_string()),
            goal_run_id: goal_run_id.map(str::to_string),
            task_id: task_id.map(str::to_string),
            decision_type: crate::agent::learning::traces::DecisionType::SkillSelection,
            selected,
            rejected_options,
            context_hash: crate::agent::learning::traces::hash_context_blob(&format!(
                "{}|{}|{}",
                selected_variant.skill_name,
                selected_variant.variant_name,
                context_tags.join(",")
            )),
            causal_factors: factors,
            outcome: crate::agent::learning::traces::CausalTraceOutcome::Unresolved,
            model_used: Some(self.config.read().await.model.clone()),
            created_at: now_millis(),
        };
        let selected_json = serde_json::to_string(&trace.selected).unwrap_or_default();
        let rejected_json = serde_json::to_string(&trace.rejected_options).unwrap_or_default();
        let factors_json = serde_json::to_string(&trace.causal_factors).unwrap_or_default();
        let outcome_json = serde_json::to_string(&trace.outcome).unwrap_or_default();
        if let Err(error) = self
            .history
            .insert_causal_trace(
                &trace.trace_id,
                trace.thread_id.as_deref(),
                trace.goal_run_id.as_deref(),
                trace.task_id.as_deref(),
                "skill_selection",
                &selected_json,
                &rejected_json,
                &trace.context_hash,
                &factors_json,
                &outcome_json,
                trace.model_used.as_deref(),
                trace.created_at,
            )
            .await
        {
            tracing::warn!(thread_id = %thread_id, skill = %selected_variant.skill_name, variant = %selected_variant.variant_name, "failed to persist skill-selection causal trace: {error}");
        }

        // Create audit entry for skill selection per D-06/TRNS-03.
        let config = self.config.read().await.clone();
        if config.audit.scope.skill {
            let confidence_val = trace.selected.estimated_success_prob;
            let data_json = serde_json::json!({
                "skill_name": selected_variant.skill_name,
                "confidence": confidence_val.map(|p| (p * 100.0).round() as u64).unwrap_or(0),
                "rejected_count": trace.rejected_options.len(),
            });
            let summary = match generate_explanation("skill_selection", &data_json) {
                ExplanationResult::Template(s) => s,
                ExplanationResult::NeedsLlm => format!(
                    "Selected skill \"{}\" for thread {}",
                    selected_variant.skill_name, thread_id
                ),
            };
            let audit_entry = AuditEntryRow {
                id: format!("audit-skill-{}", trace.trace_id),
                timestamp: trace.created_at as i64,
                action_type: "skill".to_string(),
                summary: summary.clone(),
                explanation: Some(summary),
                confidence: confidence_val,
                confidence_band: confidence_val.map(|p| confidence_band(p).as_str().to_string()),
                causal_trace_id: Some(trace.trace_id.clone()),
                thread_id: Some(thread_id.to_string()),
                goal_run_id: goal_run_id.map(str::to_string),
                task_id: task_id.map(str::to_string),
                raw_data_json: serde_json::to_string(&data_json).ok(),
            };
            if let Err(e) = self.history.insert_action_audit(&audit_entry).await {
                tracing::warn!(thread_id = %thread_id, "failed to insert skill audit entry: {e}");
            }
            let _ = self.event_tx.send(AgentEvent::AuditAction {
                id: audit_entry.id,
                timestamp: trace.created_at,
                action_type: audit_entry.action_type,
                summary: audit_entry.summary,
                explanation: audit_entry.explanation,
                confidence: audit_entry.confidence,
                confidence_band: audit_entry.confidence_band,
                causal_trace_id: audit_entry.causal_trace_id,
                thread_id: audit_entry.thread_id,
            });
        }
    }

    pub(super) async fn settle_skill_selection_causal_traces(
        &self,
        thread_id: Option<&str>,
        task_id: Option<&str>,
        goal_run_id: Option<&str>,
        outcome: &str,
    ) -> usize {
        let outcome_json = match outcome {
            "success" => {
                serde_json::to_string(&crate::agent::learning::traces::CausalTraceOutcome::Success)
                    .unwrap_or_default()
            }
            "failure" => serde_json::to_string(
                &crate::agent::learning::traces::CausalTraceOutcome::Failure {
                    reason: "selected skill guidance did not lead to successful completion"
                        .to_string(),
                },
            )
            .unwrap_or_default(),
            "cancelled" => serde_json::to_string(
                &crate::agent::learning::traces::CausalTraceOutcome::Failure {
                    reason: "work was cancelled before validating the selected skill guidance"
                        .to_string(),
                },
            )
            .unwrap_or_default(),
            _ => return 0,
        };
        self.history
            .settle_skill_selection_causal_traces(thread_id, task_id, goal_run_id, &outcome_json)
            .await
            .unwrap_or(0)
    }

    pub(super) async fn settle_goal_plan_causal_traces(
        &self,
        goal_run_id: &str,
        outcome: &str,
        reason: Option<&str>,
    ) -> usize {
        let outcome_json = match outcome {
            "success" => {
                serde_json::to_string(&crate::agent::learning::traces::CausalTraceOutcome::Success)
                    .unwrap_or_default()
            }
            "failure" => serde_json::to_string(
                &crate::agent::learning::traces::CausalTraceOutcome::Failure {
                    reason: reason
                        .filter(|value| !value.trim().is_empty())
                        .unwrap_or("goal run failed before validating the selected plan")
                        .to_string(),
                },
            )
            .unwrap_or_default(),
            "cancelled" => serde_json::to_string(
                &crate::agent::learning::traces::CausalTraceOutcome::Failure {
                    reason: reason
                        .filter(|value| !value.trim().is_empty())
                        .unwrap_or("goal run was cancelled before validating the selected plan")
                        .to_string(),
                },
            )
            .unwrap_or_default(),
            _ => return 0,
        };
        self.history
            .settle_goal_plan_causal_traces(goal_run_id, &outcome_json)
            .await
            .unwrap_or(0)
    }

    pub(super) async fn persist_recovery_near_miss_trace(
        &self,
        goal_run: &GoalRun,
        failed_task: &AgentTask,
        failure: &str,
        revised: &GoalPlanResponse,
    ) {
        let checkpoint_count = self
            .history
            .list_checkpoints_for_goal_run(&goal_run.id)
            .await
            .map(|items| items.len())
            .unwrap_or(0);
        let has_checkpoint = checkpoint_count > 0;

        let mut factors = vec![crate::agent::learning::traces::CausalFactor {
            factor_type: crate::agent::learning::traces::FactorType::PastFailure,
            description: format!(
                "step '{}' failed and triggered recovery: {}",
                failed_task
                    .goal_step_title
                    .as_deref()
                    .unwrap_or(&failed_task.title),
                crate::agent::summarize_text(failure, 180)
            ),
            weight: 0.9,
        }];
        factors.push(crate::agent::learning::traces::CausalFactor {
            factor_type: crate::agent::learning::traces::FactorType::PatternMatch,
            description: format!(
                "recovered by revising the plan into {} follow-up step(s)",
                revised.steps.len()
            ),
            weight: 0.6,
        });
        factors.push(crate::agent::learning::traces::CausalFactor {
            factor_type: crate::agent::learning::traces::FactorType::ResourceConstraint,
            description: if has_checkpoint {
                format!(
                    "{} checkpoint(s) were available, but recovery stayed in-thread via replan",
                    checkpoint_count
                )
            } else {
                "no checkpoint was available, so recovery used in-thread replanning".to_string()
            },
            weight: 0.4,
        });

        let selected = crate::agent::learning::traces::DecisionOption {
            option_type: "replan_after_failure".to_string(),
            reasoning: crate::agent::summarize_text(&revised.summary, 220),
            rejection_reason: None,
            estimated_success_prob: Some(estimate_plan_success(
                revised.steps.len(),
                revised
                    .steps
                    .iter()
                    .filter(|step| step.kind == GoalRunStepKind::Command)
                    .count(),
            )),
            arguments_hash: Some(crate::agent::learning::traces::hash_context_blob(&format!(
                "{}|{}",
                goal_run.id, failed_task.id
            ))),
        };
        let outcome = crate::agent::learning::traces::CausalTraceOutcome::NearMiss {
            what_went_wrong: crate::agent::summarize_text(failure, 220),
            how_recovered: format!(
                "Goal run continued with a revised plan and next step '{}'.",
                revised
                    .steps
                    .first()
                    .map(|step| step.title.as_str())
                    .unwrap_or("follow-up work")
            ),
        };
        let config = self.config.read().await.clone();
        let trace = crate::agent::learning::traces::CausalTrace {
            trace_id: format!("causal_{}", uuid::Uuid::new_v4()),
            thread_id: goal_run.thread_id.clone(),
            goal_run_id: Some(goal_run.id.clone()),
            task_id: Some(failed_task.id.clone()),
            decision_type: crate::agent::learning::traces::DecisionType::Recovery,
            selected,
            rejected_options: Vec::new(),
            context_hash: crate::agent::learning::traces::hash_context_blob(&format!(
                "{}|{}|{}",
                goal_run.goal, failed_task.title, failure
            )),
            causal_factors: factors,
            outcome,
            model_used: Some(config.model),
            created_at: now_millis(),
        };

        let selected_json = serde_json::to_string(&trace.selected).unwrap_or_default();
        let rejected_json = serde_json::to_string(&trace.rejected_options).unwrap_or_default();
        let factors_json = serde_json::to_string(&trace.causal_factors).unwrap_or_default();
        let outcome_json = serde_json::to_string(&trace.outcome).unwrap_or_default();
        if let Err(error) = self
            .history
            .insert_causal_trace(
                &trace.trace_id,
                trace.thread_id.as_deref(),
                trace.goal_run_id.as_deref(),
                trace.task_id.as_deref(),
                "recovery",
                &selected_json,
                &rejected_json,
                &trace.context_hash,
                &factors_json,
                &outcome_json,
                trace.model_used.as_deref(),
                trace.created_at,
            )
            .await
        {
            tracing::warn!(goal_run_id = %goal_run.id, task_id = %failed_task.id, "failed to persist recovery near-miss trace: {error}");
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct CommandBlastRadiusAdvisory {
    pub family: String,
    pub risk_level: String,
    pub evidence: String,
    pub recent_reasons: Vec<String>,
}

#[cfg(test)]
#[path = "tests/causal_traces.rs"]
mod tests;
