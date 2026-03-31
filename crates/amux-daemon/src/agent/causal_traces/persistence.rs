use super::helpers::{
    command_family, command_family_from_tool_args, estimate_plan_success,
    estimated_success_probability,
};
use super::*;

impl AgentEngine {
    pub(in crate::agent) async fn command_blast_radius_advisory(
        &self,
        tool_name: &str,
        command: &str,
    ) -> Option<CommandBlastRadiusAdvisory> {
        let family = command_family(command);
        let records = self
            .history
            .list_recent_causal_trace_records(tool_name, 64)
            .await
            .ok()?;
        let mut failure_count = 0u32;
        let mut near_miss_count = 0u32;
        let mut recent_reasons = Vec::new();

        for record in records {
            let Ok(factors) = serde_json::from_str::<
                Vec<crate::agent::learning::traces::CausalFactor>,
            >(&record.causal_factors_json) else {
                continue;
            };
            if !factors.iter().any(|factor| {
                factor.factor_type == crate::agent::learning::traces::FactorType::PatternMatch
                    && factor.description == format!("command family: {family}")
            }) {
                continue;
            }

            let Ok(outcome) = serde_json::from_str::<
                crate::agent::learning::traces::CausalTraceOutcome,
            >(&record.outcome_json) else {
                continue;
            };
            match outcome {
                crate::agent::learning::traces::CausalTraceOutcome::Failure { reason } => {
                    failure_count = failure_count.saturating_add(1);
                    if recent_reasons.len() < 3 {
                        recent_reasons.push(reason);
                    }
                }
                crate::agent::learning::traces::CausalTraceOutcome::NearMiss {
                    what_went_wrong,
                    ..
                } => {
                    near_miss_count = near_miss_count.saturating_add(1);
                    if recent_reasons.len() < 3 {
                        recent_reasons.push(what_went_wrong);
                    }
                }
                _ => {}
            }
        }

        let risk = match failure_count + near_miss_count {
            0 => return None,
            1 => "medium",
            2 => "high",
            _ => "high",
        };
        Some(CommandBlastRadiusAdvisory {
            family,
            risk_level: risk.to_string(),
            evidence: format!(
                "Similar `{}` operations had {} failure(s) and {} near-miss(es) in recent causal history.",
                command.split_whitespace().take(2).collect::<Vec<_>>().join(" "),
                failure_count,
                near_miss_count
            ),
            recent_reasons,
        })
    }

    pub(in crate::agent) async fn persist_goal_plan_causal_trace(
        &self,
        goal_run: &GoalRun,
        plan: &GoalPlanResponse,
        failure: Option<&str>,
    ) {
        let decision_type = if failure.is_some() {
            crate::agent::learning::traces::DecisionType::ReplanSelection
        } else {
            crate::agent::learning::traces::DecisionType::PlanSelection
        };
        let decision_label = match decision_type {
            crate::agent::learning::traces::DecisionType::PlanSelection => "plan_selection",
            crate::agent::learning::traces::DecisionType::ReplanSelection => "replan_selection",
            _ => "plan_selection",
        };

        let step_count = plan.steps.len();
        let command_steps = plan
            .steps
            .iter()
            .filter(|step| step.kind == GoalRunStepKind::Command)
            .count();
        let research_steps = plan
            .steps
            .iter()
            .filter(|step| step.kind == GoalRunStepKind::Research)
            .count();
        let operator_risk = {
            let model = self.operator_model.read().await;
            model.risk_fingerprint.risk_tolerance
        };

        let mut factors = vec![crate::agent::learning::traces::CausalFactor {
            factor_type: crate::agent::learning::traces::FactorType::PatternMatch,
            description: format!(
                "plan uses {step_count} step(s) with {command_steps} command step(s) and {research_steps} research step(s)"
            ),
            weight: 0.6,
        }];
        factors.push(crate::agent::learning::traces::CausalFactor {
            factor_type: crate::agent::learning::traces::FactorType::OperatorPreference,
            description: format!(
                "operator risk profile is {}",
                match operator_risk {
                    RiskTolerance::Conservative => "conservative",
                    RiskTolerance::Moderate => "moderate",
                    RiskTolerance::Aggressive => "aggressive",
                }
            ),
            weight: 0.2,
        });
        if let Some(failure) = failure.filter(|value| !value.trim().is_empty()) {
            factors.push(crate::agent::learning::traces::CausalFactor {
                factor_type: crate::agent::learning::traces::FactorType::PastFailure,
                description: format!(
                    "replan was triggered by: {}",
                    crate::agent::summarize_text(failure, 180)
                ),
                weight: 0.7,
            });
        }

        let selected = crate::agent::learning::traces::DecisionOption {
            option_type: if failure.is_some() {
                "goal_replan".to_string()
            } else {
                "goal_plan".to_string()
            },
            reasoning: crate::agent::summarize_text(&plan.summary, 240),
            rejection_reason: None,
            estimated_success_prob: Some(estimate_plan_success(step_count, command_steps)),
            arguments_hash: Some(crate::agent::learning::traces::hash_context_blob(
                &serde_json::to_string(&plan.steps).unwrap_or_default(),
            )),
        };

        let rejected_options: Vec<crate::agent::learning::traces::DecisionOption> = plan
            .rejected_alternatives
            .iter()
            .map(|alt| crate::agent::learning::traces::DecisionOption {
                option_type: "plan_alternative".to_string(),
                reasoning: alt.clone(),
                rejection_reason: None,
                estimated_success_prob: None,
                arguments_hash: None,
            })
            .collect();

        let context_hash = crate::agent::learning::traces::hash_context_blob(&format!(
            "{}|{}|{}|{}",
            goal_run.goal,
            goal_run.current_step_index,
            plan.summary,
            failure.unwrap_or_default()
        ));
        let outcome = crate::agent::learning::traces::CausalTraceOutcome::Unresolved;
        let config = self.config.read().await.clone();
        let trace = crate::agent::learning::traces::CausalTrace {
            trace_id: format!("causal_{}", uuid::Uuid::new_v4()),
            thread_id: goal_run.thread_id.clone(),
            goal_run_id: Some(goal_run.id.clone()),
            task_id: goal_run.active_task_id.clone(),
            decision_type,
            selected,
            rejected_options,
            context_hash,
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
                decision_label,
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
            tracing::warn!(goal_run_id = %goal_run.id, trace = %decision_label, "failed to persist goal plan causal trace: {error}");
        }
    }

    pub(in crate::agent) async fn persist_tool_selection_causal_trace(
        &self,
        thread_id: &str,
        goal_run_id: Option<&str>,
        task_id: Option<&str>,
        tool_call: &ToolCall,
        reasoning: Option<&str>,
        result: &ToolResult,
        trace_collector: &crate::agent::learning::traces::TraceCollector,
        config: &AgentConfig,
        provider_config: &ProviderConfig,
    ) {
        let current_tokens = {
            let threads = self.threads.read().await;
            threads
                .get(thread_id)
                .map(|thread| estimate_message_tokens(&thread.messages))
                .unwrap_or(0)
        };
        let target_tokens = effective_context_target_tokens(config, provider_config).max(1);
        let context_ratio = current_tokens as f64 / target_tokens as f64;

        let mut factors = Vec::new();
        let prior_successes = trace_collector.success_count_for_tool(&tool_call.function.name);
        if prior_successes > 0 {
            factors.push(crate::agent::learning::traces::CausalFactor {
                factor_type: crate::agent::learning::traces::FactorType::PastSuccess,
                description: format!(
                    "{} prior successful use(s) of `{}` in the active trace",
                    prior_successes, tool_call.function.name
                ),
                weight: 0.6,
            });
        }
        let prior_failures = trace_collector.failure_count_for_tool(&tool_call.function.name);
        if prior_failures > 0 {
            factors.push(crate::agent::learning::traces::CausalFactor {
                factor_type: crate::agent::learning::traces::FactorType::PastFailure,
                description: format!(
                    "{} prior failed use(s) of `{}` in the active trace",
                    prior_failures, tool_call.function.name
                ),
                weight: 0.5,
            });
        }
        if context_ratio >= 0.7 {
            factors.push(crate::agent::learning::traces::CausalFactor {
                factor_type: crate::agent::learning::traces::FactorType::ResourceConstraint,
                description: format!(
                    "context utilization was {:.0}% of the target budget",
                    context_ratio * 100.0
                ),
                weight: 0.4,
            });
        }
        let risk_tolerance = {
            let model = self.operator_model.read().await;
            model.risk_fingerprint.risk_tolerance
        };
        if matches!(
            tool_call.function.name.as_str(),
            "bash_command" | "execute_managed_command"
        ) {
            if let Some(command_family) =
                command_family_from_tool_args(&tool_call.function.arguments)
            {
                factors.push(crate::agent::learning::traces::CausalFactor {
                    factor_type: crate::agent::learning::traces::FactorType::PatternMatch,
                    description: format!("command family: {command_family}"),
                    weight: 0.5,
                });
            }
            factors.push(crate::agent::learning::traces::CausalFactor {
                factor_type: crate::agent::learning::traces::FactorType::OperatorPreference,
                description: format!(
                    "operator risk profile is {}",
                    match risk_tolerance {
                        RiskTolerance::Conservative => "conservative",
                        RiskTolerance::Moderate => "moderate",
                        RiskTolerance::Aggressive => "aggressive",
                    }
                ),
                weight: 0.2,
            });
        }

        let selected = crate::agent::learning::traces::DecisionOption {
            option_type: tool_call.function.name.clone(),
            reasoning: crate::agent::summarize_text(reasoning.unwrap_or_default(), 240),
            rejection_reason: None,
            estimated_success_prob: Some(estimated_success_probability(
                prior_successes,
                prior_failures,
                result.is_error,
            )),
            arguments_hash: Some(crate::agent::learning::traces::hash_arguments(
                &tool_call.function.arguments,
            )),
        };

        let outcome = if result.is_error {
            crate::agent::learning::traces::CausalTraceOutcome::Failure {
                reason: crate::agent::summarize_text(&result.content, 220),
            }
        } else {
            crate::agent::learning::traces::CausalTraceOutcome::Success
        };

        let context_hash = crate::agent::learning::traces::hash_context_blob(&format!(
            "{thread_id}|{}|{}|{}",
            tool_call.function.name,
            tool_call.function.arguments,
            reasoning.unwrap_or_default()
        ));
        let trace = crate::agent::learning::traces::CausalTrace {
            trace_id: format!("causal_{}", uuid::Uuid::new_v4()),
            thread_id: Some(thread_id.to_string()),
            goal_run_id: goal_run_id.map(str::to_string),
            task_id: task_id.map(str::to_string),
            decision_type: crate::agent::learning::traces::DecisionType::ToolSelection,
            selected,
            rejected_options: Vec::new(),
            context_hash,
            causal_factors: factors,
            outcome,
            model_used: Some(provider_config.model.clone()),
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
                "tool_selection",
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
            tracing::warn!(thread_id = %thread_id, tool = %tool_call.function.name, "failed to persist causal trace: {error}");
        }

        if config.audit.scope.tool {
            let confidence_val = trace.selected.estimated_success_prob;
            let data_json = serde_json::json!({
                "tool_name": tool_call.function.name,
                "session_id": thread_id,
            });
            let summary = match generate_explanation("tool_execution", &data_json) {
                ExplanationResult::Template(summary) => summary,
                ExplanationResult::NeedsLlm => format!(
                    "Executed tool \"{}\" in thread {}",
                    tool_call.function.name, thread_id
                ),
            };
            let audit_entry = AuditEntryRow {
                id: format!("audit-tool-{}", trace.trace_id),
                timestamp: trace.created_at as i64,
                action_type: "tool".to_string(),
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
            if let Err(error) = self.history.insert_action_audit(&audit_entry).await {
                tracing::warn!(thread_id = %thread_id, "failed to insert tool audit entry: {error}");
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
}
