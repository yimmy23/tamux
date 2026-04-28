use super::*;

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

pub(crate) fn jaccard_similarity(a: &[String], b: &[String]) -> f64 {
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

pub(super) fn extract_tool_sequence_from_json(json: Option<&str>) -> Vec<String> {
    json.and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
        .unwrap_or_default()
}

impl AgentEngine {
    pub(in crate::agent) async fn check_skill_promotions(
        &self,
        config: &AgentConfig,
        _deadline: &std::time::Instant,
    ) -> usize {
        let thresholds = &config.skill_promotion;
        let mut promoted = 0usize;

        for (status, next_status, threshold) in [
            ("testing", "active", thresholds.testing_to_active),
            ("active", "proven", thresholds.active_to_proven),
            (
                "proven",
                "promoted_to_canonical",
                thresholds.proven_to_canonical,
            ),
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

                    self.record_provenance_event(
                        "skill_lifecycle_promotion",
                        &format!(
                            "Skill '{}' promoted {} -> {} (success_count {} >= threshold {})",
                            variant.skill_name,
                            status,
                            next_status,
                            variant.success_count,
                            threshold
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

pub(super) fn parse_mental_test_results(response: &str) -> usize {
    #[derive(serde::Deserialize)]
    struct Scenario {
        #[serde(default)]
        would_help: bool,
    }

    if let Ok(scenarios) = serde_json::from_str::<Vec<Scenario>>(response) {
        return scenarios.iter().filter(|s| s.would_help).count();
    }

    let trimmed = response
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    if let Ok(scenarios) = serde_json::from_str::<Vec<Scenario>>(trimmed) {
        return scenarios.iter().filter(|s| s.would_help).count();
    }

    response.matches("\"would_help\": true").count()
        + response.matches("\"would_help\":true").count()
}
