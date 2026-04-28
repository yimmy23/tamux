use std::collections::BTreeMap;

use anyhow::Result;

use crate::agent::background_workers::protocol::{
    BackgroundWorkerCommand, BackgroundWorkerKind, BackgroundWorkerResult,
};
use crate::agent::background_workers::run_background_worker_command;
use crate::agent::engine::AgentEngine;
use crate::agent::skill_discovery::{infer_draft_context_tags, sanitize_agentskills_name};
use crate::history::{ExecutionTraceRow, SkillVariantRecord};

use super::arena::build_cross_breed_proposals;
use super::fitness_scorer::build_fitness_history;
use super::gene_extractor::build_candidate;
use super::types::{
    GenePoolArenaScore, GenePoolCandidate, GenePoolLifecycleAction, GenePoolRuntimeSnapshot,
};

const GENE_POOL_RUNTIME_SNAPSHOT_KEY: &str = "gene_pool_runtime_snapshot";
const GENE_POOL_TRACE_LIMIT: usize = 200;
const GENE_POOL_VARIANT_LIMIT: usize = 512;
const PROMOTION_SCORE_THRESHOLD: f64 = 0.78;
const RETIREMENT_SCORE_THRESHOLD: f64 = 0.32;
const MIN_PROMOTION_SUCCESSES: u32 = 3;
const MIN_RETIREMENT_USES: u32 = 4;

fn normalize_fitness_score(fitness_score: f64) -> f64 {
    ((fitness_score + 6.0) / 12.0).clamp(0.0, 1.0)
}

fn compute_arena_score(record: &SkillVariantRecord) -> f64 {
    let success_rate = record.success_rate();
    let fitness_component = normalize_fitness_score(record.fitness_score);
    let usage_component = (record.use_count.min(10) as f64) / 10.0;
    (0.55 * success_rate + 0.30 * fitness_component + 0.15 * usage_component).clamp(0.0, 1.0)
}

pub(crate) fn build_gene_pool_runtime_snapshot(
    successful_traces: &[ExecutionTraceRow],
    variants: &[SkillVariantRecord],
    now_ms: u64,
) -> GenePoolRuntimeSnapshot {
    let mut candidates_by_skill = BTreeMap::<String, GenePoolCandidate>::new();
    for trace in successful_traces {
        let Some(candidate) = build_candidate(
            trace,
            |tool_sequence, task_type| infer_draft_context_tags(tool_sequence, task_type, None),
            sanitize_agentskills_name,
        ) else {
            continue;
        };
        match candidates_by_skill.get(&candidate.proposed_skill_name) {
            Some(existing) if existing.quality_score >= candidate.quality_score => {}
            _ => {
                candidates_by_skill.insert(candidate.proposed_skill_name.clone(), candidate);
            }
        }
    }
    let candidates = candidates_by_skill.into_values().collect::<Vec<_>>();

    let mut arena_scores = variants
        .iter()
        .map(|record| GenePoolArenaScore {
            variant_id: record.variant_id.clone(),
            skill_name: record.skill_name.clone(),
            variant_name: record.variant_name.clone(),
            status: record.status.clone(),
            arena_score: compute_arena_score(record),
            success_rate: record.success_rate(),
            fitness_score: record.fitness_score,
        })
        .collect::<Vec<_>>();
    arena_scores.sort_by(|left, right| {
        right
            .arena_score
            .partial_cmp(&left.arena_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.variant_id.cmp(&right.variant_id))
    });

    let by_variant = arena_scores
        .iter()
        .map(|score| (score.variant_id.clone(), score))
        .collect::<BTreeMap<_, _>>();
    let arena_score_by_variant = arena_scores
        .iter()
        .map(|score| (score.variant_id.clone(), score.arena_score))
        .collect::<BTreeMap<_, _>>();
    let fitness_history = build_fitness_history(variants, now_ms);
    let cross_breed_proposals =
        build_cross_breed_proposals(variants, &arena_score_by_variant, now_ms);
    let mut lifecycle_actions = Vec::new();
    for record in variants {
        let Some(score) = by_variant.get(&record.variant_id) else {
            continue;
        };
        if matches!(record.status.as_str(), "draft" | "testing")
            && score.arena_score >= PROMOTION_SCORE_THRESHOLD
            && record.success_count >= MIN_PROMOTION_SUCCESSES
        {
            lifecycle_actions.push(GenePoolLifecycleAction {
                action: "promote".to_string(),
                variant_id: Some(record.variant_id.clone()),
                reason: format!(
                    "arena score {:.2} with success rate {:.0}% is strong enough for promotion",
                    score.arena_score,
                    score.success_rate * 100.0
                ),
                left_parent_variant_id: None,
                right_parent_variant_id: None,
            });
        }
        if matches!(record.status.as_str(), "active" | "proven" | "canonical")
            && score.arena_score <= RETIREMENT_SCORE_THRESHOLD
            && record.use_count >= MIN_RETIREMENT_USES
            && record.failure_count > record.success_count
        {
            lifecycle_actions.push(GenePoolLifecycleAction {
                action: "retire".to_string(),
                variant_id: Some(record.variant_id.clone()),
                reason: format!(
                    "arena score {:.2} with {} failures exceeds retirement threshold",
                    score.arena_score, record.failure_count
                ),
                left_parent_variant_id: None,
                right_parent_variant_id: None,
            });
        }
    }
    for proposal in &cross_breed_proposals {
        lifecycle_actions.push(GenePoolLifecycleAction {
            action: "cross_breed".to_string(),
            variant_id: None,
            reason: format!(
                "high-performing pair in {} reached co-usage rate {:.2}",
                proposal.skill_name, proposal.co_usage_rate
            ),
            left_parent_variant_id: Some(proposal.left_parent_variant_id.clone()),
            right_parent_variant_id: Some(proposal.right_parent_variant_id.clone()),
        });
    }
    let cross_breed_count = cross_breed_proposals.len();

    GenePoolRuntimeSnapshot {
        generated_at_ms: now_ms,
        candidates,
        arena_scores,
        fitness_history,
        cross_breed_proposals,
        summary: format!(
            "gene pool snapshot with {} candidates, {} arena scores, {} cross-breeds, {} lifecycle actions",
            successful_traces.len().min(GENE_POOL_TRACE_LIMIT),
            variants.len().min(GENE_POOL_VARIANT_LIMIT),
            cross_breed_count,
            lifecycle_actions.len()
        ),
        lifecycle_actions,
    }
}

impl AgentEngine {
    pub(crate) async fn refresh_gene_pool_runtime(&self) -> Result<GenePoolRuntimeSnapshot> {
        let successful_traces = self
            .history
            .list_recent_successful_traces(0, GENE_POOL_TRACE_LIMIT)
            .await?;
        let variants = self
            .history
            .list_skill_variants(None, GENE_POOL_VARIANT_LIMIT)
            .await?;
        let now = crate::agent::now_millis();

        let result = run_background_worker_command(
            BackgroundWorkerKind::Learning,
            BackgroundWorkerCommand::TickLearning {
                successful_traces,
                variants,
                now_ms: now,
            },
        )
        .await?;

        let snapshot = match result {
            BackgroundWorkerResult::LearningTick { snapshot } => snapshot,
            BackgroundWorkerResult::Error { message } => {
                anyhow::bail!("learning worker returned error: {message}");
            }
            other => anyhow::bail!("learning worker returned unexpected response: {other:?}"),
        };

        for action in &snapshot.lifecycle_actions {
            match action.action.as_str() {
                "promote" => {
                    let Some(variant_id) = action.variant_id.as_deref() else {
                        continue;
                    };
                    self.history.promote_skill_variant(variant_id).await?;
                }
                "retire" => {
                    let Some(variant_id) = action.variant_id.as_deref() else {
                        continue;
                    };
                    self.history.retire_skill_variant(variant_id).await?;
                }
                "cross_breed" => {
                    let Some(left_parent_variant_id) = action.left_parent_variant_id.as_deref()
                    else {
                        continue;
                    };
                    let Some(right_parent_variant_id) = action.right_parent_variant_id.as_deref()
                    else {
                        continue;
                    };
                    let Some(left) = self
                        .history
                        .get_skill_variant(left_parent_variant_id)
                        .await?
                    else {
                        continue;
                    };
                    let Some(right) = self
                        .history
                        .get_skill_variant(right_parent_variant_id)
                        .await?
                    else {
                        continue;
                    };
                    let _ = self
                        .history
                        .cross_breed_skill_variants(&left, &right)
                        .await?;
                }
                _ => {}
            }
        }
        self.history
            .record_gene_fitness_history(&snapshot.fitness_history)
            .await?;
        self.history
            .record_gene_crossbreed_proposals(&snapshot.cross_breed_proposals)
            .await?;

        self.history
            .set_consolidation_state(
                GENE_POOL_RUNTIME_SNAPSHOT_KEY,
                &serde_json::to_string(&snapshot)?,
                now,
            )
            .await?;

        Ok(snapshot)
    }
}
