//! Runtime causal-trace helpers for live decision capture.

use super::*;
use crate::history::AuditEntryRow;

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
        if let Err(error) = self.history.insert_causal_trace(
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
        ).await {
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
        if let Err(error) = self.history.insert_causal_trace(
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
        ).await {
            tracing::warn!(goal_run_id = %goal_run.id, task_id = %failed_task.id, "failed to persist recovery near-miss trace: {error}");
        }
    }

    pub(super) async fn build_causal_guidance_summary(&self) -> Option<String> {
        let mut advisories = Vec::new();
        for tool_name in ["execute_managed_command", "bash_command"] {
            let records = self
                .history
                .list_recent_causal_trace_records(tool_name, 48)
                .await
                .ok()?;
            let mut by_family: HashMap<String, FamilyOutcomeSummary> = HashMap::new();
            for record in records {
                let Ok(factors) = serde_json::from_str::<
                    Vec<crate::agent::learning::traces::CausalFactor>,
                >(&record.causal_factors_json) else {
                    continue;
                };
                let family = factors.iter().find_map(pattern_family_from_factor)?;
                let Ok(outcome) = serde_json::from_str::<
                    crate::agent::learning::traces::CausalTraceOutcome,
                >(&record.outcome_json) else {
                    continue;
                };
                let Some(summary) = summarize_outcome(outcome) else {
                    continue;
                };
                let entry = by_family.entry(family).or_default();
                entry.record(summary);
            }

            for (family, summary) in by_family {
                let caution_count = summary.failure_count + summary.near_miss_count;
                if caution_count == 0 {
                    continue;
                }
                let recent_reasons = summary
                    .reasons
                    .into_iter()
                    .map(|reason| crate::agent::summarize_text(&reason, 100))
                    .collect::<Vec<_>>()
                    .join("; ");
                let recovery_hint = if summary.recoveries.is_empty() {
                    String::new()
                } else {
                    format!(
                        "; recent recovery pattern: {}",
                        summary
                            .recoveries
                            .into_iter()
                            .map(|recovery| crate::agent::summarize_text(&recovery, 100))
                            .collect::<Vec<_>>()
                            .join("; ")
                    )
                };
                advisories.push(format!(
                    "- {} / {}: {} failure(s), {} near-miss(es); watch for {}{}",
                    tool_name,
                    family,
                    summary.failure_count,
                    summary.near_miss_count,
                    recent_reasons,
                    recovery_hint
                ));
            }
        }

        if let Ok(records) = self
            .history
            .list_recent_causal_trace_records("replan_after_failure", 24)
            .await
        {
            let mut recovery_patterns = Vec::new();
            for record in records {
                let Ok(outcome) = serde_json::from_str::<
                    crate::agent::learning::traces::CausalTraceOutcome,
                >(&record.outcome_json) else {
                    continue;
                };
                let Some(summary) = summarize_outcome(outcome) else {
                    continue;
                };
                if !summary.is_near_miss {
                    continue;
                }
                if let Some(recovery) = summary.recovery {
                    if recovery_patterns.len() < 2 {
                        recovery_patterns.push(crate::agent::summarize_text(&recovery, 110));
                    }
                }
            }
            if !recovery_patterns.is_empty() {
                advisories.push(format!(
                    "- recovery: recent near-miss replans recovered via {}",
                    recovery_patterns.join("; ")
                ));
            }
        }

        if advisories.is_empty() {
            None
        } else {
            advisories.sort();
            advisories.truncate(3);
            Some(format!(
                "## Recent Causal Guidance\n{}",
                advisories.join("\n")
            ))
        }
    }

    pub(super) async fn command_blast_radius_advisory(
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

    pub(super) async fn persist_goal_plan_causal_trace(
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
            rejected_options: Vec::new(),
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
        if let Err(error) = self.history.insert_causal_trace(
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
        ).await {
            tracing::warn!(goal_run_id = %goal_run.id, trace = %decision_label, "failed to persist goal plan causal trace: {error}");
        }
    }

    pub(super) async fn persist_tool_selection_causal_trace(
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
        if let Err(error) = self.history.insert_causal_trace(
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
        ).await {
            tracing::warn!(thread_id = %thread_id, tool = %tool_call.function.name, "failed to persist causal trace: {error}");
        }

        // Create audit entry for tool selection per D-06/TRNS-03.
        if config.audit.scope.tool {
            let confidence_val = trace.selected.estimated_success_prob;
            let data_json = serde_json::json!({
                "tool_name": tool_call.function.name,
                "session_id": thread_id,
            });
            let summary = match generate_explanation("tool_execution", &data_json) {
                ExplanationResult::Template(s) => s,
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
            if let Err(e) = self.history.insert_action_audit(&audit_entry).await {
                tracing::warn!(thread_id = %thread_id, "failed to insert tool audit entry: {e}");
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

    pub async fn causal_trace_report(
        &self,
        option_type: &str,
        limit: u32,
    ) -> Result<serde_json::Value> {
        let rows = self
            .history
            .list_causal_traces_for_option(option_type, limit.max(1)).await?;

        let mut success = 0u32;
        let mut failure = 0u32;
        let mut near_miss = 0u32;
        let mut unresolved = 0u32;
        let mut recent_failure_reasons = Vec::new();
        let mut recent_recoveries = Vec::new();

        for row in rows {
            let Ok(outcome) =
                serde_json::from_str::<crate::agent::learning::traces::CausalTraceOutcome>(&row)
            else {
                continue;
            };
            match outcome {
                crate::agent::learning::traces::CausalTraceOutcome::Success => success += 1,
                crate::agent::learning::traces::CausalTraceOutcome::Failure { reason } => {
                    failure += 1;
                    if recent_failure_reasons.len() < 5 {
                        recent_failure_reasons.push(reason);
                    }
                }
                crate::agent::learning::traces::CausalTraceOutcome::NearMiss {
                    what_went_wrong,
                    how_recovered,
                } => {
                    near_miss += 1;
                    if recent_failure_reasons.len() < 5 {
                        recent_failure_reasons.push(what_went_wrong);
                    }
                    if recent_recoveries.len() < 5 {
                        recent_recoveries.push(how_recovered);
                    }
                }
                crate::agent::learning::traces::CausalTraceOutcome::Unresolved => unresolved += 1,
            }
        }

        let resolved = success + failure + near_miss;
        let success_rate = if resolved == 0 {
            0.0
        } else {
            success as f64 / resolved as f64
        };

        Ok(serde_json::json!({
            "option_type": option_type,
            "sample_size": success + failure + near_miss + unresolved,
            "success": success,
            "failure": failure,
            "near_miss": near_miss,
            "unresolved": unresolved,
            "success_rate": success_rate,
            "recent_failure_reasons": recent_failure_reasons,
            "recent_recoveries": recent_recoveries,
        }))
    }

    pub async fn counterfactual_report(
        &self,
        option_type: &str,
        family_hint: &str,
        limit: u32,
    ) -> Result<serde_json::Value> {
        let normalized_family = command_family(family_hint);
        let records = self
            .history
            .list_recent_causal_trace_records(option_type, limit.max(1)).await?;
        let mut success = 0u32;
        let mut failure = 0u32;
        let mut near_miss = 0u32;
        let mut unresolved = 0u32;
        let mut recent_reasons = Vec::new();
        let mut recent_recoveries = Vec::new();

        for record in records {
            let Ok(factors) = serde_json::from_str::<
                Vec<crate::agent::learning::traces::CausalFactor>,
            >(&record.causal_factors_json) else {
                continue;
            };
            let Some(family) = factors.iter().find_map(pattern_family_from_factor) else {
                continue;
            };
            if family != normalized_family {
                continue;
            }

            let Ok(outcome) = serde_json::from_str::<
                crate::agent::learning::traces::CausalTraceOutcome,
            >(&record.outcome_json) else {
                continue;
            };
            match outcome {
                crate::agent::learning::traces::CausalTraceOutcome::Success => success += 1,
                crate::agent::learning::traces::CausalTraceOutcome::Failure { reason } => {
                    failure += 1;
                    if recent_reasons.len() < 4 {
                        recent_reasons.push(reason);
                    }
                }
                crate::agent::learning::traces::CausalTraceOutcome::NearMiss {
                    what_went_wrong,
                    how_recovered,
                } => {
                    near_miss += 1;
                    if recent_reasons.len() < 4 {
                        recent_reasons.push(what_went_wrong);
                    }
                    if recent_recoveries.len() < 4 {
                        recent_recoveries.push(how_recovered);
                    }
                }
                crate::agent::learning::traces::CausalTraceOutcome::Unresolved => unresolved += 1,
            }
        }

        let resolved = success + failure + near_miss;
        let likely_risk = if failure + near_miss >= 3 {
            "high"
        } else if failure + near_miss >= 1 {
            "medium"
        } else {
            "low"
        };
        let recommendation = if resolved == 0 {
            "no matching historical evidence"
        } else if success > failure + near_miss {
            "history leans favorable, but review recent cautions"
        } else {
            "history leans risky; prefer tighter scoping or approval-first execution"
        };

        Ok(serde_json::json!({
            "option_type": option_type,
            "command_family": normalized_family,
            "sample_size": success + failure + near_miss + unresolved,
            "success": success,
            "failure": failure,
            "near_miss": near_miss,
            "unresolved": unresolved,
            "likely_risk": likely_risk,
            "recommendation": recommendation,
            "recent_failure_reasons": recent_reasons,
            "recent_recoveries": recent_recoveries,
        }))
    }
}

#[derive(Debug, Clone)]
pub(super) struct CommandBlastRadiusAdvisory {
    pub family: String,
    pub risk_level: String,
    pub evidence: String,
    pub recent_reasons: Vec<String>,
}

#[derive(Debug, Default)]
struct FamilyOutcomeSummary {
    failure_count: u32,
    near_miss_count: u32,
    reasons: Vec<String>,
    recoveries: Vec<String>,
}

impl FamilyOutcomeSummary {
    fn record(&mut self, summary: OutcomeSummary) {
        if summary.is_near_miss {
            self.near_miss_count = self.near_miss_count.saturating_add(1);
        } else {
            self.failure_count = self.failure_count.saturating_add(1);
        }
        if self.reasons.len() < 2 {
            self.reasons.push(summary.reason);
        }
        if let Some(recovery) = summary.recovery {
            if self.recoveries.len() < 2 {
                self.recoveries.push(recovery);
            }
        }
    }
}

#[derive(Debug)]
struct OutcomeSummary {
    reason: String,
    recovery: Option<String>,
    is_near_miss: bool,
}

fn estimated_success_probability(
    prior_successes: usize,
    prior_failures: usize,
    is_error: bool,
) -> f64 {
    let total = prior_successes + prior_failures;
    if total == 0 {
        return if is_error { 0.35 } else { 0.65 };
    }
    let historical = prior_successes as f64 / total as f64;
    if is_error {
        (historical * 0.8).clamp(0.0, 1.0)
    } else {
        historical.clamp(0.0, 1.0)
    }
}

fn estimate_plan_success(step_count: usize, command_steps: usize) -> f64 {
    let complexity_penalty = ((step_count.saturating_sub(2)) as f64 * 0.08).min(0.32);
    let command_penalty = (command_steps as f64 * 0.05).min(0.2);
    (0.82 - complexity_penalty - command_penalty).clamp(0.2, 0.9)
}

fn command_family_from_tool_args(arguments_json: &str) -> Option<String> {
    let parsed = serde_json::from_str::<serde_json::Value>(arguments_json).ok()?;
    let command = parsed.get("command")?.as_str()?;
    Some(command_family(command))
}

fn command_family(command: &str) -> String {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return "unknown".to_string();
    }
    let tokens = trimmed.split_whitespace().take(2).collect::<Vec<_>>();
    let family = tokens.join(" ");
    family
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect()
}

fn pattern_family_from_factor(
    factor: &crate::agent::learning::traces::CausalFactor,
) -> Option<String> {
    if factor.factor_type != crate::agent::learning::traces::FactorType::PatternMatch {
        return None;
    }
    factor
        .description
        .strip_prefix("command family: ")
        .map(str::to_string)
}

fn summarize_outcome(
    outcome: crate::agent::learning::traces::CausalTraceOutcome,
) -> Option<OutcomeSummary> {
    match outcome {
        crate::agent::learning::traces::CausalTraceOutcome::Failure { reason } => {
            Some(OutcomeSummary {
                reason,
                recovery: None,
                is_near_miss: false,
            })
        }
        crate::agent::learning::traces::CausalTraceOutcome::NearMiss {
            what_went_wrong,
            how_recovered,
        } => Some(OutcomeSummary {
            reason: what_went_wrong,
            recovery: Some(how_recovered),
            is_near_miss: true,
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimated_success_probability_defaults_when_no_history() {
        assert!((estimated_success_probability(0, 0, false) - 0.65).abs() < f64::EPSILON);
        assert!((estimated_success_probability(0, 0, true) - 0.35).abs() < f64::EPSILON);
    }

    #[test]
    fn plan_success_estimate_decreases_with_complexity() {
        assert!(estimate_plan_success(2, 0) > estimate_plan_success(6, 3));
    }

    #[test]
    fn command_family_normalizes_prefix() {
        assert_eq!(command_family("git push origin main"), "git_push");
        assert_eq!(command_family("rm -rf build"), "rm__rf");
    }

    #[test]
    fn summarize_outcome_preserves_recovery_for_near_miss() {
        let summary = summarize_outcome(
            crate::agent::learning::traces::CausalTraceOutcome::NearMiss {
                what_went_wrong: "command timed out".to_string(),
                how_recovered: "replanned into smaller steps".to_string(),
            },
        )
        .expect("near miss should summarize");

        assert!(summary.is_near_miss);
        assert_eq!(summary.reason, "command timed out");
        assert_eq!(
            summary.recovery.as_deref(),
            Some("replanned into smaller steps")
        );
    }

    #[test]
    fn family_outcome_summary_tracks_failures_and_near_misses() {
        let mut summary = FamilyOutcomeSummary::default();
        summary.record(OutcomeSummary {
            reason: "permissions denied".to_string(),
            recovery: None,
            is_near_miss: false,
        });
        summary.record(OutcomeSummary {
            reason: "command timed out".to_string(),
            recovery: Some("replanned into smaller steps".to_string()),
            is_near_miss: true,
        });

        assert_eq!(summary.failure_count, 1);
        assert_eq!(summary.near_miss_count, 1);
        assert_eq!(summary.reasons.len(), 2);
        assert_eq!(summary.recoveries, vec!["replanned into smaller steps"]);
    }
}
