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
        if let Err(error) =
            self.history
                .record_skill_variant_consultation(&SkillVariantConsultationRecord {
                    usage_id: &usage_id,
                    variant_id: &variant.variant_id,
                    thread_id: Some(thread_id),
                    task_id,
                    goal_run_id: goal_run_id.as_deref(),
                    context_tags,
                    consulted_at: now_millis(),
                })
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
}
