#![allow(dead_code)]

//! Skill consultation tracking and outcome attribution.

use super::*;
use crate::history::{
    PendingSkillVariantConsultation, SkillVariantConsultationRecord, SkillVariantRecord,
};

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
            return;
        }

        self.record_skill_consultation_graph_links(variant, context_tags)
            .await;
    }

    async fn record_skill_consultation_graph_links(
        &self,
        variant: &SkillVariantRecord,
        context_tags: &[String],
    ) {
        let skill_node_id = format!("skill:{}", variant.variant_id);
        if let Err(error) = self
            .history
            .upsert_memory_node(
                &skill_node_id,
                &variant.skill_name,
                "skill_variant",
                Some(&format!("skill variant {}", variant.relative_path)),
                now_millis(),
            )
            .await
        {
            tracing::warn!(
                variant_id = %variant.variant_id,
                error = %error,
                "failed to upsert skill graph node"
            );
            return;
        }

        let mut graph_refs = context_tags
            .iter()
            .map(|tag| {
                (
                    format!("intent:{}", tag.to_ascii_lowercase()),
                    tag.clone(),
                    "intent_context_tag",
                )
            })
            .collect::<Vec<_>>();
        graph_refs.push((
            format!("intent:{}", variant.skill_name.to_ascii_lowercase()),
            variant.skill_name.clone(),
            "intent_skill_lookup",
        ));

        for (node_id, label, relation_type) in graph_refs {
            if let Err(error) = self
                .history
                .upsert_memory_node(
                    &node_id,
                    &label,
                    "intent",
                    Some("skill consultation intent"),
                    now_millis(),
                )
                .await
            {
                tracing::warn!(%error, %node_id, "failed to upsert consultation intent node");
                continue;
            }
            if let Err(error) = self
                .history
                .upsert_memory_edge(&node_id, &skill_node_id, relation_type, 1.0, now_millis())
                .await
            {
                tracing::warn!(%error, %node_id, %skill_node_id, "failed to upsert consultation graph edge");
            }
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
        let pending_consultations = if outcome.eq_ignore_ascii_case("success") {
            self.history
                .list_pending_skill_variant_consultations(thread_id, task_id, goal_run_id)
                .await
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        match self
            .history
            .settle_skill_variant_usage(thread_id, task_id, goal_run_id, outcome)
            .await
        {
            Ok((count, skill_names)) => {
                let _ = self
                    .settle_skill_selection_causal_traces(thread_id, task_id, goal_run_id, outcome)
                    .await;
                if count > 0 && outcome.eq_ignore_ascii_case("success") {
                    self.reinforce_skill_consultation_graph_links(&pending_consultations)
                        .await;
                }
                if count > 0 {
                    if let Some(thread_id) = thread_id {
                        let notice_message =
                            format_skill_settlement_notice(count, outcome, &skill_names);
                        let settled_skills = skill_names.iter().cloned().collect::<Vec<_>>();
                        self.emit_workflow_notice(
                            thread_id,
                            "skill-evolution",
                            notice_message,
                            Some(
                                serde_json::json!({
                                    "task_id": task_id,
                                    "goal_run_id": goal_run_id,
                                    "outcome": outcome,
                                    "count": count,
                                    "skills": settled_skills,
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

    async fn reinforce_skill_consultation_graph_links(
        &self,
        pending_consultations: &[PendingSkillVariantConsultation],
    ) {
        for consultation in pending_consultations {
            let Some(variant) = self
                .history
                .get_skill_variant(&consultation.variant_id)
                .await
                .ok()
                .flatten()
            else {
                continue;
            };

            let skill_node_id = format!("skill:{}", variant.variant_id);
            let mut graph_refs = consultation
                .context_tags
                .iter()
                .map(|tag| {
                    (
                        format!("intent:{}", tag.to_ascii_lowercase()),
                        "intent_context_tag",
                    )
                })
                .collect::<Vec<_>>();
            graph_refs.push((
                format!("intent:{}", variant.skill_name.to_ascii_lowercase()),
                "intent_skill_lookup",
            ));

            for (node_id, relation_type) in graph_refs {
                if let Err(error) = self
                    .history
                    .upsert_memory_edge(&node_id, &skill_node_id, relation_type, 1.0, now_millis())
                    .await
                {
                    tracing::warn!(
                        variant_id = %variant.variant_id,
                        %node_id,
                        %skill_node_id,
                        %relation_type,
                        error = %error,
                        "failed to reinforce consultation graph edge"
                    );
                }
            }

            for node_id in consultation
                .context_tags
                .iter()
                .map(|tag| format!("intent:{}", tag.to_ascii_lowercase()))
                .chain(std::iter::once(format!(
                    "intent:{}",
                    variant.skill_name.to_ascii_lowercase()
                )))
            {
                if let Err(error) = self
                    .history
                    .upsert_memory_edge(
                        &node_id,
                        &skill_node_id,
                        "intent_prefers_skill",
                        1.0,
                        now_millis(),
                    )
                    .await
                {
                    tracing::warn!(
                        variant_id = %variant.variant_id,
                        %node_id,
                        %skill_node_id,
                        error = %error,
                        "failed to reinforce recommendation preference edge"
                    );
                }

                self.reinforce_memory_relevance_signals(&node_id).await;
            }
        }
    }

    async fn reinforce_memory_relevance_signals(&self, intent_node_id: &str) {
        let mut frontier = std::collections::VecDeque::from([(intent_node_id.to_string(), 0u8)]);
        let mut seen_intents = std::collections::HashSet::from([intent_node_id.to_string()]);

        while let Some((current_intent, depth)) = frontier.pop_front() {
            let Ok(neighbors) = self
                .history
                .list_memory_graph_neighbors(&current_intent, 64)
                .await
            else {
                continue;
            };

            for row in neighbors {
                let target_matches = row.via_edge.target_node_id == current_intent;
                let source_matches = row.via_edge.source_node_id == current_intent;
                if !target_matches && !source_matches {
                    continue;
                }

                if row.node.node_type == "skill_variant" {
                    continue;
                }

                if row.node.node_type == "intent" {
                    if depth < 1 && seen_intents.insert(row.node.id.clone()) {
                        frontier.push_back((row.node.id.clone(), depth + 1));
                    }
                    continue;
                }

                let (source_node_id, target_node_id) = if target_matches {
                    (row.node.id.as_str(), current_intent.as_str())
                } else {
                    (current_intent.as_str(), row.node.id.as_str())
                };

                if let Err(error) = self
                    .history
                    .upsert_memory_edge(
                        source_node_id,
                        target_node_id,
                        &row.via_edge.relation_type,
                        1.0,
                        now_millis(),
                    )
                    .await
                {
                    tracing::warn!(
                        %source_node_id,
                        %target_node_id,
                        relation_type = %row.via_edge.relation_type,
                        error = %error,
                        "failed to reinforce memory-node relevance signal"
                    );
                }
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

fn format_skill_settlement_notice(count: usize, outcome: &str, skill_names: &[String]) -> String {
    if skill_names.is_empty() {
        return format!("Settled {count} skill consultation(s) with outcome={outcome}.");
    }
    format!(
        "Settled {count} skill consultation(s) with outcome={outcome} for {}.",
        skill_names.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::format_skill_settlement_notice;

    #[test]
    fn skill_settlement_notice_mentions_skill_names_when_available() {
        let skills = vec![
            "systematic-debugging".to_string(),
            "debugging-playbook".to_string(),
        ];

        let notice = format_skill_settlement_notice(2, "success", &skills);

        assert!(notice.contains("outcome=success"));
        assert!(notice.contains("systematic-debugging"));
        assert!(notice.contains("debugging-playbook"));
    }

    #[test]
    fn skill_settlement_notice_omits_skill_list_when_unknown() {
        let notice = format_skill_settlement_notice(1, "failure", &[]);

        assert_eq!(
            notice,
            "Settled 1 skill consultation(s) with outcome=failure."
        );
    }
}
