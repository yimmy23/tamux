//! Skill consultation tracking and outcome attribution.

use super::*;
use crate::history::{SkillVariantConsultationRecord, SkillVariantRecord};

impl AgentEngine {
    pub(super) async fn record_skill_consultation(
        &self,
        thread_id: &str,
        task_id: Option<&str>,
        variant: &SkillVariantRecord,
        context_tags: &[String],
    ) {
        let (goal_run_id, _, _) = self.goal_context_for_task(task_id).await;
        let usage_id = format!("skill_usage_{}", Uuid::new_v4());
        if let Err(error) = self
            .history
            .record_skill_variant_consultation(&SkillVariantConsultationRecord {
                usage_id: &usage_id,
                variant_id: &variant.variant_id,
                thread_id: Some(thread_id),
                task_id,
                goal_run_id: goal_run_id.as_deref(),
                context_tags,
                consulted_at: now_millis(),
            })
            .await
        {
            tracing::warn!(
                thread_id,
                task_id,
                variant_id = %variant.variant_id,
                error = %error,
                "failed to record skill consultation"
            );
        }
    }

    pub(super) async fn settle_task_skill_consultations(
        &self,
        task: &AgentTask,
        outcome: &str,
    ) -> usize {
        self.settle_skill_consultations(
            task.thread_id.as_deref(),
            Some(task.id.as_str()),
            task.goal_run_id.as_deref(),
            outcome,
        )
        .await
    }

    pub(super) async fn settle_goal_skill_consultations(
        &self,
        goal_run: &GoalRun,
        outcome: &str,
    ) -> usize {
        self.settle_skill_consultations(
            goal_run.thread_id.as_deref(),
            None,
            Some(goal_run.id.as_str()),
            outcome,
        )
        .await
    }

    async fn settle_skill_consultations(
        &self,
        thread_id: Option<&str>,
        task_id: Option<&str>,
        goal_run_id: Option<&str>,
        outcome: &str,
    ) -> usize {
        match self
            .history
            .settle_skill_variant_usage(thread_id, task_id, goal_run_id, outcome)
            .await
        {
            Ok(count) => {
                let _ = self
                    .settle_skill_selection_causal_traces(thread_id, task_id, goal_run_id, outcome)
                    .await;
                if count > 0 {
                    if let Some(thread_id) = thread_id {
                        self.emit_workflow_notice(
                            thread_id,
                            "skill-evolution",
                            format!(
                                "Settled {count} skill consultation(s) with outcome={outcome}."
                            ),
                            Some(
                                serde_json::json!({
                                    "task_id": task_id,
                                    "goal_run_id": goal_run_id,
                                    "outcome": outcome,
                                    "count": count,
                                })
                                .to_string(),
                            ),
                        );
                    }
                }
                count
            }
            Err(error) => {
                tracing::warn!(
                    thread_id,
                    task_id,
                    goal_run_id,
                    outcome,
                    error = %error,
                    "failed to settle skill consultations"
                );
                0
            }
        }
    }

    /// Eagerly check whether a skill variant qualifies for lifecycle promotion
    /// after a successful settle (complementing the periodic consolidation check).
    ///
    /// Returns `Some(new_status)` if promoted, `None` otherwise.
    pub(super) async fn check_lifecycle_promotion_after_settle(
        &self,
        variant_id: &str,
    ) -> Option<String> {
        let variant = match self.history.get_skill_variant(variant_id).await {
            Ok(Some(v)) => v,
            _ => return None,
        };

        let config = self.config.read().await.clone();
        let thresholds = &config.skill_promotion;

        let (current, next, threshold) = match variant.status.as_str() {
            "testing" => ("testing", "active", thresholds.testing_to_active),
            "active" => ("active", "proven", thresholds.active_to_proven),
            "proven" => (
                "proven",
                "promoted_to_canonical",
                thresholds.proven_to_canonical,
            ),
            _ => return None,
        };

        if variant.success_count < threshold {
            return None;
        }

        if let Err(e) = self
            .history
            .update_skill_variant_status(variant_id, next)
            .await
        {
            tracing::warn!(
                error = %e,
                variant_id,
                "failed to promote skill variant at settle time"
            );
            return None;
        }

        // Record provenance for settle-time promotion (D-07)
        self.record_provenance_event(
            "skill_lifecycle_promotion",
            &format!(
                "Skill '{}' eagerly promoted {} -> {} at settle time (success_count {} >= {})",
                variant.skill_name, current, next, variant.success_count, threshold
            ),
            serde_json::json!({
                "variant_id": variant_id,
                "skill_name": variant.skill_name,
                "from_status": current,
                "to_status": next,
                "success_count": variant.success_count,
                "threshold": threshold,
                "trigger": "settle_time",
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
            variant_id,
            from = current,
            to = next,
            "skill promoted at settle time"
        );

        // Announce promotion via HeartbeatDigest (and WorkflowNotice for canonical)
        self.announce_skill_promotion(&variant.skill_name, current, next, variant.success_count);

        Some(next.to_string())
    }
}
