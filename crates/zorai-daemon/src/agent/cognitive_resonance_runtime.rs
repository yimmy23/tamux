use super::*;

const RESONANCE_ADJUSTMENT_DELTA_THRESHOLD: f64 = 0.2;

impl AgentEngine {
    pub(super) async fn sample_cognitive_resonance_runtime(&self) {
        let now = now_millis();
        let model = self.operator_model.read().await.clone();
        let snapshot = CognitiveResonanceSnapshot::from_model(&model);
        let attention_thread_id = self
            .anticipatory
            .read()
            .await
            .active_attention_thread_id
            .clone();
        let revision_velocity_ms = self
            .derive_revision_velocity_ms(attention_thread_id.as_deref(), now)
            .await;
        let session_entropy = derive_session_entropy(&model);
        let approval_latency_ms = if model.risk_fingerprint.approval_requests > 0 {
            Some((model.risk_fingerprint.avg_response_time_secs.max(0.0) * 1000.0) as u64)
        } else {
            None
        };

        let previous = self
            .history
            .list_cognitive_resonance_samples(1)
            .await
            .ok()
            .and_then(|mut rows| rows.pop());

        if let Some(previous) = previous.as_ref() {
            let current_state = format!("{:?}", snapshot.state).to_ascii_lowercase();
            self.maybe_log_behavior_adjustment(
                "verbosity",
                previous.verbosity_adjustment,
                snapshot.adjustments.verbosity,
                &current_state,
                snapshot.score,
                now,
            )
            .await;
            self.maybe_log_behavior_adjustment(
                "risk_tolerance",
                previous.risk_adjustment,
                snapshot.adjustments.risk_tolerance,
                &current_state,
                snapshot.score,
                now,
            )
            .await;
            self.maybe_log_behavior_adjustment(
                "proactiveness",
                previous.proactiveness_adjustment,
                snapshot.adjustments.proactiveness,
                &current_state,
                snapshot.score,
                now,
            )
            .await;
            self.maybe_log_behavior_adjustment(
                "memory_urgency",
                previous.memory_urgency_adjustment,
                snapshot.adjustments.memory_urgency,
                &current_state,
                snapshot.score,
                now,
            )
            .await;
        }

        if let Err(error) = self
            .history
            .insert_cognitive_resonance_sample(&crate::history::CognitiveResonanceSampleRow {
                id: None,
                sampled_at_ms: now,
                revision_velocity_ms,
                session_entropy: Some(session_entropy),
                approval_latency_ms,
                tool_hesitation_count: model.implicit_feedback.tool_hesitation_count,
                cognitive_state: format!("{:?}", snapshot.state).to_ascii_lowercase(),
                state_confidence: snapshot.score.clamp(0.0, 1.0),
                resonance_score: snapshot.score,
                verbosity_adjustment: snapshot.adjustments.verbosity,
                risk_adjustment: snapshot.adjustments.risk_tolerance,
                proactiveness_adjustment: snapshot.adjustments.proactiveness,
                memory_urgency_adjustment: snapshot.adjustments.memory_urgency,
            })
            .await
        {
            tracing::warn!(%error, "failed to persist cognitive resonance sample");
        }
    }

    async fn maybe_log_behavior_adjustment(
        &self,
        parameter: &str,
        old_value: f64,
        new_value: f64,
        state: &str,
        resonance_score: f64,
        adjusted_at_ms: u64,
    ) {
        if (new_value - old_value).abs() < RESONANCE_ADJUSTMENT_DELTA_THRESHOLD {
            return;
        }
        if let Err(error) = self
            .history
            .insert_behavior_adjustment_log(&crate::history::BehaviorAdjustmentLogRow {
                id: None,
                adjusted_at_ms,
                parameter: parameter.to_string(),
                old_value,
                new_value,
                trigger_reason: format!(
                    "state={state}; delta={:.2}",
                    (new_value - old_value).abs()
                ),
                resonance_score,
            })
            .await
        {
            tracing::warn!(parameter = %parameter, %error, "failed to persist behavior adjustment log");
        }
    }

    async fn derive_revision_velocity_ms(&self, thread_id: Option<&str>, now: u64) -> Option<u64> {
        let thread_id = thread_id.unwrap_or("global");
        let signals = self
            .history
            .list_implicit_signals(thread_id, 12)
            .await
            .ok()?;
        signals
            .into_iter()
            .find(|signal| {
                matches!(
                    signal.signal_type.as_str(),
                    "operator_correction" | "high_revision_rate" | "rapid_revert"
                )
            })
            .map(|signal| now.saturating_sub(signal.timestamp_ms))
    }
}

fn derive_session_entropy(model: &OperatorModel) -> f64 {
    let switches = model.attention_topology.rapid_switch_count as f64;
    let focus_events = model.attention_topology.focus_event_count.max(1) as f64;
    (switches / focus_events).clamp(0.0, 1.0)
}
