//! Skill discovery — pure functions for evaluating execution traces as skill candidates.
//!
//! This module provides the core logic for deciding whether a completed execution
//! trace qualifies as a skill-drafting candidate based on complexity, quality, and
//! novelty relative to known patterns.

use std::collections::HashSet;

use super::learning::patterns::ToolPattern;
use super::types::SkillDiscoveryConfig;

// ---------------------------------------------------------------------------
// Complexity threshold
// ---------------------------------------------------------------------------

/// Determine whether an execution trace meets the complexity threshold for
/// skill-drafting candidacy.
///
/// Returns `true` when:
/// - `outcome` is `"success"`, AND
/// - `tool_count` exceeds `config.min_tool_count`, AND
/// - at least one of: `replan_count >= config.min_replan_count` OR
///   `quality_score > config.min_quality_score`
pub(super) fn meets_complexity_threshold(
    tool_count: usize,
    replan_count: u32,
    quality_score: Option<f64>,
    outcome: &str,
    config: &SkillDiscoveryConfig,
) -> bool {
    if outcome != "success" {
        return false;
    }
    let tool_gate = tool_count > config.min_tool_count;
    let replan_gate = replan_count >= config.min_replan_count;
    let quality_gate = quality_score.map_or(false, |q| q > config.min_quality_score);
    tool_gate && (replan_gate || quality_gate)
}

// ---------------------------------------------------------------------------
// Jaccard similarity
// ---------------------------------------------------------------------------

/// Compute the Jaccard similarity coefficient between two string slices.
///
/// Returns 1.0 when both slices are empty (two empty sets are identical),
/// 0.0 when the intersection is empty, or |A intersect B| / |A union B|.
pub(super) fn jaccard_similarity(a: &[String], b: &[String]) -> f64 {
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    let set_a: HashSet<&str> = a.iter().map(|s| s.as_str()).collect();
    let set_b: HashSet<&str> = b.iter().map(|s| s.as_str()).collect();
    let intersection = set_a.intersection(&set_b).count();
    let union = set_a.union(&set_b).count();
    if union == 0 {
        return 1.0;
    }
    intersection as f64 / union as f64
}

// ---------------------------------------------------------------------------
// Novelty detection
// ---------------------------------------------------------------------------

/// Determine whether a tool sequence is novel relative to known patterns.
///
/// Takes pre-fetched patterns (not the PatternStore directly) for testability.
/// Returns `true` when no existing pattern has a Jaccard similarity >=
/// `similarity_threshold` with the candidate sequence.
pub(super) fn is_novel_sequence(
    tool_sequence: &[String],
    _task_type: &str,
    patterns: &[&ToolPattern],
    similarity_threshold: f64,
) -> bool {
    for pattern in patterns {
        let sim = jaccard_similarity(tool_sequence, &pattern.tool_sequence);
        if sim >= similarity_threshold {
            return false;
        }
    }
    true
}

// ---------------------------------------------------------------------------
// JSON extraction
// ---------------------------------------------------------------------------

/// Parse a JSON array of strings into a `Vec<String>`.
///
/// Returns an empty vec on `None` or parse failure.
pub(super) fn extract_tool_sequence_from_json(json: Option<&str>) -> Vec<String> {
    json.and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
        .unwrap_or_default()
}

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

            let tool_sequence = extract_tool_sequence_from_json(trace.tool_sequence_json.as_deref());
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
                .set_consolidation_state(
                    "skill_draft_watermark",
                    &last_created_at.to_string(),
                    now,
                )
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

        let pending: Vec<_> = candidates
            .iter()
            .filter(|(_, v)| v == "pending")
            .collect();

        if pending.is_empty() {
            return 0;
        }

        // Process at most ONE candidate per tick
        let (key, _) = &pending[0];
        let trace_id = key
            .strip_prefix("skill_draft_candidate:")
            .unwrap_or(key);

        // Extract tool sequence from the trace (look it up again)
        let traces = match self
            .history
            .list_recent_successful_traces(0, 500)
            .await
        {
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
        let drafts_dir = self.data_dir
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
        let skill_path = self.data_dir
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

    /// Sub-task 8: Check lifecycle promotions for Testing/Active/Proven skills.
    ///
    /// Uses success_count against configured thresholds (D-06).
    /// No LLM calls -- purely threshold-based, processes all eligible variants.
    pub(super) async fn check_skill_promotions(
        &self,
        config: &AgentConfig,
        _deadline: &std::time::Instant,
    ) -> usize {
        let thresholds = &config.skill_promotion;
        let mut promoted = 0usize;

        for (status, next_status, threshold) in [
            ("testing", "active", thresholds.testing_to_active),
            ("active", "proven", thresholds.active_to_proven),
            ("proven", "promoted_to_canonical", thresholds.proven_to_canonical),
        ] {
            let variants = match self
                .history
                .list_skill_variants_by_status(status, 100)
                .await
            {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!(error = %e, status, "failed to list skill variants for promotion check");
                    continue;
                }
            };

            for variant in &variants {
                if variant.success_count >= threshold {
                    if let Err(e) = self
                        .history
                        .update_skill_variant_status(&variant.variant_id, next_status)
                        .await
                    {
                        tracing::warn!(
                            error = %e,
                            variant_id = %variant.variant_id,
                            "failed to promote skill variant"
                        );
                        continue;
                    }

                    // Record provenance for lifecycle promotion (D-07)
                    self.record_provenance_event(
                        "skill_lifecycle_promotion",
                        &format!(
                            "Skill '{}' promoted {} -> {} (success_count {} >= threshold {})",
                            variant.skill_name, status, next_status,
                            variant.success_count, threshold
                        ),
                        serde_json::json!({
                            "variant_id": variant.variant_id,
                            "skill_name": variant.skill_name,
                            "from_status": status,
                            "to_status": next_status,
                            "success_count": variant.success_count,
                            "threshold": threshold,
                        }),
                        None,
                        None,
                        None,
                        None,
                        None,
                    )
                    .await;

                    tracing::info!(
                        skill_name = %variant.skill_name,
                        variant_id = %variant.variant_id,
                        from = status,
                        to = next_status,
                        success_count = variant.success_count,
                        "skill promoted through lifecycle"
                    );

                    // Announce promotion via HeartbeatDigest (and WorkflowNotice for canonical)
                    self.announce_skill_promotion(
                        &variant.skill_name,
                        status,
                        next_status,
                        variant.success_count,
                    );

                    promoted += 1;
                }
            }
        }

        promoted
    }
}

/// Parse the LLM mental test response to count passing scenarios.
fn parse_mental_test_results(response: &str) -> usize {
    // Try to parse as JSON array
    #[derive(serde::Deserialize)]
    struct Scenario {
        #[serde(default)]
        would_help: bool,
    }

    // Try direct parse first
    if let Ok(scenarios) = serde_json::from_str::<Vec<Scenario>>(response) {
        return scenarios.iter().filter(|s| s.would_help).count();
    }

    // Try to extract JSON array from markdown code blocks
    let trimmed = response
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    if let Ok(scenarios) = serde_json::from_str::<Vec<Scenario>>(trimmed) {
        return scenarios.iter().filter(|s| s.would_help).count();
    }

    // Fallback: count occurrences of "would_help": true
    response.matches("\"would_help\": true").count()
        + response.matches("\"would_help\":true").count()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::learning::patterns::{PatternType, ToolPattern};

    fn default_config() -> SkillDiscoveryConfig {
        SkillDiscoveryConfig::default()
    }

    fn make_pattern(tools: &[&str]) -> ToolPattern {
        ToolPattern {
            id: "test-pattern".to_string(),
            pattern_type: PatternType::SuccessSequence,
            tool_sequence: tools.iter().map(|s| s.to_string()).collect(),
            task_type: "coding".to_string(),
            occurrences: 5,
            success_rate: 0.9,
            confidence: 0.8,
            last_seen_at: 1000,
            created_at: 500,
        }
    }

    fn seq(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    // -----------------------------------------------------------------------
    // meets_complexity_threshold
    // -----------------------------------------------------------------------

    #[test]
    fn skill_discovery_complexity_returns_false_when_outcome_not_success() {
        let cfg = default_config();
        assert!(!meets_complexity_threshold(20, 2, Some(0.95), "failure", &cfg));
    }

    #[test]
    fn skill_discovery_complexity_returns_false_when_tool_count_at_threshold() {
        let cfg = default_config();
        // tool_count == min_tool_count (8), not >, so false
        assert!(!meets_complexity_threshold(8, 2, Some(0.95), "success", &cfg));
    }

    #[test]
    fn skill_discovery_complexity_returns_true_with_replan() {
        let cfg = default_config();
        // tool_count > 8, replan_count >= 1, outcome success
        assert!(meets_complexity_threshold(10, 1, None, "success", &cfg));
    }

    #[test]
    fn skill_discovery_complexity_returns_true_with_quality() {
        let cfg = default_config();
        // tool_count > 8, replan_count=0, quality > 0.8, outcome success
        assert!(meets_complexity_threshold(10, 0, Some(0.85), "success", &cfg));
    }

    #[test]
    fn skill_discovery_complexity_returns_false_no_replan_no_quality() {
        let cfg = default_config();
        // tool_count > 8, replan_count=0, quality <= 0.8
        assert!(!meets_complexity_threshold(10, 0, Some(0.8), "success", &cfg));
        assert!(!meets_complexity_threshold(10, 0, None, "success", &cfg));
    }

    // -----------------------------------------------------------------------
    // jaccard_similarity
    // -----------------------------------------------------------------------

    #[test]
    fn skill_discovery_jaccard_identical_sets() {
        let a = seq(&["A", "B", "C"]);
        let b = seq(&["A", "B", "C"]);
        assert!((jaccard_similarity(&a, &b) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn skill_discovery_jaccard_disjoint_sets() {
        let a = seq(&["A", "B"]);
        let b = seq(&["C", "D"]);
        assert!((jaccard_similarity(&a, &b) - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn skill_discovery_jaccard_partial_overlap() {
        let a = seq(&["A", "B", "C"]);
        let b = seq(&["B", "C", "D"]);
        // intersection={B,C}=2, union={A,B,C,D}=4 => 0.5
        assert!((jaccard_similarity(&a, &b) - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn skill_discovery_jaccard_empty_sets() {
        let a: Vec<String> = vec![];
        let b: Vec<String> = vec![];
        assert!((jaccard_similarity(&a, &b) - 1.0).abs() < f64::EPSILON);
    }

    // -----------------------------------------------------------------------
    // is_novel_sequence
    // -----------------------------------------------------------------------

    #[test]
    fn skill_discovery_novel_when_no_patterns_match() {
        let candidate = seq(&["X", "Y", "Z"]);
        let pattern = make_pattern(&["A", "B", "C"]);
        let patterns = vec![&pattern];
        assert!(is_novel_sequence(&candidate, "coding", &patterns, 0.7));
    }

    #[test]
    fn skill_discovery_not_novel_when_pattern_similar() {
        let candidate = seq(&["A", "B", "C"]);
        let pattern = make_pattern(&["A", "B", "C"]);
        let patterns = vec![&pattern];
        // similarity=1.0 >= 0.7 threshold
        assert!(!is_novel_sequence(&candidate, "coding", &patterns, 0.7));
    }

    // -----------------------------------------------------------------------
    // extract_tool_sequence_from_json
    // -----------------------------------------------------------------------

    #[test]
    fn skill_discovery_extract_tool_sequence_valid_json() {
        let json = r#"["file_read", "terminal_exec", "file_write"]"#;
        let result = extract_tool_sequence_from_json(Some(json));
        assert_eq!(result, vec!["file_read", "terminal_exec", "file_write"]);
    }

    #[test]
    fn skill_discovery_extract_tool_sequence_none() {
        let result = extract_tool_sequence_from_json(None);
        assert!(result.is_empty());
    }

    #[test]
    fn skill_discovery_extract_tool_sequence_invalid_json() {
        let result = extract_tool_sequence_from_json(Some("not json"));
        assert!(result.is_empty());
    }

    // -----------------------------------------------------------------------
    // parse_mental_test_results
    // -----------------------------------------------------------------------

    #[test]
    fn skill_discovery_mental_test_parses_valid_json() {
        let response = r#"[
            {"scenario": "Debugging a CI failure", "would_help": true},
            {"scenario": "Writing a README", "would_help": false},
            {"scenario": "Refactoring a module", "would_help": true}
        ]"#;
        assert_eq!(parse_mental_test_results(response), 2);
    }

    #[test]
    fn skill_discovery_mental_test_parses_json_in_code_block() {
        let response = "```json\n[\n  {\"scenario\": \"A\", \"would_help\": true},\n  {\"scenario\": \"B\", \"would_help\": true},\n  {\"scenario\": \"C\", \"would_help\": true}\n]\n```";
        assert_eq!(parse_mental_test_results(response), 3);
    }

    #[test]
    fn skill_discovery_mental_test_returns_zero_for_all_false() {
        let response = r#"[
            {"scenario": "A", "would_help": false},
            {"scenario": "B", "would_help": false},
            {"scenario": "C", "would_help": false}
        ]"#;
        assert_eq!(parse_mental_test_results(response), 0);
    }

    #[test]
    fn skill_discovery_mental_test_returns_zero_for_invalid_response() {
        assert_eq!(parse_mental_test_results("I cannot evaluate this skill."), 0);
    }

    #[test]
    fn skill_discovery_mental_test_fallback_counts_would_help() {
        let response = r#"Some text "would_help": true and "would_help":true but "would_help": false"#;
        assert_eq!(parse_mental_test_results(response), 2);
    }
}
