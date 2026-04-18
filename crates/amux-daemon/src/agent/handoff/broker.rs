//! Handoff broker orchestration — routes tasks to specialist subagents,
//! assembles context bundles, and validates specialist output.
//!
//! This module wires together all Plan 01 types and Plan 02 logic into
//! the live agent pipeline, making handoffs work end-to-end.

use anyhow::{Context, Result};
use uuid::Uuid;

use super::profiles::{match_specialist, select_specialist};
use super::{AcceptanceCriteria, ContextBundle, HandoffResult, RoutingMethod, ValidationResult};
use crate::agent::background_workers::domain_routing::select_snapshot_candidate;
use crate::agent::background_workers::protocol::{
    BackgroundWorkerCommand, BackgroundWorkerKind, BackgroundWorkerResult,
};
use crate::agent::background_workers::run_background_worker_command;
use crate::agent::engine::AgentEngine;
use crate::agent::types::RoutingMode;
use crate::agent::TaskStatus;

/// Maximum handoff depth before escalating to operator (HAND-08).
const MAX_HANDOFF_DEPTH: u8 = 3;

/// Token ceiling for context bundles (HAND-03 locked decision).
const CONTEXT_BUNDLE_TOKEN_CEILING: u32 = 2000;

/// Maximum episodic refs to include in a context bundle.
const MAX_EPISODIC_REFS: usize = 3;

fn validate_match_threshold(threshold: f64) -> Result<()> {
    if !threshold.is_finite() {
        anyhow::bail!("handoff match_threshold must be finite");
    }
    if !(0.0..=1.0).contains(&threshold) {
        anyhow::bail!("handoff match_threshold must be within 0.0..=1.0, got {threshold}");
    }
    Ok(())
}

impl AgentEngine {
    /// Assemble a context bundle for a specialist handoff.
    ///
    /// Pulls episodic refs, negative constraints, parent context summary,
    /// and partial outputs into a token-limited bundle.
    pub(crate) async fn assemble_context_bundle(
        &self,
        task_description: &str,
        parent_task_id: Option<&str>,
        _goal_run_id: Option<&str>,
        thread_id: &str,
        acceptance_criteria_str: &str,
        current_depth: u8,
    ) -> Result<ContextBundle> {
        let mut bundle = ContextBundle::new(
            task_description.to_string(),
            acceptance_criteria_str.to_string(),
        );
        bundle.handoff_depth = current_depth;

        if let Some(snapshot) = self
            .get_resonance_context_snapshot(task_description, Some(thread_id), parent_task_id)
            .await
        {
            bundle.episodic_refs = snapshot.episodic_refs;
            bundle.negative_constraints = snapshot.negative_constraints;
            if !snapshot.collaboration_context.is_empty() {
                bundle.partial_outputs.push(super::PartialOutput {
                    step_index: 0,
                    content: serde_json::to_string(&snapshot.collaboration_context)
                        .unwrap_or_else(|_| "[]".to_string()),
                    status: "resonance_shared_context".to_string(),
                });
            }
            if let Some(structural_memory) = snapshot.structural_memory {
                bundle.partial_outputs.push(super::PartialOutput {
                    step_index: 0,
                    content: serde_json::to_string(&structural_memory)
                        .unwrap_or_else(|_| "{}".to_string()),
                    status: "resonance_structural_memory".to_string(),
                });
            }
        } else {
            let snapshot = self
                .build_resonance_context_snapshot(
                    task_description,
                    Some(thread_id),
                    parent_task_id,
                    MAX_EPISODIC_REFS,
                )
                .await;
            bundle.episodic_refs = snapshot.episodic_refs.clone();
            bundle.negative_constraints = snapshot.negative_constraints.clone();
            if !snapshot.collaboration_context.is_empty() {
                bundle.partial_outputs.push(super::PartialOutput {
                    step_index: 0,
                    content: serde_json::to_string(&snapshot.collaboration_context)
                        .unwrap_or_else(|_| "[]".to_string()),
                    status: "resonance_shared_context".to_string(),
                });
            }
            if let Some(structural_memory) = snapshot.structural_memory.clone() {
                bundle.partial_outputs.push(super::PartialOutput {
                    step_index: 0,
                    content: serde_json::to_string(&structural_memory)
                        .unwrap_or_else(|_| "{}".to_string()),
                    status: "resonance_structural_memory".to_string(),
                });
            }
            self.put_resonance_context_snapshot(
                task_description,
                Some(thread_id),
                parent_task_id,
                snapshot,
            )
            .await;
        }

        // 1. Pull partial outputs from parent task if available
        if let Some(ptid) = parent_task_id {
            let tasks = self.tasks.lock().await;
            if let Some(parent) = tasks.iter().find(|t| t.id == ptid) {
                if let Some(ref result) = parent.result {
                    bundle.partial_outputs.push(super::PartialOutput {
                        step_index: 0,
                        content: result.clone(),
                        status: format!("{:?}", parent.status),
                    });
                }
            }
        }

        // 2. Parent context summary: last few messages from the thread
        {
            let threads = self.threads.read().await;
            if let Some(thread) = threads.get(thread_id) {
                let recent: Vec<&str> = thread
                    .messages
                    .iter()
                    .rev()
                    .take(3)
                    .map(|m| m.content.as_str())
                    .collect();
                let joined = recent.into_iter().rev().collect::<Vec<_>>().join(" | ");
                bundle.parent_context_summary =
                    crate::agent::goal_parsing::summarize_text(&joined, 500);
            }
        }

        // 3. Enforce token ceiling per HAND-03
        bundle.recompute_estimated_tokens();
        bundle.enforce_token_ceiling(CONTEXT_BUNDLE_TOKEN_CEILING);

        Ok(bundle)
    }

    /// Route a task to a specialist subagent through the handoff broker.
    ///
    /// Flow: check depth -> match specialist -> assemble bundle -> audit -> enqueue task
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn route_handoff(
        &self,
        task_description: &str,
        capability_tags: &[String],
        parent_task_id: Option<&str>,
        goal_run_id: Option<&str>,
        thread_id: &str,
        acceptance_criteria_str: &str,
        current_depth: u8,
    ) -> Result<HandoffResult> {
        // Depth check (HAND-08)
        if current_depth >= MAX_HANDOFF_DEPTH {
            anyhow::bail!(
                "Handoff depth limit ({MAX_HANDOFF_DEPTH} hops) reached -- escalating to operator"
            );
        }

        // Read broker profiles
        let broker = self.handoff_broker.read().await;
        let profiles = broker.profiles.clone();
        let threshold = broker.match_threshold;
        drop(broker);
        validate_match_threshold(threshold)?;
        let routing_cfg = self.config.read().await.routing.clone();
        let required_tags = if capability_tags.is_empty() {
            crate::agent::morphogenesis::task_router::classify_domains(
                task_description,
                capability_tags,
            )
        } else {
            capability_tags.to_vec()
        };

        let occupied_specialists = {
            let tasks = self.tasks.lock().await;
            tasks
                .iter()
                .filter(|task| {
                    task.source == "handoff"
                        && matches!(
                            task.status,
                            TaskStatus::InProgress | TaskStatus::AwaitingApproval
                        )
                })
                .filter_map(|task| {
                    task.title
                        .strip_prefix('[')
                        .and_then(|rest| rest.split(']').next())
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(ToOwned::to_owned)
                })
                .collect::<std::collections::HashSet<_>>()
        };
        let available_profiles = profiles
            .iter()
            .filter(|profile| !occupied_specialists.contains(profile.role.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        let routing_profiles = if available_profiles.is_empty() {
            profiles.clone()
        } else {
            available_profiles
        };

        let routing_snapshot =
            if routing_cfg.enabled && matches!(routing_cfg.method, RoutingMode::Probabilistic) {
                let score_rows = self
                    .load_capability_score_rows(&required_tags)
                    .await
                    .context("loading capability score rows for handoff routing")?;
                let morphogenesis = self
                    .load_morphogenesis_affinities(&required_tags)
                    .await
                    .context("loading morphogenesis affinities for handoff routing")?;
                let result = run_background_worker_command(
                    BackgroundWorkerKind::Routing,
                    BackgroundWorkerCommand::TickRouting {
                        profiles: routing_profiles.clone(),
                        required_tags: required_tags.clone(),
                        score_rows,
                        morphogenesis,
                        routing: routing_cfg.clone(),
                        now_ms: crate::history::now_ts() * 1000,
                    },
                )
                .await
                .context("building worker-backed routing snapshot")?;

                match result {
                    BackgroundWorkerResult::RoutingTick { snapshot } => Some(snapshot),
                    BackgroundWorkerResult::Error { message } => {
                        anyhow::bail!("routing worker returned error: {message}");
                    }
                    other => {
                        anyhow::bail!("routing worker returned unexpected response: {other:?}");
                    }
                }
            } else {
                None
            };

        let learned_selection = routing_snapshot.as_ref().and_then(|snapshot| {
            select_snapshot_candidate(snapshot, routing_cfg.confidence_threshold)
        });

        // Match specialist
        let (selection, routing_confidence_threshold) =
            if let Some((profile_idx, routing_score)) = learned_selection {
                (
                    super::profiles::RoutingSelection {
                        profile_idx,
                        routing_method: RoutingMethod::Probabilistic,
                        routing_score,
                        fallback_used: false,
                    },
                    routing_cfg.confidence_threshold,
                )
            } else if matches!(routing_cfg.method, RoutingMode::Probabilistic) {
                if let Some((idx, score)) =
                    match_specialist(&routing_profiles, &required_tags, threshold)
                {
                    (
                        super::profiles::RoutingSelection {
                            profile_idx: idx,
                            routing_method: RoutingMethod::Deterministic,
                            routing_score: score,
                            fallback_used: true,
                        },
                        routing_cfg.confidence_threshold,
                    )
                } else {
                    (
                        super::profiles::RoutingSelection {
                            profile_idx: routing_profiles.len().saturating_sub(1),
                            routing_method: RoutingMethod::Deterministic,
                            routing_score: 0.0,
                            fallback_used: true,
                        },
                        routing_cfg.confidence_threshold,
                    )
                }
            } else if matches!(routing_cfg.method, RoutingMode::Deterministic) {
                if let Some((idx, score)) =
                    match_specialist(&routing_profiles, capability_tags, threshold)
                {
                    (
                        super::profiles::RoutingSelection {
                            profile_idx: idx,
                            routing_method: RoutingMethod::Deterministic,
                            routing_score: score,
                            fallback_used: false,
                        },
                        threshold,
                    )
                } else {
                    (
                        super::profiles::RoutingSelection {
                            profile_idx: routing_profiles.len().saturating_sub(1),
                            routing_method: RoutingMethod::Deterministic,
                            routing_score: 0.0,
                            fallback_used: true,
                        },
                        threshold,
                    )
                }
            } else {
                let fallback = select_specialist(&routing_profiles, capability_tags, threshold)
                    .context("selecting specialist profile for handoff")?;
                (fallback, threshold)
            };
        let profile_idx = selection.profile_idx;
        let routing_method = selection.routing_method;
        let routing_score = selection.routing_score;
        let fallback_used = selection.fallback_used;

        let specialist = &routing_profiles[profile_idx];
        let specialist_id = specialist.id.clone();
        let specialist_name = specialist.name.clone();
        let specialist_role = specialist.role.clone();
        let system_prompt_snippet = specialist.system_prompt_snippet.clone();
        let routing_rationale = if fallback_used {
            format!(
                "routing fallback selected {} via {} routing because no available candidate cleared the threshold (score {:.2})",
                specialist_name,
                routing_method.as_str(),
                routing_score,
            )
        } else if routing_profiles.len() != profiles.len() {
            format!(
                "routing selected {} via {} routing with score {:.2} after availability filtering skipped occupied specialists",
                specialist_name,
                routing_method.as_str(),
                routing_score,
            )
        } else {
            format!(
                "routing selected {} via {} routing with score {:.2}",
                specialist_name,
                routing_method.as_str(),
                routing_score,
            )
        };
        let matched_capability_tags = capability_tags
            .iter()
            .filter(|required_tag| {
                specialist
                    .capabilities
                    .iter()
                    .any(|capability| capability.tag == required_tag.as_str())
            })
            .cloned()
            .collect::<Vec<_>>();
        let specialization_diagnostics = serde_json::json!({
            "matched_capability_tags": matched_capability_tags,
            "learned_routing_influenced": matches!(routing_method, RoutingMethod::Probabilistic),
            "morphogenesis_influenced": routing_snapshot.as_ref().is_some_and(|snapshot| {
                snapshot
                    .ranked_candidates
                    .iter()
                    .any(|candidate| candidate.morphogenesis_affinity > 0.0)
            }),
            "fallback_used": fallback_used,
            "availability_filtered": routing_profiles.len() != profiles.len(),
            "occupied_specialists": occupied_specialists,
            "routing_snapshot_top_candidate": routing_snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.ranked_candidates.first())
                .map(|candidate| serde_json::json!({
                    "profile_id": candidate.profile_id,
                    "learned_weight": candidate.learned_weight,
                    "morphogenesis_affinity": candidate.morphogenesis_affinity,
                    "final_weight": candidate.final_weight,
                }))
                .unwrap_or(serde_json::Value::Null),
            "routing_confidence": {
                "score": routing_score,
                "threshold": routing_confidence_threshold,
                "cleared_threshold": !fallback_used && routing_score >= routing_confidence_threshold,
            },
        });

        // Assemble context bundle
        let bundle = self
            .assemble_context_bundle(
                task_description,
                parent_task_id,
                goal_run_id,
                thread_id,
                acceptance_criteria_str,
                current_depth,
            )
            .await
            .context("assembling context bundle for handoff")?;

        let bundle_tokens = bundle.estimated_tokens;

        // Generate handoff log ID
        let handoff_log_id = Uuid::new_v4().to_string();

        // Serialize bundle and criteria for audit
        let bundle_json = serde_json::to_string(&bundle).unwrap_or_else(|_| "{}".to_string());
        let criteria_json = serde_json::to_string(&AcceptanceCriteria {
            description: acceptance_criteria_str.to_string(),
            structural_checks: vec!["non_empty".to_string()],
            require_llm_validation: false,
        })
        .unwrap_or_else(|_| "{}".to_string());
        let capability_tags_json =
            serde_json::to_string(capability_tags).unwrap_or_else(|_| "[]".to_string());

        // Log detailed handoff record
        if let Err(e) = self
            .log_handoff_detail(
                &handoff_log_id,
                parent_task_id.unwrap_or("none"),
                &specialist_id,
                None, // to_task_id not yet known
                task_description,
                &criteria_json,
                &bundle_json,
                &capability_tags_json,
                current_depth,
                "dispatched",
                None,
                routing_method.as_str(),
                routing_score,
                fallback_used,
            )
            .await
        {
            tracing::warn!("handoff broker: failed to log detail: {e}");
        }

        // Record WORM audit
        if let Err(e) = self
            .record_handoff_audit(
                parent_task_id.unwrap_or("none"),
                &specialist_id,
                "pending", // task not yet created
                task_description,
                "dispatched",
                None,
                None,
                &handoff_log_id,
                routing_method.as_str(),
                &capability_tags_json,
                routing_score,
                fallback_used,
            )
            .await
        {
            tracing::warn!("handoff broker: failed to record audit: {e}");
        }

        // Build task description with specialist context
        let full_description = {
            let mut desc = format!(
                "[Handoff to {specialist_name} ({specialist_role})]\n\n\
                 ## Task\n{task_description}\n\n\
                 ## Acceptance Criteria\n{acceptance_criteria_str}\n"
            );
            if let Some(ref snippet) = system_prompt_snippet {
                desc.push_str(&format!("\n## Specialist Instructions\n{snippet}\n"));
            }
            if !bundle.negative_constraints.is_empty() {
                desc.push_str("\n## Known Constraints (DO NOT attempt)\n");
                for constraint in &bundle.negative_constraints {
                    desc.push_str(&format!("- {constraint}\n"));
                }
            }
            if !bundle.episodic_refs.is_empty() {
                desc.push_str("\n## Relevant Past Episodes\n");
                for ep in &bundle.episodic_refs {
                    desc.push_str(&format!(
                        "- [{}] {} ({})\n",
                        ep.episode_id, ep.summary, ep.outcome
                    ));
                }
            }
            desc
        };

        // Enqueue task via existing spawn infrastructure
        let task = self
            .enqueue_task(
                format!(
                    "[{specialist_role}] {}",
                    crate::agent::goal_parsing::summarize_text(task_description, 72)
                ),
                full_description,
                "normal",
                None,                               // command
                None,                               // session_id
                Vec::new(),                         // dependencies
                None,                               // scheduled_at
                "handoff",                          // source
                goal_run_id.map(str::to_string),    // goal_run_id
                parent_task_id.map(str::to_string), // parent_task_id
                Some(thread_id.to_string()),        // parent_thread_id
                None,                               // runtime
            )
            .await;

        let task_id = task.id.clone();

        // Bind the handoff log to the actual dispatched task ID.
        if let Err(e) = self.bind_handoff_task_id(&handoff_log_id, &task_id).await {
            tracing::warn!("handoff broker: failed to bind to_task_id for handoff log: {e}");
        }
        if let Err(e) = self
            .update_handoff_outcome(&handoff_log_id, "dispatched", None, None)
            .await
        {
            tracing::warn!("handoff broker: failed to update handoff outcome: {e}");
        }

        Ok(HandoffResult {
            task_id,
            specialist_profile_id: specialist_id,
            specialist_name,
            handoff_log_id,
            context_bundle_tokens: bundle_tokens,
            routing_method,
            routing_score,
            fallback_used,
            routing_rationale,
            specialization_diagnostics,
        })
    }

    /// Validate specialist output against acceptance criteria (HAND-05).
    ///
    /// Looks up the completed task, runs structural validation, records
    /// the validation outcome to the WORM audit trail.
    pub(crate) async fn validate_specialist_output(
        &self,
        handoff_log_id: &str,
        task_id: &str,
        acceptance_criteria: &AcceptanceCriteria,
    ) -> Result<ValidationResult> {
        // Look up completed task result
        let task_result = {
            let tasks = self.tasks.lock().await;
            tasks
                .iter()
                .find(|t| t.id == task_id)
                .and_then(|t| t.result.clone())
        };

        let output = match task_result {
            Some(result) => result,
            None => {
                // Task has no result -- validation fails
                let result = ValidationResult {
                    passed: false,
                    failures: vec!["specialist task has no result output".to_string()],
                    needs_llm_validation: false,
                };

                // Record failure in audit
                if let Err(e) = self
                    .update_handoff_outcome(
                        handoff_log_id,
                        "rejected",
                        None,
                        Some("no result output"),
                    )
                    .await
                {
                    tracing::warn!("handoff broker: failed to update outcome: {e}");
                }

                // WORM audit for rejection
                if let Err(e) = self
                    .record_handoff_audit(
                        "validation",
                        "validator",
                        task_id,
                        "output validation",
                        "rejected",
                        None,
                        None,
                        handoff_log_id,
                        "deterministic",
                        "[]",
                        0.0,
                        false,
                    )
                    .await
                {
                    tracing::warn!("handoff broker: failed to record validation audit: {e}");
                }

                return Ok(result);
            }
        };

        // Run structural validation
        let result = acceptance_criteria.validate_structural(&output);

        // Determine outcome
        let outcome = if result.passed {
            "accepted"
        } else {
            "rejected"
        };
        let error_msg = if result.passed {
            None
        } else {
            Some(result.failures.join("; "))
        };

        // Update handoff outcome
        if let Err(e) = self
            .update_handoff_outcome(handoff_log_id, outcome, None, error_msg.as_deref())
            .await
        {
            tracing::warn!("handoff broker: failed to update validation outcome: {e}");
        }

        // WORM audit for validation
        if let Err(e) = self
            .record_handoff_audit(
                "validation",
                "validator",
                task_id,
                "output validation",
                outcome,
                None,
                None,
                handoff_log_id,
                "deterministic",
                "[]",
                0.0,
                false,
            )
            .await
        {
            tracing::warn!("handoff broker: failed to record validation audit: {e}");
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::types::{AgentConfig, AgentMessage, AgentThread, TaskPriority, TaskStatus};
    use crate::history::now_ts;
    use crate::session_manager::SessionManager;
    use tempfile::tempdir;

    #[tokio::test]
    async fn probabilistic_routing_falls_back_to_deterministic_when_learned_score_is_below_threshold(
    ) {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let mut config = AgentConfig::default();
        config.routing.enabled = true;
        config.routing.method = crate::agent::types::RoutingMode::Probabilistic;
        config.routing.confidence_threshold = 0.3;
        let engine = AgentEngine::new_test(manager, config, root.path()).await;

        let now_ms = now_ts() * 1000;
        engine
            .record_capability_outcome(
                "researcher",
                &["research".to_string()],
                "failure",
                0.1,
                0,
                0.3,
            )
            .await
            .expect("record researcher outcome");
        engine
            .record_capability_outcome(
                "generalist",
                &["research".to_string()],
                "failure",
                0.1,
                0,
                0.3,
            )
            .await
            .expect("record generalist outcome");

        let result = engine
            .route_handoff(
                "Investigate a research question",
                &["research".to_string()],
                None,
                None,
                "thread-routing-fallback",
                "non_empty",
                0,
            )
            .await
            .expect("handoff should succeed");

        assert_eq!(result.routing_method, RoutingMethod::Deterministic);
        assert!(result.fallback_used);
        assert!(result.routing_score >= 0.3);
        assert!(result.routing_rationale.contains("fallback selected"));
        assert!(result
            .specialization_diagnostics
            .get("routing_confidence")
            .and_then(|v| v.get("threshold"))
            .and_then(|v| v.as_f64())
            .is_some_and(|threshold| (threshold - 0.3).abs() < f64::EPSILON));
        assert!(result
            .specialization_diagnostics
            .get("routing_confidence")
            .and_then(|v| v.get("cleared_threshold"))
            .and_then(|v| v.as_bool())
            .is_some_and(|cleared| !cleared));
        assert!(
            result.specialist_profile_id == "researcher"
                || result.specialist_profile_id == "generalist"
        );
        let rows = engine
            .load_capability_score_rows(&["research".to_string()])
            .await
            .expect("capability rows should load");
        assert!(rows
            .iter()
            .any(|row| row.last_attempt_ms.is_some_and(|ts| ts <= now_ms)));
    }

    #[tokio::test]
    async fn routing_skips_occupied_specialist_profiles() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let mut config = AgentConfig::default();
        config.routing.enabled = true;
        config.routing.method = crate::agent::types::RoutingMode::Deterministic;
        let engine = AgentEngine::new_test(manager, config, root.path()).await;

        let mut occupied = engine
            .enqueue_task(
                "[researcher] existing task".to_string(),
                "occupied specialist".to_string(),
                "normal",
                None,
                None,
                Vec::new(),
                None,
                "handoff",
                None,
                None,
                Some("thread-occupied-specialist".to_string()),
                None,
            )
            .await;
        occupied.status = TaskStatus::InProgress;
        occupied.title = "[researcher] already running".to_string();
        occupied.description = "existing occupied researcher task".to_string();
        engine.tasks.lock().await.push_back(occupied);

        let result = engine
            .route_handoff(
                "Investigate a research question",
                &["research".to_string()],
                None,
                None,
                "thread-routing-availability",
                "non_empty",
                0,
            )
            .await
            .expect("handoff should succeed");

        assert_ne!(
            result.specialist_profile_id, "researcher",
            "occupied highest-affinity specialist should be skipped"
        );
        assert!(
            result.routing_rationale.contains("availability")
                || result.specialist_profile_id == "generalist",
            "routing result should reflect availability-aware fallback: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn routing_prefers_specialist_with_stronger_morphogenesis_affinity() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let mut config = AgentConfig::default();
        config.routing.enabled = true;
        config.routing.method = crate::agent::types::RoutingMode::Probabilistic;
        config.routing.confidence_threshold = 0.3;
        let engine = AgentEngine::new_test(manager, config, root.path()).await;

        for _ in 0..3 {
            engine
                .record_morphogenesis_outcome(
                    "researcher",
                    &["research".to_string()],
                    crate::agent::morphogenesis::types::MorphogenesisOutcome::Success,
                )
                .await
                .expect("record morphogenesis outcome");
        }

        let result = engine
            .route_handoff(
                "Investigate a research question",
                &["research".to_string()],
                None,
                None,
                "thread-routing-morphogenesis",
                "non_empty",
                0,
            )
            .await
            .expect("handoff should succeed");

        assert_eq!(result.specialist_profile_id, "researcher");
        assert_eq!(result.routing_method, RoutingMethod::Probabilistic);
        assert!(!result.fallback_used);
        assert!(result
            .specialization_diagnostics
            .get("morphogenesis_influenced")
            .and_then(|value| value.as_bool())
            .is_some_and(|value| value));
        assert!(result
            .specialization_diagnostics
            .get("routing_snapshot_top_candidate")
            .and_then(|value| value.get("morphogenesis_affinity"))
            .and_then(|value| value.as_f64())
            .is_some_and(|value| value > 0.0));
    }

    #[tokio::test]
    async fn related_task_reuses_resonance_cached_shared_context_within_thread() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

        engine.threads.write().await.insert(
            "thread-resonance-related".to_string(),
            AgentThread {
                id: "thread-resonance-related".to_string(),
                agent_name: Some("Svarog".to_string()),
                title: "resonance related thread".to_string(),
                messages: vec![AgentMessage::user("inspect handoff cache", 1)],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );

        let parent = engine
            .enqueue_task(
                "Parent".to_string(),
                "Coordinate child work".to_string(),
                "normal",
                None,
                None,
                Vec::new(),
                None,
                "user",
                None,
                None,
                Some("thread-resonance-related".to_string()),
                Some("daemon".to_string()),
            )
            .await;

        let mut cached_child = parent.clone();
        cached_child.id = "task-cached-child".to_string();
        cached_child.source = "subagent".to_string();
        cached_child.parent_task_id = Some(parent.id.clone());
        cached_child.priority = TaskPriority::Normal;
        cached_child.status = TaskStatus::Queued;

        let mut listening_child = cached_child.clone();
        listening_child.id = "task-listening-child".to_string();

        engine.tasks.lock().await.push_back(cached_child.clone());
        engine.tasks.lock().await.push_back(listening_child.clone());

        engine.collaboration.write().await.insert(
            parent.id.clone(),
            crate::agent::collaboration::CollaborationSession {
                id: "collab-related-1".to_string(),
                parent_task_id: parent.id.clone(),
                thread_id: Some("thread-resonance-related".to_string()),
                goal_run_id: None,
                mission: "shared mission".to_string(),
                agents: Vec::new(),
                contributions: vec![crate::agent::collaboration::Contribution {
                    id: "contrib-related-1".to_string(),
                    task_id: "peer-task".to_string(),
                    topic: "debugging".to_string(),
                    position: "reuse cached context across siblings".to_string(),
                    confidence: 0.9,
                    evidence: vec!["peer evidence".to_string()],
                    created_at: 1,
                }],
                disagreements: Vec::new(),
                consensus: None,
                bids: Vec::new(),
                role_assignment: None,
                call_metadata: None,
                updated_at: 1,
            },
        );

        let snapshot = engine
            .build_resonance_context_snapshot(
                "inspect   handoff   cache",
                Some("thread-resonance-related"),
                Some(cached_child.id.as_str()),
                3,
            )
            .await;
        engine
            .put_resonance_context_snapshot(
                "inspect   handoff   cache",
                Some("thread-resonance-related"),
                Some(cached_child.id.as_str()),
                snapshot,
            )
            .await;

        let bundle = engine
            .assemble_context_bundle(
                "inspect handoff cache",
                Some(listening_child.id.as_str()),
                None,
                "thread-resonance-related",
                "non_empty",
                1,
            )
            .await
            .expect("bundle should assemble");

        assert!(bundle
            .partial_outputs
            .iter()
            .any(|output| output.status == "resonance_shared_context"));

        let cached = engine
            .get_resonance_context_snapshot(
                "inspect handoff cache",
                Some("thread-resonance-related"),
                Some(cached_child.id.as_str()),
            )
            .await
            .expect("cached resonance snapshot should exist");
        assert!(cached.hit_count >= 1);
    }

    #[tokio::test]
    async fn assemble_context_bundle_reuses_resonance_cached_shared_context() {
        let root = tempdir().expect("tempdir");
        let manager = SessionManager::new_test(root.path()).await;
        let engine = AgentEngine::new_test(manager, AgentConfig::default(), root.path()).await;

        engine.threads.write().await.insert(
            "thread-resonance".to_string(),
            AgentThread {
                id: "thread-resonance".to_string(),
                agent_name: Some("Svarog".to_string()),
                title: "resonance thread".to_string(),
                messages: vec![AgentMessage::user("inspect handoff cache", 1)],
                pinned: false,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
                created_at: 1,
                updated_at: 1,
            },
        );

        let parent = engine
            .enqueue_task(
                "Parent".to_string(),
                "Coordinate child work".to_string(),
                "normal",
                None,
                None,
                Vec::new(),
                None,
                "user",
                None,
                None,
                Some("thread-resonance".to_string()),
                Some("daemon".to_string()),
            )
            .await;

        let mut child = parent.clone();
        child.id = "task-child".to_string();
        child.source = "subagent".to_string();
        child.parent_task_id = Some(parent.id.clone());
        child.priority = TaskPriority::Normal;
        child.status = TaskStatus::Queued;
        engine.tasks.lock().await.push_back(child.clone());

        engine.collaboration.write().await.insert(
            parent.id.clone(),
            crate::agent::collaboration::CollaborationSession {
                id: "collab-1".to_string(),
                parent_task_id: parent.id.clone(),
                thread_id: Some("thread-resonance".to_string()),
                goal_run_id: None,
                mission: "shared mission".to_string(),
                agents: Vec::new(),
                contributions: vec![crate::agent::collaboration::Contribution {
                    id: "contrib-1".to_string(),
                    task_id: "peer-task".to_string(),
                    topic: "debugging".to_string(),
                    position: "reuse cached context".to_string(),
                    confidence: 0.8,
                    evidence: vec!["peer evidence".to_string()],
                    created_at: 1,
                }],
                disagreements: Vec::new(),
                consensus: None,
                bids: Vec::new(),
                role_assignment: None,
                call_metadata: None,
                updated_at: 1,
            },
        );

        let snapshot = engine
            .build_resonance_context_snapshot(
                "inspect handoff cache",
                Some("thread-resonance"),
                Some(child.id.as_str()),
                3,
            )
            .await;
        engine
            .put_resonance_context_snapshot(
                "inspect handoff cache",
                Some("thread-resonance"),
                Some(child.id.as_str()),
                snapshot,
            )
            .await;

        let bundle = engine
            .assemble_context_bundle(
                "inspect handoff cache",
                Some(child.id.as_str()),
                None,
                "thread-resonance",
                "non_empty",
                1,
            )
            .await
            .expect("bundle should assemble");

        assert!(bundle
            .partial_outputs
            .iter()
            .any(|output| output.status == "resonance_shared_context"));
        let cached = engine
            .get_resonance_context_snapshot(
                "inspect handoff cache",
                Some("thread-resonance"),
                Some(child.id.as_str()),
            )
            .await
            .expect("cached resonance snapshot should exist");
        assert!(cached.hit_count >= 1);
    }
}
