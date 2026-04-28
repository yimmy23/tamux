use super::*;

const TEMPORAL_PATTERN_DECAY_RATE: f64 = 0.01;

impl AgentEngine {
    pub(super) async fn persist_temporal_foresight_runtime(&self, items: &[AnticipatoryItem]) {
        if let Err(error) = self.persist_intent_model_snapshot().await {
            tracing::warn!(%error, "failed to persist intent model snapshot");
        }
        if let Err(error) = self.persist_session_rhythm_patterns().await {
            tracing::warn!(%error, "failed to persist session rhythm temporal patterns");
        }

        for item in items {
            let result = match item.kind.as_str() {
                "intent_prediction" => {
                    self.persist_intent_prediction_temporal_artifacts(item)
                        .await
                }
                "system_outcome_foresight" => {
                    self.persist_system_outcome_temporal_artifacts(item).await
                }
                _ => Ok(()),
            };
            if let Err(error) = result {
                tracing::warn!(kind = %item.kind, %error, "failed to persist temporal foresight artifact");
            }
        }
    }

    pub(super) async fn record_anticipatory_prewarm_precomputation(
        &self,
        thread_id: &str,
        repo_root: &str,
        summary: &str,
    ) -> Option<(i64, i64)> {
        let now = now_millis();
        let pattern_id = self
            .history
            .insert_temporal_pattern(&crate::history::TemporalPatternRow {
                id: None,
                pattern_type: "file_access".to_string(),
                timescale: "minutes".to_string(),
                pattern_description: format!(
                    "After repo activity in {thread_id}, preload repository context before likely verification."
                ),
                context_filter: Some(format!("thread={thread_id};repo_root={repo_root}")),
                frequency: 1,
                last_observed_ms: now,
                first_observed_ms: now,
                confidence: 0.78,
                decay_rate: TEMPORAL_PATTERN_DECAY_RATE,
                created_at_ms: now,
            })
            .await
            .ok()?;
        let prediction_id = self
            .history
            .insert_temporal_prediction(&crate::history::TemporalPredictionRow {
                id: None,
                pattern_id,
                predicted_action: "inspect or test recent repo changes".to_string(),
                predicted_at_ms: now,
                confidence: 0.78,
                actual_action: None,
                was_accepted: None,
                accuracy_score: None,
            })
            .await
            .ok()?;
        let precomputation_id = self
            .history
            .insert_precomputation_log(&crate::history::PrecomputationLogRow {
                id: None,
                prediction_id,
                precomputation_type: "context_prefetch".to_string(),
                precomputation_details: summary.to_string(),
                started_at_ms: now,
                completed_at_ms: Some(now),
                was_used: None,
            })
            .await
            .ok()?;
        Some((prediction_id, precomputation_id))
    }

    pub(super) async fn mark_prewarm_cache_used(&self, thread_id: &str) {
        let precomputation_id = {
            let runtime = self.anticipatory.read().await;
            runtime
                .prewarm_cache_by_thread
                .get(thread_id)
                .and_then(|snapshot| snapshot.precomputation_id)
        };
        let Some(precomputation_id) = precomputation_id else {
            return;
        };
        if let Err(error) = self
            .history
            .update_precomputation_usage(precomputation_id, true, now_millis())
            .await
        {
            tracing::warn!(thread_id = %thread_id, %error, "failed to mark anticipatory precomputation as used");
        }
    }

    pub(super) async fn build_anticipatory_prompt_context(
        &self,
        thread_id: &str,
    ) -> Option<String> {
        let (intent_item, foresight_item, prewarm_summary) = {
            let runtime = self.anticipatory.read().await;
            let intent_item = runtime
                .items
                .iter()
                .find(|item| {
                    item.kind == "intent_prediction" && item.thread_id.as_deref() == Some(thread_id)
                })
                .cloned();
            let foresight_item = runtime
                .items
                .iter()
                .find(|item| {
                    item.kind == "system_outcome_foresight"
                        && item.thread_id.as_deref() == Some(thread_id)
                })
                .cloned();
            let prewarm_summary = runtime
                .prewarm_cache_by_thread
                .get(thread_id)
                .map(|snapshot| snapshot.summary.clone());
            (intent_item, foresight_item, prewarm_summary)
        };

        if intent_item.is_none() && foresight_item.is_none() && prewarm_summary.is_none() {
            return None;
        }

        self.mark_prewarm_cache_used(thread_id).await;

        let mut lines = Vec::new();
        if let Some(intent_item) = intent_item {
            if let Some(payload) = intent_item.intent_prediction {
                lines.push(format!(
                    "- Likely next action: {} (confidence {:.2})",
                    payload.primary_action, payload.confidence
                ));
                for candidate in payload.ranked_actions.iter().take(3) {
                    lines.push(format!(
                        "- Ranked candidate {}: {} ({:.2}) -- {}",
                        candidate.rank, candidate.action, candidate.confidence, candidate.rationale
                    ));
                }
            }
        }
        if let Some(foresight_item) = foresight_item {
            lines.push(format!(
                "- System foresight: {} (confidence {:.2})",
                foresight_item.summary, foresight_item.confidence
            ));
            for bullet in foresight_item.bullets.iter().take(3) {
                lines.push(format!("- {bullet}"));
            }
        }
        if let Some(summary) = prewarm_summary {
            lines.push(format!("- Cached precomputation: {summary}"));
        }

        Some(format!("## Temporal Foresight\n{}\n", lines.join("\n")))
    }

    async fn persist_intent_model_snapshot(&self) -> Result<()> {
        let model = self.operator_model.read().await.clone();
        let now = now_millis();
        self.history
            .upsert_intent_model(&crate::history::IntentModelRow {
                id: None,
                agent_id: crate::agent::agent_identity::WELES_AGENT_ID.to_string(),
                model_blob: Some(serde_json::to_vec(&serde_json::json!({
                    "session_rhythm": model.session_rhythm,
                    "attention_topology": model.attention_topology,
                    "operator_satisfaction": model.operator_satisfaction,
                    "implicit_feedback": model.implicit_feedback,
                }))?),
                created_at_ms: now,
                accuracy_score: self
                    .history
                    .recent_intent_prediction_success_rate("continue the active thread", 12)
                    .await?,
            })
            .await
    }

    async fn persist_session_rhythm_patterns(&self) -> Result<()> {
        let model = self.operator_model.read().await.clone();
        if model.session_rhythm.session_count == 0 {
            return Ok(());
        }

        let now = now_millis();
        let mut patterns = Vec::new();
        if let Some(hour) = model.session_rhythm.typical_start_hour_utc {
            patterns.push((
                "session_start",
                "days",
                format!("Typical session start around {:02}:00 UTC.", hour),
            ));
        }
        if let Some(hour) = model.session_rhythm.typical_start_hour_utc {
            patterns.push((
                "weekly_rhythm",
                "weeks",
                format!(
                    "Weekly rhythm still clusters around {:02}:00 UTC starts.",
                    hour
                ),
            ));
        }
        if model.session_rhythm.session_duration_avg_minutes > 0.0 {
            patterns.push((
                "session_start",
                "hours",
                format!(
                    "Average active session spans about {:.0} minutes.",
                    model.session_rhythm.session_duration_avg_minutes
                ),
            ));
        }

        for (pattern_type, timescale, description) in patterns {
            self.history
                .insert_temporal_pattern(&crate::history::TemporalPatternRow {
                    id: None,
                    pattern_type: pattern_type.to_string(),
                    timescale: timescale.to_string(),
                    pattern_description: description,
                    context_filter: None,
                    frequency: model.session_rhythm.session_count,
                    last_observed_ms: now,
                    first_observed_ms: now,
                    confidence: model.operator_satisfaction.score.clamp(0.25, 0.95),
                    decay_rate: TEMPORAL_PATTERN_DECAY_RATE,
                    created_at_ms: now,
                })
                .await?;
        }
        Ok(())
    }

    async fn persist_intent_prediction_temporal_artifacts(
        &self,
        item: &AnticipatoryItem,
    ) -> Result<()> {
        let Some(payload) = item.intent_prediction.as_ref() else {
            return Ok(());
        };
        let now = now_millis();
        for timescale in ["seconds", "minutes"] {
            let pattern_id = self
                .history
                .insert_temporal_pattern(&crate::history::TemporalPatternRow {
                    id: None,
                    pattern_type: "task_sequence".to_string(),
                    timescale: timescale.to_string(),
                    pattern_description: format!(
                        "Recent context predicts '{}' as the likely next step.",
                        payload.primary_action
                    ),
                    context_filter: item
                        .thread_id
                        .as_ref()
                        .map(|thread_id| format!("thread={thread_id}")),
                    frequency: 1,
                    last_observed_ms: now,
                    first_observed_ms: now,
                    confidence: payload.confidence,
                    decay_rate: TEMPORAL_PATTERN_DECAY_RATE,
                    created_at_ms: now,
                })
                .await?;
            self.history
                .insert_temporal_prediction(&crate::history::TemporalPredictionRow {
                    id: None,
                    pattern_id,
                    predicted_action: payload.primary_action.clone(),
                    predicted_at_ms: now,
                    confidence: payload.confidence,
                    actual_action: None,
                    was_accepted: None,
                    accuracy_score: None,
                })
                .await?;
        }
        Ok(())
    }

    async fn persist_system_outcome_temporal_artifacts(
        &self,
        item: &AnticipatoryItem,
    ) -> Result<()> {
        let prediction_type = item
            .bullets
            .iter()
            .find_map(|bullet| bullet.strip_prefix("prediction_type="))
            .unwrap_or("unknown");
        let predicted_action = if prediction_type == "stale_context" {
            "refresh thread context"
        } else {
            "verify build or test status"
        };
        let now = now_millis();
        let pattern_id = self
            .history
            .insert_temporal_pattern(&crate::history::TemporalPatternRow {
                id: None,
                pattern_type: "tool_usage".to_string(),
                timescale: "hours".to_string(),
                pattern_description: format!(
                    "System foresight predicts '{}' after the current workflow state.",
                    predicted_action
                ),
                context_filter: item
                    .thread_id
                    .as_ref()
                    .map(|thread_id| format!("thread={thread_id}")),
                frequency: 1,
                last_observed_ms: now,
                first_observed_ms: now,
                confidence: item.confidence,
                decay_rate: TEMPORAL_PATTERN_DECAY_RATE,
                created_at_ms: now,
            })
            .await?;
        self.history
            .insert_temporal_prediction(&crate::history::TemporalPredictionRow {
                id: None,
                pattern_id,
                predicted_action: predicted_action.to_string(),
                predicted_at_ms: now,
                confidence: item.confidence,
                actual_action: None,
                was_accepted: None,
                accuracy_score: None,
            })
            .await?;
        Ok(())
    }
}
