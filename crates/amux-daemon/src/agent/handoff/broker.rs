//! Handoff broker orchestration — routes tasks to specialist subagents,
//! assembles context bundles, and validates specialist output.
//!
//! This module wires together all Plan 01 types and Plan 02 logic into
//! the live agent pipeline, making handoffs work end-to-end.

use anyhow::{Context, Result};
use uuid::Uuid;

use super::profiles::match_specialist;
use super::{AcceptanceCriteria, ContextBundle, EpisodeRef, HandoffResult, ValidationResult};
use crate::agent::engine::AgentEngine;

/// Maximum handoff depth before escalating to operator (HAND-08).
const MAX_HANDOFF_DEPTH: u8 = 3;

/// Token ceiling for context bundles (HAND-03 locked decision).
const CONTEXT_BUNDLE_TOKEN_CEILING: u32 = 2000;

/// Maximum episodic refs to include in a context bundle.
const MAX_EPISODIC_REFS: usize = 3;

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

        // 1. Retrieve relevant episodes (max 3 for bundles)
        match self
            .retrieve_relevant_episodes(task_description, MAX_EPISODIC_REFS)
            .await
        {
            Ok(episodes) => {
                bundle.episodic_refs = episodes
                    .into_iter()
                    .map(|ep| EpisodeRef {
                        episode_id: ep.id,
                        summary: ep.summary,
                        outcome: format!("{:?}", ep.outcome),
                    })
                    .collect();
            }
            Err(e) => {
                tracing::warn!("handoff broker: failed to retrieve episodes: {e}");
            }
        }

        // 2. Query active negative constraints (global)
        match self.query_active_constraints(None).await {
            Ok(constraints) => {
                bundle.negative_constraints =
                    constraints.into_iter().map(|c| c.description).collect();
            }
            Err(e) => {
                tracing::warn!("handoff broker: failed to query constraints: {e}");
            }
        }

        // 3. Pull partial outputs from parent task if available
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

        // 4. Parent context summary: last few messages from the thread
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

        // 5. Enforce token ceiling per HAND-03
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
        let profiles = &broker.profiles;
        let threshold = broker.match_threshold;

        // Match specialist
        let (profile_idx, _score) = match match_specialist(profiles, capability_tags, threshold) {
            Some(result) => result,
            None => {
                // Fallback to generalist (last profile)
                let generalist_idx = profiles.len().saturating_sub(1);
                (generalist_idx, 0.0)
            }
        };

        let specialist = &profiles[profile_idx];
        let specialist_id = specialist.id.clone();
        let specialist_name = specialist.name.clone();
        let specialist_role = specialist.role.clone();
        let system_prompt_snippet = specialist.system_prompt_snippet.clone();
        drop(broker);

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
                current_depth,
                "dispatched",
                None,
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
            )
            .await
        {
            tracing::warn!("handoff broker: failed to record validation audit: {e}");
        }

        Ok(result)
    }
}
