//! Heartbeat system — periodic health checks and notifications.
//!
//! Contains the structured heartbeat orchestration (`run_structured_heartbeat`) that:
//! - Gathers all built-in check results (stale todos, stuck goals, unreplied messages, repo changes)
//! - Collects custom HeartbeatItem prompts that are due
//! - Feeds everything into a single LLM synthesis call (BEAT-08/D-09)
//! - Broadcasts HeartbeatDigest only when actionable (BEAT-03/D-14)
//! - Persists every cycle to SQLite regardless of LLM outcome (D-12/Pitfall 4)

use std::collections::HashMap;

use super::*;
use crate::history::AuditEntryRow;
use chrono::Timelike;

/// Pure function: check if a given hour falls within a quiet window.
///
/// Returns `true` when heartbeat execution should be suppressed.
/// Handles midnight-wrap ranges (e.g., start=22, end=6 means 22:00-05:59).
/// When `dnd` is `true`, always returns `true` (manual do-not-disturb).
pub(super) fn check_quiet_window(
    hour: u32,
    start: Option<u32>,
    end: Option<u32>,
    dnd: bool,
) -> bool {
    if dnd {
        return true;
    }
    let (s, e) = match (start, end) {
        (Some(s), Some(e)) => (s, e),
        _ => return false,
    };
    if s <= e {
        // Same-day range: e.g., 9..17 means 9:00-16:59
        hour >= s && hour < e
    } else {
        // Midnight wrap: e.g., 22..6 means 22:00-05:59
        hour >= s || hour < e
    }
}

/// Resolve the effective cron expression from config fields.
///
/// Prefers `heartbeat_cron` if set, otherwise converts `heartbeat_interval_mins`
/// via the legacy helper. Per D-08.
pub(super) fn resolve_cron_from_config(config: &AgentConfig) -> String {
    config
        .heartbeat_cron
        .clone()
        .unwrap_or_else(|| interval_mins_to_cron(config.heartbeat_interval_mins))
}

/// Pure function: determine whether to broadcast a HeartbeatDigest event.
///
/// Per D-14 (silent default): only broadcast when there are actionable items
/// OR the LLM produced digest items that need attention.
pub(super) fn should_broadcast(actionable: bool, items: &[HeartbeatDigestItem]) -> bool {
    actionable || !items.is_empty()
}

/// Pure function: determine the persistence status string for a heartbeat cycle.
///
/// Per Pitfall 4: persist EVERY cycle regardless of LLM success.
/// Returns "completed" when synthesis succeeded, "synthesis_failed" otherwise.
pub(super) fn heartbeat_persistence_status(synthesis_json: Option<&str>) -> &'static str {
    if synthesis_json.is_some() {
        "completed"
    } else {
        "synthesis_failed"
    }
}

/// Pure function: determine if a custom HeartbeatItem check is due for execution.
///
/// An item is due if:
/// - It has never been run (`last_run_at` is None), OR
/// - The elapsed time since last run exceeds the configured interval.
///
/// `item_interval_minutes` is the item's own interval (0 means use global).
/// `global_interval_mins` is the fallback from `config.heartbeat_interval_mins`.
pub(super) fn is_custom_item_due(
    now: u64,
    last_run_at: Option<u64>,
    item_interval_minutes: u64,
    global_interval_mins: u64,
) -> bool {
    let interval_ms = if item_interval_minutes > 0 {
        item_interval_minutes * 60 * 1000
    } else {
        global_interval_mins * 60 * 1000
    };
    match last_run_at {
        Some(last) => now.saturating_sub(last) >= interval_ms,
        None => true,
    }
}

/// Parse the LLM synthesis response into structured digest items.
///
/// Parses lines matching the format:
/// `- PRIORITY:<N> TYPE:<check_type> TITLE:<text> SUGGESTION:<text>`
pub(super) fn parse_digest_items(response: &str) -> Vec<HeartbeatDigestItem> {
    response
        .lines()
        .filter(|l| l.trim_start().starts_with("- PRIORITY:"))
        .filter_map(|line| {
            let priority = line
                .split("PRIORITY:")
                .nth(1)?
                .split_whitespace()
                .next()?
                .parse::<u8>()
                .ok()?;
            let type_str = line.split("TYPE:").nth(1)?.split_whitespace().next()?;
            let check_type = match type_str.to_lowercase().as_str() {
                "staletodos" | "stale_todos" => HeartbeatCheckType::StaleTodos,
                "stuckgoalruns" | "stuck_goal_runs" => HeartbeatCheckType::StuckGoalRuns,
                "unrepliedgatewaymessages" | "unreplied_gateway_messages" => {
                    HeartbeatCheckType::UnrepliedGatewayMessages
                }
                "repochanges" | "repo_changes" => HeartbeatCheckType::RepoChanges,
                _ => HeartbeatCheckType::StaleTodos, // fallback
            };
            let title = line
                .split("TITLE:")
                .nth(1)?
                .split("SUGGESTION:")
                .next()?
                .trim()
                .to_string();
            let suggestion = line.split("SUGGESTION:").nth(1)?.trim().to_string();
            Some(HeartbeatDigestItem {
                priority,
                check_type,
                title,
                suggestion,
            })
        })
        .collect()
}

/// Map a `HeartbeatCheckType` to its action_type string for audit entries.
fn check_type_to_action_type(check_type: &HeartbeatCheckType) -> &'static str {
    match check_type {
        HeartbeatCheckType::StaleTodos => "stale_todo",
        HeartbeatCheckType::StuckGoalRuns => "stuck_goal",
        HeartbeatCheckType::UnrepliedGatewayMessages => "unreplied_message",
        HeartbeatCheckType::RepoChanges => "repo_change",
        HeartbeatCheckType::SkillLifecycle => "skill_lifecycle",
    }
}

/// Build which built-in checks should run based on `HeartbeatChecksConfig` enabled flags.
///
/// Returns a `Vec<HeartbeatCheckType>` of enabled checks.
pub(super) fn enabled_checks(config: &HeartbeatChecksConfig) -> Vec<HeartbeatCheckType> {
    let mut checks = Vec::new();
    if config.stale_todos_enabled {
        checks.push(HeartbeatCheckType::StaleTodos);
    }
    if config.stuck_goals_enabled {
        checks.push(HeartbeatCheckType::StuckGoalRuns);
    }
    if config.unreplied_messages_enabled {
        checks.push(HeartbeatCheckType::UnrepliedGatewayMessages);
    }
    if config.repo_changes_enabled {
        checks.push(HeartbeatCheckType::RepoChanges);
    }
    checks
}

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
                    self.check_unreplied_messages(
                        checks_config.unreplied_message_threshold_hours,
                    )
                    .await,
                );
            }
            if checks_config.repo_changes_enabled
                && should_run_check(effective_repo_weight, cycle_count)
            {
                check_results.push(self.check_repo_changes().await);
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

        let anticipatory_summary = if anticipatory_items.is_empty() {
            String::new()
        } else {
            anticipatory_items
                .iter()
                .map(|item| {
                    let priority_hint = if item.kind == "hydration" {
                        "LOW-PRIORITY INFORMATIONAL"
                    } else {
                        "ACTIONABLE"
                    };
                    let bullets_text = if !item.bullets.is_empty() {
                        format!("\n    Bullets: {}", item.bullets.join(", "))
                    } else {
                        String::new()
                    };
                    format!(
                        "- [{}] ({}) {} (confidence: {:.2}): {}{}",
                        item.kind, priority_hint, item.title, item.confidence, item.summary,
                        bullets_text
                    )
                })
                .collect::<Vec<_>>()
                .join("\n")
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
        let (synthesis_json, actionable, digest_text, digest_items, llm_tokens) =
            match self.send_message(None, &synthesis_prompt).await {
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
                    (
                        None,
                        false,
                        format!("Synthesis failed: {e}"),
                        vec![],
                        0u64,
                    )
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
            Some(format!("Found {} items: {}", digest_items.len(), parts.join("; ")))
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

        // --- Phase 6b: Create audit entries for each actionable digest item ---
        if config.audit.scope.heartbeat {
            for (idx, item) in digest_items.iter().enumerate() {
                let action_type = check_type_to_action_type(&item.check_type);
                let item_data = serde_json::json!({
                    "title": item.title,
                    "suggestion": item.suggestion,
                    "priority": item.priority,
                    "check_type": format!("{:?}", item.check_type),
                });
                let summary = match generate_explanation(action_type, &item_data) {
                    ExplanationResult::Template(s) => s,
                    ExplanationResult::NeedsLlm => item.title.clone(),
                };
                let entry = AuditEntryRow {
                    id: format!("audit-hb-{}-{}", cycle_id, idx),
                    timestamp: now as i64,
                    action_type: action_type.to_string(),
                    summary: summary.clone(),
                    explanation: Some(summary.clone()),
                    confidence: None,
                    confidence_band: None,
                    causal_trace_id: None,
                    thread_id: None,
                    goal_run_id: None,
                    task_id: None,
                    raw_data_json: serde_json::to_string(&item).ok(),
                };
                if let Err(e) = self.history.insert_action_audit(&entry).await {
                    tracing::warn!(cycle_id = %cycle_id, idx = idx, "failed to insert heartbeat audit entry: {e}");
                }
                let _ = self.event_tx.send(AgentEvent::AuditAction {
                    id: entry.id.clone(),
                    timestamp: now,
                    action_type: entry.action_type.clone(),
                    summary: entry.summary.clone(),
                    explanation: entry.explanation.clone(),
                    confidence: None,
                    confidence_band: None,
                    causal_trace_id: None,
                    thread_id: None,
                });
            }

            // Piggyback audit cleanup on heartbeat cycle per Pitfall 4.
            match self
                .history
                .cleanup_action_audit(config.audit.max_entries, config.audit.max_age_days)
                .await
            {
                Ok(deleted) if deleted > 0 => {
                    tracing::info!(deleted = deleted, "audit trail cleanup removed old entries");
                }
                Err(e) => {
                    tracing::warn!("audit trail cleanup failed: {e}");
                }
                _ => {}
            }
        }

        // --- Phase 7: Update custom HeartbeatItem state ---
        {
            let mut items = self.heartbeat_items.write().await;
            for item in items.iter_mut() {
                if item.enabled {
                    item.last_run_at = Some(now);
                }
            }
        }
        self.persist_heartbeat().await;

        // --- Phase 8: Update EMA smoothed activity histogram (per D-02, BEAT-06) ---
        {
            let config_snap = self.config.read().await;
            let alpha = config_snap.ema_alpha;
            drop(config_snap);
            let mut model = self.operator_model.write().await;
            let new_smoothed = smooth_activity_histogram(
                &model.session_rhythm.smoothed_activity_histogram,
                &model.session_rhythm.activity_hour_histogram,
                alpha,
            );
            model.session_rhythm.smoothed_activity_histogram = new_smoothed;
        }

        // --- Phase 9: Weight update loop — learn what the operator cares about (D-04, BEAT-07) ---
        // Query feedback signals from the last 7 days.
        let seven_days_ago_ms = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64)
            - (7 * 24 * 3600 * 1000);

        let history = &self.history;
        let dismissals = history
            .count_dismissals_by_type(seven_days_ago_ms)
            .await
            .unwrap_or_default();
        let shown = history
            .count_shown_by_type(seven_days_ago_ms)
            .await
            .unwrap_or_default();
        let acted_on = history
            .count_acted_on_by_type(seven_days_ago_ms)
            .await
            .unwrap_or_default();

        // Compute updated priority weight for each built-in check type.
        let check_types = [
            (HeartbeatCheckType::StaleTodos, "stale_todo"),
            (HeartbeatCheckType::StuckGoalRuns, "stuck_goal"),
            (HeartbeatCheckType::UnrepliedGatewayMessages, "unreplied_message"),
            (HeartbeatCheckType::RepoChanges, "repo_change"),
        ];

        let decay_rate = 0.05; // 5% penalty per dismissal
        let recovery_rate = 0.1; // 10% recovery per acted-on entry

        let mut new_weights = HashMap::new();
        for (check_type, action_type_key) in &check_types {
            let dismiss_count = dismissals.get(*action_type_key).copied().unwrap_or(0);
            let total_shown = shown.get(*action_type_key).copied().unwrap_or(0);
            let recovery_count = acted_on.get(*action_type_key).copied().unwrap_or(0);
            // Inaction = shown but neither dismissed nor acted upon
            let inaction_count = total_shown
                .saturating_sub(dismiss_count)
                .saturating_sub(recovery_count);

            let weight = compute_check_priority(
                dismiss_count,
                inaction_count,
                total_shown,
                recovery_count,
                decay_rate,
                recovery_rate,
            );
            new_weights.insert(*check_type, weight);
        }

        // Store learned weights on AgentEngine (read by should_run_check gating above).
        {
            let mut weights = self.learned_check_weights.write().await;
            *weights = new_weights;
        }

        {
            let weights_snapshot = self.learned_check_weights.read().await.clone();
            tracing::debug!(
                weights = ?weights_snapshot,
                "updated learned check priority weights from feedback signals"
            );
        }

        // --- Phase 10: Memory consolidation (MEMO-01 through MEMO-08) ---
        // Per D-03: consolidation runs as a heartbeat sub-phase during idle periods.
        let consolidation_budget = std::time::Duration::from_secs(
            config.consolidation.budget_secs,
        );
        let consolidation_result = self
            .maybe_run_consolidation_if_idle(consolidation_budget)
            .await;
        if let Some(ref result) = consolidation_result {
            tracing::info!(
                traces = result.traces_reviewed,
                decayed = result.facts_decayed,
                tombstones = result.tombstones_purged,
                refined = result.facts_refined,
                "consolidation tick completed"
            );
        }

        // Recompute duration after consolidation phase so it includes Phase 10 time.
        let duration_ms = start.elapsed().as_millis() as i64;
        let consolidated = consolidation_result.is_some();
        tracing::info!(
            cycle_id = %cycle_id,
            actionable = actionable,
            checks = check_results.len(),
            consolidated = consolidated,
            duration_ms = duration_ms,
            "heartbeat cycle complete"
        );

        Ok(())
    }

    pub async fn get_heartbeat_items(&self) -> Vec<HeartbeatItem> {
        self.heartbeat_items.read().await.clone()
    }

    pub async fn set_heartbeat_items(&self, items: Vec<HeartbeatItem>) {
        *self.heartbeat_items.write().await = items;
        self.persist_heartbeat().await;
    }

    pub(super) async fn run_heartbeat(&self) -> Result<()> {
        let items = self.heartbeat_items.read().await.clone();
        let now = now_millis();

        for item in &items {
            if !item.enabled {
                continue;
            }

            let interval_ms = if item.interval_minutes > 0 {
                item.interval_minutes * 60 * 1000
            } else {
                self.config.read().await.heartbeat_interval_mins * 60 * 1000
            };

            let due = match item.last_run_at {
                Some(last) => now - last >= interval_ms,
                None => true,
            };

            if !due {
                continue;
            }

            let prompt = format!(
                "Heartbeat check: {}\n\n\
                 Respond with HEARTBEAT_OK if everything is normal, \
                 or HEARTBEAT_ALERT: <explanation> if something needs attention.",
                item.prompt
            );

            let result = match self.send_message(None, &prompt).await {
                Ok(thread_id) => {
                    // Check the last assistant message for OK/ALERT
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

                    if response.contains("HEARTBEAT_OK") {
                        (HeartbeatOutcome::Ok, "OK".into())
                    } else if response.contains("HEARTBEAT_ALERT") {
                        (HeartbeatOutcome::Alert, response)
                    } else {
                        (HeartbeatOutcome::Ok, response)
                    }
                }
                Err(e) => (HeartbeatOutcome::Error, format!("Error: {e}")),
            };

            let _ = self.event_tx.send(AgentEvent::HeartbeatResult {
                item_id: item.id.clone(),
                result: result.0,
                message: result.1.clone(),
            });

            // Update item state
            {
                let mut items = self.heartbeat_items.write().await;
                if let Some(i) = items.iter_mut().find(|i| i.id == item.id) {
                    i.last_run_at = Some(now);
                    i.last_result = Some(result.0);
                    i.last_message = Some(result.1);
                }
            }

            // If alert and notify enabled, send notification
            if result.0 == HeartbeatOutcome::Alert && item.notify_on_alert {
                let _ = self.event_tx.send(AgentEvent::Notification {
                    title: format!("Heartbeat Alert: {}", item.label),
                    body: item.last_message.clone().unwrap_or_default(),
                    severity: NotificationSeverity::Alert,
                    channels: item.notify_channels.clone(),
                });
            }
        }

        self.persist_heartbeat().await;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Activity-aware scheduling pure functions (BEAT-06/D-01/D-03)
// ---------------------------------------------------------------------------

/// Check if a given UTC hour is a peak activity hour.
///
/// Returns `true` if the hour is in the explicit `peak_hours` list OR if the
/// EMA-smoothed activity count for that hour meets/exceeds `ema_threshold`.
pub(super) fn is_peak_activity_hour(
    current_hour_utc: u8,
    peak_hours: &[u8],
    smoothed_histogram: &HashMap<u8, f64>,
    ema_threshold: f64,
) -> bool {
    peak_hours.contains(&current_hour_utc)
        || smoothed_histogram
            .get(&current_hour_utc)
            .map(|&count| count >= ema_threshold)
            .unwrap_or(false)
}

/// Decide whether a check should run this cycle based on its priority weight.
///
/// Weight 1.0 = every cycle, 0.5 = every 2nd, 0.25 = every 4th. Weight 0.0
/// means never run. Per D-05: checks are never fully disabled (caller ensures
/// weight stays >= 0.1), but this function respects 0.0 for completeness.
pub(super) fn should_run_check(weight: f64, cycle_count: u64) -> bool {
    if weight >= 1.0 {
        return true;
    }
    if weight <= 0.0 {
        return false;
    }
    let skip_factor = (1.0 / weight).round() as u64;
    if skip_factor == 0 {
        return true;
    }
    cycle_count % skip_factor == 0
}

/// Compute a check's learned priority weight from user feedback signals.
///
/// Returns a value in `[0.1, 1.0]` (per D-05: never fully disables).
///
/// - `dismiss_count`: times user dismissed this check type
/// - `inaction_count`: times user never acted on this check type
/// - `total_shown`: total times this check was shown
/// - `recovery_count`: times user acted after a period of ignoring
/// - `decay_rate`: penalty per dismissal (suggest 0.1)
/// - `recovery_rate`: bonus per recovery action (suggest 0.1)
pub(super) fn compute_check_priority(
    dismiss_count: u64,
    inaction_count: u64,
    total_shown: u64,
    recovery_count: u64,
    decay_rate: f64,
    recovery_rate: f64,
) -> f64 {
    let dismiss_penalty = (dismiss_count as f64 * decay_rate).min(0.6);
    let inaction_penalty = if total_shown > 0 {
        let inaction_rate = inaction_count as f64 / total_shown as f64;
        (inaction_rate * 0.4).min(0.3)
    } else {
        0.0
    };
    let recovery_bonus = (recovery_count as f64 * recovery_rate).min(0.5);
    let raw = 1.0 - dismiss_penalty - inaction_penalty + recovery_bonus;
    raw.clamp(0.1, 1.0) // Never fully disable per D-05
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── check_quiet_window pure function tests ─────────────────────────

    #[test]
    fn quiet_hours_within_midnight_wrap_window() {
        // start=22, end=6, hour=23 → quiet
        assert!(check_quiet_window(23, Some(22), Some(6), false));
    }

    #[test]
    fn quiet_hours_outside_midnight_wrap_window() {
        // start=22, end=6, hour=12 → not quiet
        assert!(!check_quiet_window(12, Some(22), Some(6), false));
    }

    #[test]
    fn quiet_hours_midnight_wrap_early_morning() {
        // start=22, end=6, hour=3 → quiet (early morning within wrap)
        assert!(check_quiet_window(3, Some(22), Some(6), false));
    }

    #[test]
    fn quiet_hours_midnight_wrap_boundary_end() {
        // start=22, end=6, hour=6 → NOT quiet (end hour is exclusive)
        assert!(!check_quiet_window(6, Some(22), Some(6), false));
    }

    #[test]
    fn quiet_hours_midnight_wrap_boundary_start() {
        // start=22, end=6, hour=22 → quiet (start hour is inclusive)
        assert!(check_quiet_window(22, Some(22), Some(6), false));
    }

    #[test]
    fn dnd_enabled_overrides_everything() {
        // dnd=true → always quiet regardless of hour or window config
        assert!(check_quiet_window(12, None, None, true));
        assert!(check_quiet_window(12, Some(22), Some(6), true));
        assert!(check_quiet_window(0, Some(9), Some(17), true));
    }

    #[test]
    fn no_quiet_hours_configured_and_no_dnd() {
        // No quiet hours, no DND → never quiet
        assert!(!check_quiet_window(12, None, None, false));
        assert!(!check_quiet_window(0, None, None, false));
        assert!(!check_quiet_window(23, None, None, false));
    }

    #[test]
    fn same_day_range_inside() {
        // start=9, end=17, hour=12 → quiet
        assert!(check_quiet_window(12, Some(9), Some(17), false));
    }

    #[test]
    fn same_day_range_outside() {
        // start=9, end=17, hour=20 → not quiet
        assert!(!check_quiet_window(20, Some(9), Some(17), false));
    }

    #[test]
    fn partial_config_only_start_set() {
        // Only start set (no end) → not quiet
        assert!(!check_quiet_window(23, Some(22), None, false));
    }

    #[test]
    fn partial_config_only_end_set() {
        // Only end set (no start) → not quiet
        assert!(!check_quiet_window(3, None, Some(6), false));
    }

    // ── resolve_cron_from_config tests ─────────────────────────────────

    #[test]
    fn resolve_cron_prefers_explicit_cron() {
        let config = AgentConfig {
            heartbeat_cron: Some("0 * * * *".to_string()),
            heartbeat_interval_mins: 15,
            ..AgentConfig::default()
        };
        assert_eq!(resolve_cron_from_config(&config), "0 * * * *");
    }

    #[test]
    fn resolve_cron_falls_back_to_interval_mins() {
        let config = AgentConfig {
            heartbeat_cron: None,
            heartbeat_interval_mins: 15,
            ..AgentConfig::default()
        };
        assert_eq!(resolve_cron_from_config(&config), "*/15 * * * *");
    }

    #[test]
    fn resolve_cron_with_hourly_interval() {
        let config = AgentConfig {
            heartbeat_cron: None,
            heartbeat_interval_mins: 60,
            ..AgentConfig::default()
        };
        assert_eq!(resolve_cron_from_config(&config), "0 * * * *");
    }

    #[test]
    fn resolve_cron_explicit_overrides_interval() {
        let config = AgentConfig {
            heartbeat_cron: Some("30 2 * * *".to_string()),
            heartbeat_interval_mins: 60,
            ..AgentConfig::default()
        };
        assert_eq!(resolve_cron_from_config(&config), "30 2 * * *");
    }

    // ── should_broadcast tests (D-14: silent default) ───────────────────

    #[test]
    fn broadcast_when_actionable_true_and_items_present() {
        let items = vec![HeartbeatDigestItem {
            priority: 1,
            check_type: HeartbeatCheckType::StaleTodos,
            title: "Stale todo".into(),
            suggestion: "Review it".into(),
        }];
        assert!(should_broadcast(true, &items));
    }

    #[test]
    fn broadcast_when_actionable_true_but_no_items() {
        assert!(should_broadcast(true, &[]));
    }

    #[test]
    fn broadcast_when_not_actionable_but_items_present() {
        let items = vec![HeartbeatDigestItem {
            priority: 3,
            check_type: HeartbeatCheckType::RepoChanges,
            title: "Repo change".into(),
            suggestion: "Check it".into(),
        }];
        assert!(should_broadcast(false, &items));
    }

    #[test]
    fn no_broadcast_when_not_actionable_and_no_items() {
        // D-14: silent default — no event broadcast
        assert!(!should_broadcast(false, &[]));
    }

    // ── heartbeat_persistence_status tests (Pitfall 4) ──────────────────

    #[test]
    fn persistence_status_completed_when_synthesis_present() {
        assert_eq!(
            heartbeat_persistence_status(Some("LLM response text")),
            "completed"
        );
    }

    #[test]
    fn persistence_status_failed_when_synthesis_none() {
        assert_eq!(heartbeat_persistence_status(None), "synthesis_failed");
    }

    // ── is_custom_item_due tests ────────────────────────────────────────

    #[test]
    fn custom_item_due_when_never_run() {
        // last_run_at=None → always due
        assert!(is_custom_item_due(100_000_000, None, 15, 30));
    }

    #[test]
    fn custom_item_due_when_interval_elapsed() {
        let now = 100_000_000;
        let last = now - (16 * 60 * 1000); // 16 minutes ago
        // item_interval=15min → 15*60*1000=900_000 < 960_000 elapsed → due
        assert!(is_custom_item_due(now, Some(last), 15, 30));
    }

    #[test]
    fn custom_item_not_due_when_interval_not_elapsed() {
        let now = 100_000_000;
        let last = now - (10 * 60 * 1000); // 10 minutes ago
        // item_interval=15min → not enough time elapsed → not due
        assert!(!is_custom_item_due(now, Some(last), 15, 30));
    }

    #[test]
    fn custom_item_uses_global_interval_when_item_interval_zero() {
        let now = 100_000_000;
        let last = now - (31 * 60 * 1000); // 31 minutes ago
        // item_interval=0, global=30min → 30*60*1000=1_800_000 < 1_860_000 elapsed → due
        assert!(is_custom_item_due(now, Some(last), 0, 30));
    }

    #[test]
    fn custom_item_not_due_with_global_interval() {
        let now = 100_000_000;
        let last = now - (20 * 60 * 1000); // 20 minutes ago
        // item_interval=0, global=30min → not enough time elapsed → not due
        assert!(!is_custom_item_due(now, Some(last), 0, 30));
    }

    // ── enabled_checks tests (check gating by config) ───────────────────

    #[test]
    fn all_checks_enabled_by_default() {
        let config = HeartbeatChecksConfig::default();
        let checks = enabled_checks(&config);
        assert_eq!(checks.len(), 4);
        assert!(checks.contains(&HeartbeatCheckType::StaleTodos));
        assert!(checks.contains(&HeartbeatCheckType::StuckGoalRuns));
        assert!(checks.contains(&HeartbeatCheckType::UnrepliedGatewayMessages));
        assert!(checks.contains(&HeartbeatCheckType::RepoChanges));
    }

    #[test]
    fn only_enabled_checks_are_included() {
        let config = HeartbeatChecksConfig {
            stale_todos_enabled: true,
            stuck_goals_enabled: false,
            unreplied_messages_enabled: false,
            repo_changes_enabled: true,
            ..HeartbeatChecksConfig::default()
        };
        let checks = enabled_checks(&config);
        assert_eq!(checks.len(), 2);
        assert!(checks.contains(&HeartbeatCheckType::StaleTodos));
        assert!(checks.contains(&HeartbeatCheckType::RepoChanges));
        assert!(!checks.contains(&HeartbeatCheckType::StuckGoalRuns));
        assert!(!checks.contains(&HeartbeatCheckType::UnrepliedGatewayMessages));
    }

    #[test]
    fn no_checks_when_all_disabled() {
        let config = HeartbeatChecksConfig {
            stale_todos_enabled: false,
            stuck_goals_enabled: false,
            unreplied_messages_enabled: false,
            repo_changes_enabled: false,
            ..HeartbeatChecksConfig::default()
        };
        let checks = enabled_checks(&config);
        assert!(checks.is_empty());
    }

    // ── parse_digest_items tests ────────────────────────────────────────

    #[test]
    fn parse_digest_items_from_valid_response() {
        let response = "\
ACTIONABLE: true
DIGEST: 2 items need attention
ITEMS:
- PRIORITY:1 TYPE:stale_todos TITLE:Stale todo found SUGGESTION:Review pending items
- PRIORITY:3 TYPE:repo_changes TITLE:Uncommitted changes SUGGESTION:Commit or stash";

        let items = parse_digest_items(response);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].priority, 1);
        assert_eq!(items[0].check_type, HeartbeatCheckType::StaleTodos);
        assert_eq!(items[0].title, "Stale todo found");
        assert_eq!(items[0].suggestion, "Review pending items");
        assert_eq!(items[1].priority, 3);
        assert_eq!(items[1].check_type, HeartbeatCheckType::RepoChanges);
    }

    #[test]
    fn parse_digest_items_empty_when_no_items_section() {
        let response = "ACTIONABLE: false\nDIGEST: All systems normal.";
        let items = parse_digest_items(response);
        assert!(items.is_empty());
    }

    #[test]
    fn parse_digest_items_handles_camelcase_types() {
        let response =
            "- PRIORITY:2 TYPE:StuckGoalRuns TITLE:Goal stuck SUGGESTION:Cancel it";
        let items = parse_digest_items(response);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].check_type, HeartbeatCheckType::StuckGoalRuns);
    }

    // ── is_peak_activity_hour tests (BEAT-06/D-01) ──────────────────────

    #[test]
    fn peak_activity_hour_in_peak_hours_list() {
        let smoothed: HashMap<u8, f64> = HashMap::new();
        assert!(is_peak_activity_hour(9, &[9, 10, 14], &smoothed, 2.0));
    }

    #[test]
    fn peak_activity_hour_above_ema_threshold() {
        let mut smoothed: HashMap<u8, f64> = HashMap::new();
        smoothed.insert(15, 5.0);
        assert!(is_peak_activity_hour(15, &[], &smoothed, 2.0));
    }

    #[test]
    fn peak_activity_hour_below_threshold_and_not_in_list() {
        let mut smoothed: HashMap<u8, f64> = HashMap::new();
        smoothed.insert(3, 1.0);
        assert!(!is_peak_activity_hour(3, &[9, 10], &smoothed, 2.0));
    }

    // ── should_run_check tests (BEAT-06/D-05) ──────────────────────────

    #[test]
    fn should_run_check_weight_one_always_runs() {
        assert!(should_run_check(1.0, 0));
        assert!(should_run_check(1.0, 1));
        assert!(should_run_check(1.0, 99));
    }

    #[test]
    fn should_run_check_weight_quarter_every_fourth_cycle() {
        assert!(should_run_check(0.25, 4));  // 4 % 4 == 0
        assert!(should_run_check(0.25, 8));  // 8 % 4 == 0
        assert!(should_run_check(0.25, 0));  // 0 % 4 == 0
    }

    #[test]
    fn should_run_check_weight_quarter_skips_other_cycles() {
        assert!(!should_run_check(0.25, 1)); // 1 % 4 != 0
        assert!(!should_run_check(0.25, 3)); // 3 % 4 != 0
    }

    #[test]
    fn should_run_check_weight_zero_never_runs() {
        assert!(!should_run_check(0.0, 0));
        assert!(!should_run_check(0.0, 1));
        assert!(!should_run_check(0.0, 100));
    }

    // ── compute_check_priority tests (BEAT-09/D-04/D-05) ───────────────

    #[test]
    fn compute_check_priority_zero_dismissals_returns_one() {
        let result = compute_check_priority(0, 0, 0, 0, 0.1, 0.1);
        assert!((result - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn compute_check_priority_many_dismissals_clamped_minimum() {
        // 100 dismissals * 0.1 decay = 10.0 penalty, capped at 0.6
        // With 0 inaction, 0 recovery: 1.0 - 0.6 = 0.4
        // But also test with very high dismissals to hit 0.1 floor
        let result = compute_check_priority(100, 100, 100, 0, 0.1, 0.1);
        assert!((result - 0.1).abs() < f64::EPSILON);
    }

    #[test]
    fn compute_check_priority_recovery_partially_restores() {
        // 5 dismissals * 0.1 = 0.5 penalty
        // 0 inaction: no penalty
        // 3 recovery * 0.1 = 0.3 bonus
        // 1.0 - 0.5 + 0.3 = 0.8
        let result = compute_check_priority(5, 0, 0, 3, 0.1, 0.1);
        assert!((result - 0.8).abs() < f64::EPSILON);
    }

    #[test]
    fn priority_floor_never_below_point_one() {
        // Extreme dismissals and inaction with no recovery
        let result = compute_check_priority(1000, 1000, 1000, 0, 1.0, 0.0);
        assert!((result - 0.1).abs() < f64::EPSILON);
    }
}
