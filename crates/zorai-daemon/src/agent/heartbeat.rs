#![allow(dead_code)]

//! Heartbeat system — periodic health checks and notifications.
//!
//! Contains the structured heartbeat orchestration (`run_structured_heartbeat`) that:
//! - Gathers all built-in check results (stale todos, stuck goals, unreplied messages, repo changes)
//! - Collects custom HeartbeatItem prompts that are due
//! - Feeds everything into a single LLM synthesis call (BEAT-08/D-09)
//! - Broadcasts HeartbeatDigest only when actionable (BEAT-03/D-14)
//! - Persists every cycle to SQLite regardless of LLM outcome (D-12/Pitfall 4)

use super::*;
use chrono::Timelike;

mod helpers;
mod legacy;
mod postprocess;
use helpers::{
    check_quiet_window, check_type_to_action_type, format_anticipatory_items_for_heartbeat,
    format_speculative_summary_for_heartbeat, heartbeat_persistence_status, is_custom_item_due,
    parse_digest_items, should_broadcast, should_run_check,
};
#[cfg(test)]
pub(crate) use helpers::{compute_check_priority, enabled_checks};
pub(in crate::agent) use helpers::{is_peak_activity_hour, resolve_cron_from_config};

impl AgentEngine {
    /// Check if current time falls within quiet hours or DND is active. Per D-07.
    pub(super) async fn is_quiet_hours(&self) -> bool {
        let config = self.config.read().await;
        let hour = chrono::Local::now().hour();
        check_quiet_window(
            hour,
            config.quiet_hours_start,
            config.quiet_hours_end,
            config.dnd_enabled,
        )
    }

    /// Resolve the effective cron expression from config. Per D-08.
    /// Prefers heartbeat_cron if set, otherwise converts heartbeat_interval_mins.
    pub(super) async fn resolve_heartbeat_cron(&self) -> String {
        let config = self.config.read().await;
        resolve_cron_from_config(&config)
    }

    /// Run the structured heartbeat (backward-compatible wrapper).
    /// Delegates to `run_structured_heartbeat_adaptive(0)` so existing callers
    /// (tests, legacy code paths) continue to work without changes.
    pub(super) async fn run_structured_heartbeat(&self) -> Result<()> {
        self.run_structured_heartbeat_adaptive(0).await
    }

    /// Run the structured heartbeat with activity-aware priority gating and
    /// learned weight updates. `cycle_count` drives `should_run_check` modular
    /// gating: weight 1.0 = every cycle, 0.5 = every 2nd, etc.
    ///
    /// Per D-01, D-04, D-06, D-09, D-10, D-11, D-12, D-14, BEAT-02 through BEAT-08.
    pub(super) async fn run_structured_heartbeat_adaptive(&self, cycle_count: u64) -> Result<()> {
        let start = std::time::Instant::now();
        let cycle_id = uuid::Uuid::new_v4().to_string();
        let now = now_millis();

        // --- Phase 0: Check for global priority reset (D-11) ---
        let config = self.config.read().await.clone();
        let checks_config = &config.heartbeat_checks;

        if checks_config.reset_learned_priorities {
            tracing::info!("resetting all learned priority weights to defaults");
            let mut weights = self.learned_check_weights.write().await;
            weights.clear();
            drop(weights);
            // Note: The config flag is a one-shot action. The user should set it back to false
            // after reset. If they leave it true, it just means weights stay at defaults.
        }

        // --- Phase 1: Priority-aware check gathering (D-01, D-06, D-11) ---
        // Three-level priority cascade: override > learned > config default.
        let mut check_results: Vec<HeartbeatCheckResult> = Vec::new();
        {
            let learned_weights = self.learned_check_weights.read().await;

            let effective_stale_weight = checks_config
                .stale_todos_priority_override
                .unwrap_or_else(|| {
                    learned_weights
                        .get(&HeartbeatCheckType::StaleTodos)
                        .copied()
                        .unwrap_or(checks_config.stale_todos_priority_weight)
                });

            let effective_stuck_weight = checks_config
                .stuck_goals_priority_override
                .unwrap_or_else(|| {
                    learned_weights
                        .get(&HeartbeatCheckType::StuckGoalRuns)
                        .copied()
                        .unwrap_or(checks_config.stuck_goals_priority_weight)
                });

            let effective_unreplied_weight = checks_config
                .unreplied_messages_priority_override
                .unwrap_or_else(|| {
                    learned_weights
                        .get(&HeartbeatCheckType::UnrepliedGatewayMessages)
                        .copied()
                        .unwrap_or(checks_config.unreplied_messages_priority_weight)
                });

            let effective_repo_weight = checks_config
                .repo_changes_priority_override
                .unwrap_or_else(|| {
                    learned_weights
                        .get(&HeartbeatCheckType::RepoChanges)
                        .copied()
                        .unwrap_or(checks_config.repo_changes_priority_weight)
                });
            let effective_plugin_auth_weight = checks_config
                .plugin_auth_priority_override
                .unwrap_or_else(|| {
                    learned_weights
                        .get(&HeartbeatCheckType::PluginAuth)
                        .copied()
                        .unwrap_or(checks_config.plugin_auth_priority_weight)
                });

            drop(learned_weights);

            if checks_config.stale_todos_enabled
                && should_run_check(effective_stale_weight, cycle_count)
            {
                check_results.push(
                    self.check_stale_todos(checks_config.stale_todo_threshold_hours)
                        .await,
                );
            }
            if checks_config.stuck_goals_enabled
                && should_run_check(effective_stuck_weight, cycle_count)
            {
                check_results.push(
                    self.check_stuck_goal_runs(checks_config.stuck_goal_threshold_hours)
                        .await,
                );
            }
            if checks_config.unreplied_messages_enabled
                && should_run_check(effective_unreplied_weight, cycle_count)
            {
                check_results.push(
                    self.check_unreplied_messages(checks_config.unreplied_message_threshold_hours)
                        .await,
                );
            }
            if checks_config.repo_changes_enabled
                && should_run_check(effective_repo_weight, cycle_count)
            {
                check_results.push(self.check_repo_changes().await);
            }
            if checks_config.plugin_auth_enabled
                && should_run_check(effective_plugin_auth_weight, cycle_count)
            {
                check_results.push(self.check_plugin_auth().await);
            }
        }

        // --- Phase 1.5: Emit trajectory updates for active goal runs (AWAR-04) ---
        {
            let goal_runs = self.goal_runs.lock().await;
            let running: Vec<_> = goal_runs
                .iter()
                .filter(|g| g.status == GoalRunStatus::Running)
                .map(|g| (g.id.clone(), g.goal.clone()))
                .collect();
            drop(goal_runs);

            for (gr_id, gr_goal) in &running {
                if let Some(traj) = self.get_awareness_trajectory(gr_id).await {
                    let _ = self.event_tx.send(AgentEvent::TrajectoryUpdate {
                        goal_run_id: gr_id.clone(),
                        direction: traj.label.to_string(),
                        progress_ratio: traj.progress_ratio,
                        message: format!(
                            "Goal '{}' trajectory: {} ({:.0}% progress ratio)",
                            gr_goal.chars().take(50).collect::<String>(),
                            traj.label,
                            traj.progress_ratio * 100.0,
                        ),
                    });
                }
            }
        }

        // --- Phase 2: Run custom HeartbeatItem checks (per D-03) ---
        let custom_items = self.heartbeat_items.read().await.clone();
        let mut custom_summaries: Vec<String> = Vec::new();
        for item in &custom_items {
            if !item.enabled {
                continue;
            }
            if !is_custom_item_due(
                now,
                item.last_run_at,
                item.interval_minutes,
                config.heartbeat_interval_mins,
            ) {
                continue;
            }
            custom_summaries.push(format!("- Custom check '{}': {}", item.label, item.prompt));
        }

        // --- Phase 2.5: Gather anticipatory items for heartbeat merge (D-07, D-08, D-09) ---
        let (anticipatory_items, is_first_heartbeat) = self.get_anticipatory_for_heartbeat().await;

        let speculative_summary = {
            let runtime = self.anticipatory.read().await;
            let queued = runtime
                .opportunity_queue
                .iter()
                .cloned()
                .collect::<Vec<_>>();
            let cached = runtime
                .speculative_results_by_thread
                .values()
                .flat_map(|results| results.iter().cloned())
                .collect::<Vec<_>>();
            format_speculative_summary_for_heartbeat(&queued, &cached, now)
        };
        let anticipatory_summary = {
            let mut parts = Vec::new();
            if !anticipatory_items.is_empty() {
                parts.push(format_anticipatory_items_for_heartbeat(&anticipatory_items));
            }
            if let Some(speculative_summary) = speculative_summary {
                parts.push(speculative_summary);
            }
            parts.join("\n")
        };

        let anticipatory_section = if anticipatory_summary.is_empty() {
            String::new()
        } else {
            format!("\n\n== Anticipatory Items ==\n{}", anticipatory_summary)
        };

        let morning_brief_note = if is_first_heartbeat {
            "\n\nNOTE: This is the FIRST heartbeat of a new session. Include a morning brief \
             summary of overnight changes, stuck items, and context for today's work. \
             Mark morning brief items clearly in your DIGEST."
        } else {
            ""
        };

        // --- Phase 2.6: Learning transparency -- detect and report meaningful pattern changes (D-10) ---
        let mut learning_observations: Vec<String> = Vec::new();

        // (a) Detect peak hours change: compare current smoothed peak hours to last-reported.
        {
            let model = self.operator_model.read().await;
            let config_snap = self.config.read().await;
            let threshold = config_snap.ema_activity_threshold;
            drop(config_snap);

            // Compute current peak hours from smoothed histogram
            let mut current_peaks: Vec<u8> = model
                .session_rhythm
                .smoothed_activity_histogram
                .iter()
                .filter(|(_, &v)| v >= threshold)
                .map(|(&h, _)| h)
                .collect();
            current_peaks.sort();

            let previous_peaks = &model.session_rhythm.peak_activity_hours_utc;

            // Meaningful change: symmetric difference has > 2 hours
            let added: Vec<u8> = current_peaks
                .iter()
                .filter(|h| !previous_peaks.contains(h))
                .copied()
                .collect();
            let removed: Vec<u8> = previous_peaks
                .iter()
                .filter(|h| !current_peaks.contains(h))
                .copied()
                .collect();

            if added.len() + removed.len() > 2 && model.session_rhythm.session_count >= 5 {
                let peak_str = current_peaks
                    .iter()
                    .map(|h| format!("{}:00", h))
                    .collect::<Vec<_>>()
                    .join(", ");
                let data = serde_json::json!({ "peak_hours": peak_str });
                if let ExplanationResult::Template(explanation) =
                    generate_explanation("schedule_learned", &data)
                {
                    learning_observations.push(explanation);
                }
            }
        }

        // (b) Detect check deprioritization: when a learned weight crosses below 0.5.
        {
            let weights = self.learned_check_weights.read().await;
            let check_names = [
                (HeartbeatCheckType::StaleTodos, "stale todos"),
                (HeartbeatCheckType::StuckGoalRuns, "stuck goals"),
                (
                    HeartbeatCheckType::UnrepliedGatewayMessages,
                    "unreplied messages",
                ),
                (HeartbeatCheckType::RepoChanges, "repo changes"),
                (HeartbeatCheckType::PluginAuth, "plugin auth"),
            ];
            for (check_type, check_name) in &check_names {
                if let Some(&weight) = weights.get(check_type) {
                    if weight < 0.5 {
                        let data =
                            serde_json::json!({ "check_name": check_name, "weight": weight });
                        if let ExplanationResult::Template(explanation) =
                            generate_explanation("check_deprioritized", &data)
                        {
                            learning_observations.push(explanation);
                        }
                    }
                }
            }
        }

        let learning_section = if learning_observations.is_empty() {
            String::new()
        } else {
            let observations = learning_observations
                .iter()
                .map(|obs| format!("- [learning] {}", obs))
                .collect::<Vec<_>>()
                .join("\n");
            format!("\n\n== Learning Observations ==\n{}", observations)
        };

        // --- Phase 3: Build LLM synthesis prompt (per D-09, D-10) ---
        let built_in_summary = check_results
            .iter()
            .map(|r| {
                let mut s = format!(
                    "### {:?} ({})\n{}",
                    r.check_type,
                    if r.items_found > 0 {
                        format!("{} item(s)", r.items_found)
                    } else {
                        "clear".into()
                    },
                    r.summary
                );
                for detail in &r.details {
                    s.push_str(&format!(
                        "\n  - [{:?}] {} ({:.1}h): {}",
                        detail.severity, detail.label, detail.age_hours, detail.context
                    ));
                }
                s
            })
            .collect::<Vec<_>>()
            .join("\n\n");

        let custom_summary = if custom_summaries.is_empty() {
            "No custom checks configured.".to_string()
        } else {
            custom_summaries.join("\n")
        };

        let synthesis_prompt = format!(
            "HEARTBEAT SYNTHESIS\n\
             You are performing a scheduled heartbeat check for the operator. \
             Analyze the following data and respond in this exact format:\n\n\
             ACTIONABLE: true|false\n\
             DIGEST: <one-line natural language summary>\n\
             ITEMS:\n\
             - PRIORITY:<1-5> TYPE:<check_type> TITLE:<brief title> SUGGESTION:<suggested action>\n\
             ...\n\n\
             Rules:\n\
             - Set ACTIONABLE to true only if there are items genuinely needing operator attention\n\
             - Keep DIGEST concise (under 200 chars)\n\
             - Priority 1 is highest urgency\n\
             - If nothing is actionable, you may still note meta-observations in DIGEST \
               (e.g., 'All systems quiet for 3 days - is the project stalled?')\n\
             - Do NOT include items that are purely informational with no suggested action\n\
             - Anticipatory items marked LOW-PRIORITY INFORMATIONAL are context only, not actionable\n\
             - Learning observations should be mentioned in DIGEST when present\n\n\
             == Built-in Check Results ==\n{}\n\n\
             == Custom Check Results ==\n{}{}{}{}\n",
            built_in_summary,
            custom_summary,
            anticipatory_section,
            morning_brief_note,
            learning_section,
        );

        // --- Phase 4: Single LLM synthesis call (per D-09, D-10, BEAT-08) ---
        let checks_json = serde_json::to_string(&check_results).unwrap_or_default();
        let (synthesis_json, actionable, digest_text, digest_items, llm_tokens) = match self
            .send_internal_message_as(
                None,
                crate::agent::agent_identity::WELES_AGENT_ID,
                &synthesis_prompt,
            )
            .await
        {
            Ok(thread_id) => {
                let threads = self.threads.read().await;
                let response = threads
                    .get(&thread_id)
                    .and_then(|t| {
                        t.messages
                            .iter()
                            .rev()
                            .find(|m| m.role == MessageRole::Assistant)
                            .map(|m| m.content.clone())
                    })
                    .unwrap_or_default();

                let actionable = response.contains("ACTIONABLE: true");
                let digest = response
                    .lines()
                    .find(|l| l.starts_with("DIGEST:"))
                    .map(|l| l.trim_start_matches("DIGEST:").trim().to_string())
                    .unwrap_or_else(|| {
                        if actionable {
                            "Items need attention.".into()
                        } else {
                            "All systems normal.".into()
                        }
                    });

                let items = parse_digest_items(&response);

                (
                    Some(response),
                    actionable,
                    digest,
                    items,
                    0u64, // token count from Done event not easily accessible here
                )
            }
            Err(e) => {
                tracing::error!("heartbeat LLM synthesis failed: {e}");
                (None, false, format!("Synthesis failed: {e}"), vec![], 0u64)
            }
        };

        // --- Phase 5: Persist to SQLite (per D-12, Pitfall 4) ---
        // CRITICAL: Persist REGARDLESS of LLM success/failure (Pitfall 4)
        let duration_ms = start.elapsed().as_millis() as i64;
        let status = heartbeat_persistence_status(synthesis_json.as_deref());
        if let Err(e) = self
            .history
            .insert_heartbeat_history(
                &cycle_id,
                now as i64,
                &checks_json,
                synthesis_json.as_deref(),
                actionable,
                Some(&digest_text),
                llm_tokens as i64,
                duration_ms,
                status,
            )
            .await
        {
            tracing::warn!("failed to persist heartbeat history: {e}");
        }

        self.refresh_weles_health_from_heartbeat(now).await;

        // --- Phase 6: Broadcast to clients (per D-11, D-13, D-14, BEAT-03, BEAT-04) ---
        // Build composite explanation from digest items per D-01.
        let digest_explanation = if digest_items.is_empty() {
            None
        } else if digest_items.len() == 1 {
            let item = &digest_items[0];
            let action_type = check_type_to_action_type(&item.check_type);
            let item_data = serde_json::json!({
                "title": item.title,
                "hours": 0,
                "count": 1,
                "source": "heartbeat",
                "repo": "unknown",
            });
            match generate_explanation(action_type, &item_data) {
                ExplanationResult::Template(s) => Some(s),
                ExplanationResult::NeedsLlm => Some(item.title.clone()),
            }
        } else {
            let mut parts = Vec::new();
            for item in &digest_items {
                let action_type = check_type_to_action_type(&item.check_type);
                let item_data = serde_json::json!({
                    "title": item.title,
                    "hours": 0,
                    "count": 1,
                    "source": "heartbeat",
                    "repo": "unknown",
                });
                match generate_explanation(action_type, &item_data) {
                    ExplanationResult::Template(s) => parts.push(s),
                    ExplanationResult::NeedsLlm => parts.push(item.title.clone()),
                }
            }
            Some(format!(
                "Found {} items: {}",
                digest_items.len(),
                parts.join("; ")
            ))
        };

        // Only broadcast when actionable OR LLM had something to say (per D-14: silent by default)
        if should_broadcast(actionable, &digest_items) {
            let _ = self.event_tx.send(AgentEvent::HeartbeatDigest {
                cycle_id: cycle_id.clone(),
                actionable,
                digest: digest_text.clone(),
                items: digest_items.clone(),
                checked_at: now,
                explanation: digest_explanation,
                confidence: None, // Heartbeat checks don't have confidence
            });
        } else {
            tracing::debug!(cycle_id = %cycle_id, "heartbeat quiet tick — no broadcast");
        }

        // Clear morning brief flag after consumption (Pitfall 3: prevent repeat).
        // Only clear AFTER successful synthesis, not before.
        if is_first_heartbeat && synthesis_json.is_some() {
            self.anticipatory.write().await.session_start_pending_at = None;
            tracing::info!("morning brief consumed in heartbeat digest");
        }

        self.finalize_heartbeat_postprocess(
            &cycle_id,
            now,
            &config,
            &digest_items,
            start,
            actionable,
            check_results.len(),
        )
        .await;

        Ok(())
    }

    pub async fn get_heartbeat_items(&self) -> Vec<HeartbeatItem> {
        self.heartbeat_items.read().await.clone()
    }

    pub async fn set_heartbeat_items(&self, items: Vec<HeartbeatItem>) {
        *self.heartbeat_items.write().await = items;
        self.persist_heartbeat().await;
    }
}

#[cfg(test)]
#[path = "tests/heartbeat.rs"]
mod tests;
