//! Skill discovery — pure functions for evaluating execution traces as skill candidates.
//!
//! This module provides the core logic for deciding whether a completed execution
//! trace qualifies as a skill-drafting candidate based on complexity, quality, and
//! novelty relative to known patterns.

use std::collections::HashSet;

use super::learning::patterns::ToolPattern;
use super::types::SkillDiscoveryConfig;

mod helpers;
pub(crate) use helpers::jaccard_similarity;
use helpers::{
    extract_tool_sequence_from_json, is_novel_sequence, meets_complexity_threshold,
    parse_mental_test_results,
};

// ---------------------------------------------------------------------------
// Complexity threshold
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Jaccard similarity
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Novelty detection
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// JSON extraction
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Consolidation sub-task methods on AgentEngine (Phase 6 Plan 02)
// ---------------------------------------------------------------------------

use super::engine::AgentEngine;
use super::types::AgentConfig;

impl AgentEngine {
    /// Sub-task 5: Flag execution traces that qualify as skill draft candidates.
    ///
    /// Uses a separate watermark (`skill_draft_watermark`) from trace review
    /// (Pitfall 7) to avoid coupling with heuristic promotion scanning.
    pub(super) async fn flag_skill_draft_candidates(
        &self,
        config: &AgentConfig,
        deadline: &std::time::Instant,
    ) -> usize {
        let watermark: u64 = match self
            .history
            .get_consolidation_state("skill_draft_watermark")
            .await
        {
            Ok(Some(val)) => val.parse().unwrap_or(0),
            _ => 0,
        };

        let traces = match self
            .history
            .list_recent_successful_traces(watermark, 50)
            .await
        {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(error = %e, "failed to list traces for skill draft flagging");
                return 0;
            }
        };

        if traces.is_empty() {
            return 0;
        }

        let now = super::now_millis();
        let mut flagged = 0usize;
        let mut last_created_at: u64 = watermark;
        let max_per_tick = 20;

        for trace in traces.iter().take(max_per_tick) {
            if std::time::Instant::now() >= *deadline {
                break;
            }

            last_created_at = last_created_at.max(trace.created_at as u64);

            let tool_sequence =
                extract_tool_sequence_from_json(trace.tool_sequence_json.as_deref());
            if tool_sequence.is_empty() {
                continue;
            }

            let outcome = trace.outcome.as_deref().unwrap_or("unknown");

            // Get replan_count from goal_run if present (Pitfall 1)
            let replan_count: u32 = if let Some(ref gr_id) = trace.goal_run_id {
                match self.history.get_goal_run(gr_id).await {
                    Ok(Some(gr)) => gr.replan_count,
                    _ => 0,
                }
            } else {
                0
            };

            let quality_score = trace.quality_score;

            if !meets_complexity_threshold(
                tool_sequence.len(),
                replan_count,
                quality_score,
                outcome,
                &config.skill_discovery,
            ) {
                continue;
            }

            // Check novelty against known patterns
            let task_type = trace.task_type.as_deref().unwrap_or("unknown");
            let is_novel = {
                let patterns = self.pattern_store.read().await;
                let matching = patterns.find_patterns(
                    task_type,
                    super::learning::patterns::PatternType::SuccessSequence,
                );
                is_novel_sequence(
                    &tool_sequence,
                    task_type,
                    &matching,
                    config.skill_discovery.novelty_similarity_threshold,
                )
            };

            if !is_novel {
                continue;
            }

            // Store as pending candidate
            let key = format!("skill_draft_candidate:{}", trace.id);
            if let Err(e) = self
                .history
                .set_consolidation_state(&key, "pending", now)
                .await
            {
                tracing::warn!(error = %e, trace_id = %trace.id, "failed to flag skill draft candidate");
                continue;
            }

            flagged += 1;
        }

        // Update watermark
        if last_created_at > watermark {
            if let Err(e) = self
                .history
                .set_consolidation_state("skill_draft_watermark", &last_created_at.to_string(), now)
                .await
            {
                tracing::warn!(error = %e, "failed to update skill draft watermark");
            }
        }

        if flagged > 0 {
            tracing::debug!(flagged, "flagged skill draft candidates");
        }
        flagged
    }

    /// Sub-task 6: Draft flagged skill candidates into SKILL.md files via hybrid LLM generation.
    ///
    /// Processes at most ONE candidate per tick (LLM budget, Pitfall 4).
    /// Creates `~/.tamux/skills/drafts/{skill_name}/SKILL.md` and registers in DB.
    pub(super) async fn draft_flagged_skill_candidates(
        &self,
        config: &AgentConfig,
        deadline: &std::time::Instant,
    ) -> usize {
        // Need at least 10 seconds for LLM call (Pitfall 4)
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.as_secs() < 10 {
            tracing::debug!("skipping skill drafting -- insufficient budget");
            return 0;
        }

        // Check circuit breaker
        if let Err(_e) = self.check_circuit_breaker(&config.provider).await {
            tracing::debug!(
                provider = %config.provider,
                "skipping skill drafting -- circuit breaker open"
            );
            return 0;
        }

        // List pending candidates
        let candidates = match self
            .history
            .list_consolidation_state_by_prefix("skill_draft_candidate:")
            .await
        {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(error = %e, "failed to list skill draft candidates");
                return 0;
            }
        };

        let pending: Vec<_> = candidates.iter().filter(|(_, v)| v == "pending").collect();

        if pending.is_empty() {
            return 0;
        }

        // Process at most ONE candidate per tick
        let (key, _) = &pending[0];
        let trace_id = key.strip_prefix("skill_draft_candidate:").unwrap_or(key);

        // Extract tool sequence from the trace (look it up again)
        let traces = match self.history.list_recent_successful_traces(0, 500).await {
            Ok(t) => t,
            Err(_) => return 0,
        };
        let trace = match traces.iter().find(|t| t.id == trace_id) {
            Some(t) => t,
            None => {
                // Trace no longer available -- mark as skipped
                let _ = self
                    .history
                    .set_consolidation_state(key, "skipped", super::now_millis())
                    .await;
                return 0;
            }
        };

        let tool_sequence = extract_tool_sequence_from_json(trace.tool_sequence_json.as_deref());
        let task_type = trace.task_type.as_deref().unwrap_or("unknown");

        // Build template part (tool sequence) + LLM prompt for description/guidance
        let tool_list = tool_sequence.join(", ");
        let drafting_prompt = format!(
            "You are a skill documentation agent for an AI assistant.\n\n\
             A successful execution trace used these tools in order: [{tool_list}]\n\
             Task type: {task_type}\n\n\
             Generate a SKILL.md document for this discovered skill pattern.\n\
             Use this exact format:\n\n\
             ---\n\
             name: <concise snake_case skill name>\n\
             description: <one-line description>\n\
             context_tags: [<relevant tags>]\n\
             ---\n\n\
             # <Skill Name>\n\n\
             ## When to Use\n\
             <2-3 sentences describing when this skill is useful>\n\n\
             ## Tool Sequence\n\
             {tool_list}\n\n\
             ## Guidance\n\
             <3-5 bullet points of tactical guidance for executing this skill effectively>\n\n\
             Respond ONLY with the SKILL.md content, nothing else.",
        );

        let llm_timeout = remaining.saturating_sub(std::time::Duration::from_secs(2));

        let skill_content: String = match tokio::time::timeout(
            llm_timeout,
            self.send_refinement_llm_call(config, &drafting_prompt),
        )
        .await
        {
            Ok(Ok(response)) => response.trim().to_string(),
            Ok(Err(e)) => {
                tracing::warn!(error = %e, "skill drafting LLM call failed");
                return 0;
            }
            Err(_) => {
                tracing::debug!("skill drafting LLM call timed out");
                return 0;
            }
        };

        if skill_content.is_empty() {
            return 0;
        }

        // Extract skill name from generated content
        let skill_name: String = skill_content
            .lines()
            .find(|l: &&str| l.starts_with("name:"))
            .and_then(|l: &str| l.strip_prefix("name:"))
            .map(|s: &str| s.trim().to_string())
            .unwrap_or_else(|| format!("skill_{}", &trace_id[..8.min(trace_id.len())]));

        // Create drafts directory (Pitfall 3)
        let drafts_dir = self
            .data_dir
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .join("skills")
            .join("drafts")
            .join(&skill_name);

        if let Err(e) = std::fs::create_dir_all(&drafts_dir) {
            tracing::warn!(error = %e, path = %drafts_dir.display(), "failed to create drafts directory");
            return 0;
        }

        let skill_path = drafts_dir.join("SKILL.md");
        if let Err(e) = std::fs::write(&skill_path, &skill_content) {
            tracing::warn!(error = %e, path = %skill_path.display(), "failed to write SKILL.md");
            return 0;
        }

        // Register in DB
        let variant = match self.history.register_skill_document(&skill_path).await {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = %e, "failed to register drafted skill document");
                return 0;
            }
        };

        // Override status to "draft" (Pitfall 6: register_skill_document defaults to "active")
        if let Err(e) = self
            .history
            .update_skill_variant_status(&variant.variant_id, "draft")
            .await
        {
            tracing::warn!(error = %e, "failed to set drafted skill status to draft");
        }

        // Mark candidate as drafted
        let now = super::now_millis();
        let _ = self
            .history
            .set_consolidation_state(key, "drafted", now)
            .await;

        // Record provenance for skill drafting (D-07)
        self.record_provenance_event(
            "skill_drafted",
            &format!(
                "Drafted skill '{}' from trace {} (tools: {})",
                skill_name, trace_id, tool_list
            ),
            serde_json::json!({
                "variant_id": variant.variant_id,
                "skill_name": skill_name,
                "trace_id": trace_id,
                "tool_sequence": tool_sequence,
                "task_type": task_type,
            }),
            trace.goal_run_id.as_deref(),
            trace.task_id.as_deref(),
            None,
            None,
            None,
        )
        .await;

        tracing::info!(
            skill_name = %skill_name,
            variant_id = %variant.variant_id,
            "drafted new skill from execution trace"
        );

        // Announce via concierge (D-08: first drafts are milestones)
        let description_excerpt: String = skill_content.chars().take(200).collect();
        self.announce_skill_draft(&skill_name, &description_excerpt);

        1
    }

    /// Sub-task 7: Run mental tests on Draft-status skills via LLM evaluation.
    ///
    /// Presents the skill content to the LLM with 3 hypothetical scenarios.
    /// If at least 2/3 pass, promotes Draft -> Testing.
    /// Processes at most ONE draft per tick (budget-safe).
    pub(super) async fn run_skill_mental_tests(
        &self,
        config: &AgentConfig,
        deadline: &std::time::Instant,
    ) -> usize {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.as_secs() < 10 {
            tracing::debug!("skipping skill mental tests -- insufficient budget");
            return 0;
        }

        if let Err(_e) = self.check_circuit_breaker(&config.provider).await {
            tracing::debug!(
                provider = %config.provider,
                "skipping skill mental tests -- circuit breaker open"
            );
            return 0;
        }

        // Query draft skills
        let drafts = match self
            .history
            .list_skill_variants_by_status("draft", 10)
            .await
        {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!(error = %e, "failed to list draft skills for mental tests");
                return 0;
            }
        };

        if drafts.is_empty() {
            return 0;
        }

        // Process at most ONE draft per tick
        let draft = &drafts[0];

        // Read SKILL.md content from disk
        let skill_path = self
            .data_dir
            .parent()
            .unwrap_or(std::path::Path::new("."))
            .join("skills")
            .join(&draft.relative_path);

        let skill_content = match tokio::fs::read_to_string(&skill_path).await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    path = %skill_path.display(),
                    variant_id = %draft.variant_id,
                    "failed to read draft skill for mental testing"
                );
                return 0;
            }
        };

        // Build LLM prompt for mental test evaluation (D-05)
        let test_prompt = format!(
            "You are evaluating whether a draft AI skill document is useful and correct.\n\n\
             ## Skill Document\n\n\
             {skill_content}\n\n\
             ## Task\n\n\
             Evaluate this skill against 3 hypothetical scenarios where it might be used.\n\
             For each scenario, determine whether the skill's guidance would genuinely help.\n\n\
             Respond with ONLY a JSON array of exactly 3 objects:\n\
             [\n\
               {{\"scenario\": \"<description>\", \"would_help\": true/false}},\n\
               {{\"scenario\": \"<description>\", \"would_help\": true/false}},\n\
               {{\"scenario\": \"<description>\", \"would_help\": true/false}}\n\
             ]\n\n\
             Be critical. Only mark would_help=true if the skill provides genuinely useful, \
             non-obvious guidance for that scenario.",
        );

        let llm_timeout = remaining.saturating_sub(std::time::Duration::from_secs(2));

        let response: String = match tokio::time::timeout(
            llm_timeout,
            self.send_refinement_llm_call(config, &test_prompt),
        )
        .await
        {
            Ok(Ok(r)) => r.trim().to_string(),
            Ok(Err(e)) => {
                tracing::warn!(error = %e, "skill mental test LLM call failed");
                return 0;
            }
            Err(_) => {
                tracing::debug!("skill mental test LLM call timed out");
                return 0;
            }
        };

        // Parse response: count scenarios where would_help=true
        let pass_count = parse_mental_test_results(&response);

        if pass_count >= 2 {
            // Promote to Testing
            if let Err(e) = self
                .history
                .update_skill_variant_status(&draft.variant_id, "testing")
                .await
            {
                tracing::warn!(error = %e, "failed to promote draft to testing");
                return 0;
            }

            // Record provenance for mental test promotion (D-07)
            self.record_provenance_event(
                "skill_mental_test_passed",
                &format!(
                    "Skill '{}' passed mental tests ({}/3 scenarios) -- promoted Draft -> Testing",
                    draft.skill_name, pass_count
                ),
                serde_json::json!({
                    "variant_id": draft.variant_id,
                    "skill_name": draft.skill_name,
                    "pass_count": pass_count,
                    "total_scenarios": 3,
                    "transition": "draft -> testing",
                }),
                None,
                None,
                None,
                None,
                None,
            )
            .await;

            tracing::info!(
                skill_name = %draft.skill_name,
                variant_id = %draft.variant_id,
                pass_count,
                "skill passed mental tests -- promoted to Testing"
            );

            1
        } else {
            tracing::debug!(
                skill_name = %draft.skill_name,
                pass_count,
                "skill failed mental tests -- remains Draft"
            );
            0
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "tests/skill_discovery.rs"]
mod tests;
