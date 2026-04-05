use super::*;

impl AgentEngine {
    pub(crate) async fn learned_approval_decision(
        &self,
        command: &str,
        risk_level: &str,
    ) -> Option<ApprovalDecision> {
        let settings = self.config.read().await.operator_model.clone();
        if !settings.enabled || !settings.allow_approval_learning {
            return None;
        }

        let category = classify_command_category(command, risk_level);
        let model = self.operator_model.read().await;
        if model
            .risk_fingerprint
            .auto_deny_categories
            .iter()
            .any(|candidate| candidate == category)
        {
            return Some(ApprovalDecision::Deny);
        }
        if model
            .risk_fingerprint
            .auto_approve_categories
            .iter()
            .any(|candidate| candidate == category)
        {
            return Some(ApprovalDecision::ApproveOnce);
        }
        None
    }

    pub(crate) async fn build_operator_model_prompt_summary(&self) -> Option<String> {
        let settings = self.config.read().await.operator_model.clone();
        if !settings.enabled {
            return None;
        }
        let model = self.operator_model.read().await;
        if model.cognitive_style.message_count == 0
            && model.risk_fingerprint.approval_requests == 0
            && model.attention_topology.focus_event_count == 0
        {
            return None;
        }

        let mut lines = Vec::new();
        if settings.allow_message_statistics && model.cognitive_style.message_count > 0 {
            lines.push(format!(
                "- Output density: {} (avg {:.1} words per message, questions {:.0}%)",
                verbosity_label(model.cognitive_style.verbosity_preference),
                model.cognitive_style.avg_message_length,
                model.cognitive_style.question_frequency * 100.0,
            ));
            if let Some(reading_pattern) = reading_pattern_summary(&model.cognitive_style) {
                lines.push(format!("- Reading pattern: {reading_pattern}"));
            }
        }
        if settings.allow_approval_learning && model.risk_fingerprint.approval_requests > 0 {
            lines.push(format!(
                "- Risk tolerance: {} ({} approvals across {} approval requests, avg response {:.1}s)",
                risk_tolerance_label(model.risk_fingerprint.risk_tolerance),
                model.risk_fingerprint.approvals,
                model.risk_fingerprint.approval_requests,
                model.risk_fingerprint.avg_response_time_secs,
            ));
            if !model.risk_fingerprint.auto_approve_categories.is_empty()
                || !model.risk_fingerprint.auto_deny_categories.is_empty()
            {
                let auto_approve = if model.risk_fingerprint.auto_approve_categories.is_empty() {
                    "none".to_string()
                } else {
                    model.risk_fingerprint.auto_approve_categories.join(", ")
                };
                let auto_deny = if model.risk_fingerprint.auto_deny_categories.is_empty() {
                    "none".to_string()
                } else {
                    model.risk_fingerprint.auto_deny_categories.join(", ")
                };
                lines.push(format!(
                    "- Learned approval shortcuts: auto-approve [{}]; auto-deny [{}]",
                    auto_approve, auto_deny
                ));
            }
        }
        if settings.allow_message_statistics {
            if let Some(hour) = model.session_rhythm.typical_start_hour_utc {
                lines.push(format!(
                    "- Session rhythm: usually starts around {:02}:00 UTC; avg observed session {:.1}m",
                    hour, model.session_rhythm.session_duration_avg_minutes,
                ));
            }
        }
        if settings.allow_attention_tracking && model.attention_topology.focus_event_count > 0 {
            let dominant_surface = model
                .attention_topology
                .dominant_surface
                .as_deref()
                .unwrap_or("unknown");
            let transitions = if model.attention_topology.top_transitions.is_empty() {
                "no stable transitions yet".to_string()
            } else {
                model.attention_topology.top_transitions.join(", ")
            };
            lines.push(format!(
                "- Attention topology: mainly {} ({} focus events, avg dwell {:.1}s, rapid switches {}); common transitions {}; deep focus {}",
                dominant_surface,
                model.attention_topology.focus_event_count,
                model.attention_topology.avg_surface_dwell_secs,
                model.attention_topology.rapid_switch_count,
                transitions,
                model.attention_topology.deep_focus_surface.as_deref().unwrap_or("unknown"),
            ));
        }
        if settings.allow_implicit_feedback
            && (model.implicit_feedback.tool_hesitation_count > 0
                || model.implicit_feedback.revision_message_count > 0
                || model.implicit_feedback.fast_denial_count > 0)
        {
            let fallback = model
                .implicit_feedback
                .top_tool_fallbacks
                .first()
                .cloned()
                .unwrap_or_else(|| "none yet".to_string());
            lines.push(format!(
                "- Implicit feedback: {} tool fallback(s), {} revision-style operator message(s), {} fast denial(s); common fallback {}",
                model.implicit_feedback.tool_hesitation_count,
                model.implicit_feedback.revision_message_count,
                model.implicit_feedback.fast_denial_count,
                fallback,
            ));
        }
        if lines.is_empty() {
            return None;
        }

        Some(format!("## Operator Model\n{}", lines.join("\n")))
    }

    pub(crate) async fn record_operator_message(
        &self,
        thread_id: &str,
        content: &str,
        is_new_thread: bool,
    ) -> Result<()> {
        let settings = self.config.read().await.operator_model.clone();
        if !settings.enabled
            || (!settings.allow_message_statistics && !settings.allow_implicit_feedback)
        {
            return Ok(());
        }
        ensure_operator_model_file(&self.data_dir).await?;
        let now = now_millis();
        let word_count = count_words(content) as f64;
        let is_question = content.contains('?');
        let confirmation_like = contains_confirmation_phrase(content);
        let revision_kind = detect_revision_signal(content);
        let reading_signal = detect_reading_signal(content);
        let current_hour_utc = current_utc_hour(now);

        let thread_created_at = {
            let threads = self.threads.read().await;
            threads.get(thread_id).map(|thread| thread.created_at)
        };

        let observed_minutes_delta = {
            let mut active_sessions = self.active_operator_sessions.write().await;
            if is_new_thread {
                active_sessions.insert(thread_id.to_string(), 0);
            }

            if let Some(created_at) = thread_created_at {
                let observed_minutes = now.saturating_sub(created_at) / 60_000;
                if let Some(previous_minutes) = active_sessions.get_mut(thread_id) {
                    if observed_minutes > *previous_minutes {
                        let delta = observed_minutes - *previous_minutes;
                        *previous_minutes = observed_minutes;
                        Some(delta)
                    } else {
                        None
                    }
                } else {
                    None
                }
            } else {
                None
            }
        };

        {
            let mut model = self.operator_model.write().await;
            model.last_updated = now;

            if settings.allow_message_statistics {
                let next_count = model.cognitive_style.message_count + 1;
                model.cognitive_style.avg_message_length = update_running_average(
                    model.cognitive_style.avg_message_length,
                    model.cognitive_style.message_count,
                    word_count,
                );
                model.cognitive_style.message_count = next_count;
                if is_question {
                    model.cognitive_style.question_count += 1;
                }
                if confirmation_like {
                    model.cognitive_style.confirmation_count += 1;
                }
                model.cognitive_style.question_frequency =
                    model.cognitive_style.question_count as f64 / next_count as f64;
                model.cognitive_style.confirmation_seeking =
                    model.cognitive_style.confirmation_count as f64 / next_count as f64;
                model.cognitive_style.verbosity_preference =
                    verbosity_preference_for_length(model.cognitive_style.avg_message_length);
                record_reading_signal(&mut model.cognitive_style, reading_signal);
            }
            if settings.allow_implicit_feedback {
                if revision_kind.is_revision() {
                    model.implicit_feedback.revision_message_count += 1;
                }
                if revision_kind.is_correction() {
                    model.implicit_feedback.correction_message_count += 1;
                }
            }

            if settings.allow_message_statistics {
                *model
                    .session_rhythm
                    .activity_hour_histogram
                    .entry(current_hour_utc)
                    .or_insert(0) += 1;
                model.session_rhythm.peak_activity_hours_utc =
                    top_hours(&model.session_rhythm.activity_hour_histogram, 3);

                if is_new_thread {
                    model.session_count += 1;
                    model.session_rhythm.session_count += 1;
                    *model
                        .session_rhythm
                        .start_hour_histogram
                        .entry(current_hour_utc)
                        .or_insert(0) += 1;
                    model.session_rhythm.typical_start_hour_utc =
                        most_common_hour(&model.session_rhythm.start_hour_histogram);
                }

                if let Some(delta) = observed_minutes_delta {
                    model.session_rhythm.total_observed_session_minutes += delta;
                    if model.session_rhythm.session_count > 0 {
                        model.session_rhythm.session_duration_avg_minutes =
                            model.session_rhythm.total_observed_session_minutes as f64
                                / model.session_rhythm.session_count as f64;
                    }
                }
            }

            persist_operator_model(&self.data_dir, &model)?;
        }
        self.record_behavioral_event(
            "operator_message",
            BehavioralEventContext {
                thread_id: Some(thread_id),
                task_id: None,
                goal_run_id: None,
                approval_id: None,
            },
            serde_json::json!({
                "is_new_thread": is_new_thread,
                "word_count": count_words(content),
                "is_question": is_question,
                "confirmation_like": confirmation_like,
                "revision_signal": format!("{revision_kind:?}").to_ascii_lowercase(),
            }),
        )
        .await?;

        Ok(())
    }

    pub(crate) async fn record_operator_approval_requested(
        &self,
        pending_approval: &ToolPendingApproval,
    ) -> Result<()> {
        let settings = self.config.read().await.operator_model.clone();
        if !settings.enabled || !settings.allow_approval_learning {
            return Ok(());
        }
        ensure_operator_model_file(&self.data_dir).await?;
        let category =
            classify_command_category(&pending_approval.command, &pending_approval.risk_level);
        self.pending_operator_approvals.write().await.insert(
            pending_approval.approval_id.clone(),
            PendingApprovalObservation {
                requested_at: now_millis(),
                category: category.to_string(),
            },
        );

        let mut model = self.operator_model.write().await;
        model.last_updated = now_millis();
        model.risk_fingerprint.approval_requests += 1;
        *model
            .risk_fingerprint
            .category_requests
            .entry(category.to_string())
            .or_insert(0) += 1;
        refresh_risk_metrics(&mut model.risk_fingerprint);
        persist_operator_model(&self.data_dir, &model)?;
        self.record_behavioral_event(
            "approval_requested",
            BehavioralEventContext {
                thread_id: None,
                task_id: None,
                goal_run_id: None,
                approval_id: Some(&pending_approval.approval_id),
            },
            serde_json::json!({
                "category": category,
                "command": pending_approval.command,
                "risk_level": pending_approval.risk_level,
            }),
        )
        .await?;
        Ok(())
    }

    pub async fn record_tool_hesitation(
        &self,
        from_tool: &str,
        to_tool: &str,
        from_error: bool,
        to_error: bool,
    ) -> Result<()> {
        let settings = self.config.read().await.operator_model.clone();
        if !settings.enabled || !settings.allow_implicit_feedback {
            return Ok(());
        }
        if !from_error || to_error {
            return Ok(());
        }
        let from_tool = from_tool.trim();
        let to_tool = to_tool.trim();
        if from_tool.is_empty() || to_tool.is_empty() || from_tool.eq_ignore_ascii_case(to_tool) {
            return Ok(());
        }

        ensure_operator_model_file(&self.data_dir).await?;
        let now = now_millis();
        let mut model = self.operator_model.write().await;
        model.last_updated = now;
        model.implicit_feedback.tool_hesitation_count += 1;
        let pair = format!("{from_tool} -> {to_tool}");
        *model
            .implicit_feedback
            .fallback_histogram
            .entry(pair)
            .or_insert(0) += 1;
        model.implicit_feedback.top_tool_fallbacks =
            top_keys(&model.implicit_feedback.fallback_histogram, 3);
        persist_operator_model(&self.data_dir, &model)?;
        self.record_behavioral_event(
            "tool_fallback",
            BehavioralEventContext {
                thread_id: None,
                task_id: None,
                goal_run_id: None,
                approval_id: None,
            },
            serde_json::json!({
                "from_tool": from_tool,
                "to_tool": to_tool,
                "from_error": from_error,
                "to_error": to_error,
            }),
        )
        .await?;
        Ok(())
    }

    pub async fn record_attention_surface(&self, surface: &str) -> Result<()> {
        let settings = self.config.read().await.operator_model.clone();
        if !settings.enabled || !settings.allow_attention_tracking {
            return Ok(());
        }
        let normalized = normalize_attention_surface(surface);
        if normalized.is_empty() {
            return Ok(());
        }

        ensure_operator_model_file(&self.data_dir).await?;
        let now = now_millis();
        let mut model = self.operator_model.write().await;
        model.last_updated = now;
        record_attention_event(&mut model, &normalized, now);
        persist_operator_model(&self.data_dir, &model)?;
        self.record_behavioral_event(
            "attention_surface",
            BehavioralEventContext {
                thread_id: None,
                task_id: None,
                goal_run_id: None,
                approval_id: None,
            },
            serde_json::json!({
                "surface": normalized,
            }),
        )
        .await?;
        Ok(())
    }

    pub async fn record_operator_approval_resolution(
        &self,
        approval_id: &str,
        decision: ApprovalDecision,
    ) -> Result<()> {
        let settings = self.config.read().await.operator_model.clone();
        if !settings.enabled || !settings.allow_approval_learning {
            return Ok(());
        }
        ensure_operator_model_file(&self.data_dir).await?;
        let pending = self
            .pending_operator_approvals
            .write()
            .await
            .remove(approval_id);
        let now = now_millis();

        let mut model = self.operator_model.write().await;
        model.last_updated = now;
        if matches!(
            decision,
            ApprovalDecision::ApproveOnce | ApprovalDecision::ApproveSession
        ) {
            model.risk_fingerprint.approvals += 1;
        } else {
            model.risk_fingerprint.denials += 1;
        }
        if let Some(pending) = pending {
            let category = pending.category.clone();
            if matches!(
                decision,
                ApprovalDecision::ApproveOnce | ApprovalDecision::ApproveSession
            ) {
                *model
                    .risk_fingerprint
                    .category_approvals
                    .entry(category.clone())
                    .or_insert(0) += 1;
            }
            let response_secs = now.saturating_sub(pending.requested_at) as f64 / 1000.0;
            let responses = model.risk_fingerprint.approvals + model.risk_fingerprint.denials;
            model.risk_fingerprint.avg_response_time_secs = update_running_average(
                model.risk_fingerprint.avg_response_time_secs,
                responses.saturating_sub(1),
                response_secs,
            );
            if settings.allow_implicit_feedback
                && matches!(decision, ApprovalDecision::Deny)
                && response_secs <= 8.0
            {
                model.implicit_feedback.fast_denial_count += 1;
                *model
                    .risk_fingerprint
                    .fast_denials_by_category
                    .entry(category)
                    .or_insert(0) += 1;
            }
        }
        refresh_risk_metrics(&mut model.risk_fingerprint);
        persist_operator_model(&self.data_dir, &model)?;
        self.record_behavioral_event(
            "approval_resolved",
            BehavioralEventContext {
                thread_id: None,
                task_id: None,
                goal_run_id: None,
                approval_id: Some(approval_id),
            },
            serde_json::json!({
                "decision": format!("{decision:?}").to_ascii_lowercase(),
            }),
        )
        .await?;
        Ok(())
    }

    pub(crate) fn operator_profile_diagnostics_snapshot(&self) -> serde_json::Value {
        let sync_state = match super::operator_profile::user_sync::current_user_sync_state() {
            super::operator_profile::user_sync::UserProfileSyncState::Clean => "clean",
            super::operator_profile::user_sync::UserProfileSyncState::Dirty => "dirty",
            super::operator_profile::user_sync::UserProfileSyncState::Reconciling => "reconciling",
        };
        serde_json::json!({
            "operator_profile_sync_state": sync_state,
            "operator_profile_sync_dirty": sync_state != "clean",
            "operator_profile_scheduler_fallback": false,
        })
    }
}
