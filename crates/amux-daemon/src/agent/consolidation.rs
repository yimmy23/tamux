//! Idle-time memory consolidation -- trace review, fact aging, heuristic promotion.

use super::*;

/// Default memory fact decay half-life in hours (~69h, per D-04).
const DEFAULT_HALF_LIFE_HOURS: f64 = 69.0;

/// Default idle threshold in milliseconds (5 minutes, per D-01).
#[allow(dead_code)]
const DEFAULT_IDLE_THRESHOLD_MS: u64 = 5 * 60 * 1000;

/// Default consolidation tick budget in seconds (per D-02).
#[allow(dead_code)]
const DEFAULT_BUDGET_SECS: u64 = 30;

// ---------------------------------------------------------------------------
// Pure functions
// ---------------------------------------------------------------------------

/// Check all four idle conditions required for consolidation per D-01.
/// Returns true only when ALL conditions are simultaneously met:
/// no active tasks, no active goal runs, no active streams, and operator idle > threshold.
pub(super) fn is_idle_for_consolidation(
    active_task_count: usize,
    running_goal_count: usize,
    active_stream_count: usize,
    last_presence_at: Option<u64>,
    now: u64,
    idle_threshold_ms: u64,
) -> bool {
    if active_task_count > 0 || running_goal_count > 0 || active_stream_count > 0 {
        return false;
    }
    match last_presence_at {
        Some(last) => now.saturating_sub(last) >= idle_threshold_ms,
        None => true,
    }
}

/// Compute current confidence for a memory fact based on exponential decay.
/// Returns a value in 0.0..=1.0. A fact confirmed exactly `half_life_hours` ago
/// will have confidence ~0.5. Active facts (recently confirmed) stay near 1.0.
pub(super) fn compute_decay_confidence(
    last_confirmed_at: u64,
    now: u64,
    half_life_hours: f64,
) -> f64 {
    if last_confirmed_at == 0 || half_life_hours <= 0.0 {
        return 0.0;
    }
    let age_ms = now.saturating_sub(last_confirmed_at) as f64;
    let age_hours = age_ms / 3_600_000.0;
    let lambda = 2.0_f64.ln() / half_life_hours;
    let confidence = (-lambda * age_hours).exp();
    confidence.clamp(0.0, 1.0)
}

// ---------------------------------------------------------------------------
// Consolidation entry point and sub-tasks
// ---------------------------------------------------------------------------

impl AgentEngine {
    /// Run a consolidation tick if idle conditions are met. Per D-03, this is called
    /// from within run_structured_heartbeat_adaptive() as a sub-phase.
    pub(super) async fn maybe_run_consolidation_if_idle(
        &self,
        budget: std::time::Duration,
    ) -> Option<ConsolidationResult> {
        let config = self.config.read().await.clone();
        if !config.consolidation.enabled {
            return None;
        }

        // Check idle conditions per D-01
        let active_tasks = self
            .tasks
            .lock()
            .await
            .iter()
            .filter(|t| matches!(t.status, TaskStatus::InProgress))
            .count();
        let running_goals = self
            .goal_runs
            .lock()
            .await
            .iter()
            .filter(|g| matches!(g.status, GoalRunStatus::Running | GoalRunStatus::Planning))
            .count();
        let active_streams = self.stream_cancellations.lock().await.len();
        let last_presence = self.anticipatory.read().await.last_presence_at;
        let now = now_millis();
        let idle_threshold_ms = config.consolidation.idle_threshold_secs * 1000;

        if !is_idle_for_consolidation(
            active_tasks,
            running_goals,
            active_streams,
            last_presence,
            now,
            idle_threshold_ms,
        ) {
            return None;
        }

        tracing::info!("idle conditions met -- starting consolidation tick");

        let deadline = std::time::Instant::now() + budget;
        let mut result = ConsolidationResult::default();

        // Sub-task 1: Review execution traces -> promote heuristics (MEMO-01, MEMO-06)
        if std::time::Instant::now() < deadline {
            result.traces_reviewed = self.review_execution_traces(&config, &deadline).await;
        }

        // Sub-task 2: Decay stale memory facts and tombstone low-confidence ones (MEMO-02)
        if std::time::Instant::now() < deadline {
            result.facts_decayed = self.apply_fact_decay(&config, &deadline).await;
        }

        // Sub-task 3: Cleanup expired tombstones - 7-day TTL (MEMO-05)
        if std::time::Instant::now() < deadline {
            result.tombstones_purged = self.cleanup_expired_tombstones(&config).await;
        }

        // Sub-task 4: Proactive memory refinement (LLM call, most expensive -- runs LAST)
        if std::time::Instant::now() < deadline {
            result.facts_refined = self.refine_memory_facts(&deadline).await;
        }

        // Sub-task 5: Flag skill draft candidates (SKIL-01, SKIL-02, per D-02)
        if std::time::Instant::now() < deadline {
            result.skill_candidates_flagged =
                self.flag_skill_draft_candidates(&config, &deadline).await;
        }

        // Sub-task 6: Draft flagged candidates into SKILL.md (SKIL-01, per D-03)
        if std::time::Instant::now() < deadline {
            result.skills_drafted =
                self.draft_flagged_skill_candidates(&config, &deadline).await;
        }

        // Sub-task 7: Run mental tests on Draft skills (SKIL-04, per D-05)
        if std::time::Instant::now() < deadline {
            result.skills_tested = self.run_skill_mental_tests(&config, &deadline).await;
        }

        // Sub-task 8: Check lifecycle promotions (SKIL-05, per D-06)
        if std::time::Instant::now() < deadline {
            result.skills_promoted = self.check_skill_promotions(&config, &deadline).await;
        }

        // Persist learning stores after consolidation updates (D-10)
        if result.traces_reviewed > 0 {
            self.persist_learning_stores().await;
        }

        // Log provenance for the consolidation tick (MEMO-04)
        self.record_provenance_event(
            "memory_consolidation",
            &format!(
                "Consolidation tick: {} traces reviewed, {} facts decayed, {} tombstones purged, {} facts refined, {} skill candidates flagged, {} skills drafted, {} skills tested, {} skills promoted",
                result.traces_reviewed, result.facts_decayed, result.tombstones_purged, result.facts_refined,
                result.skill_candidates_flagged, result.skills_drafted, result.skills_tested, result.skills_promoted
            ),
            serde_json::json!({
                "traces_reviewed": result.traces_reviewed,
                "facts_decayed": result.facts_decayed,
                "tombstones_purged": result.tombstones_purged,
                "facts_refined": result.facts_refined,
                "skill_candidates_flagged": result.skill_candidates_flagged,
                "skills_drafted": result.skills_drafted,
                "skills_tested": result.skills_tested,
                "skills_promoted": result.skills_promoted,
            }),
            None,
            None,
            None,
            None,
            None,
        )
        .await;

        Some(result)
    }

    /// Review recent execution traces and promote successful tool sequences to
    /// heuristics when they cross the promotion threshold (MEMO-01, MEMO-06).
    ///
    /// Uses a watermark to avoid re-reviewing traces across restarts (Pitfall 5).
    async fn review_execution_traces(
        &self,
        config: &AgentConfig,
        deadline: &std::time::Instant,
    ) -> usize {
        // Get watermark from consolidation state
        let watermark: u64 = match self
            .history
            .get_consolidation_state("trace_review_watermark")
            .await
        {
            Ok(Some(val)) => val.parse().unwrap_or(0),
            _ => 0,
        };

        // Fetch a batch of successful traces since the watermark
        let traces = match self
            .history
            .list_recent_successful_traces(watermark, 50)
            .await
        {
            Ok(t) => t,
            Err(e) => {
                tracing::warn!(error = %e, "failed to list recent traces for consolidation");
                return 0;
            }
        };

        if traces.is_empty() {
            return 0;
        }

        let promotion_threshold = config.consolidation.heuristic_promotion_threshold;
        let now = now_millis();
        let mut reviewed = 0;
        let mut last_created_at: u64 = watermark;

        for trace in &traces {
            // Budget check at the start of each iteration
            if std::time::Instant::now() >= *deadline {
                break;
            }

            // Extract tool sequence from tool_sequence_json
            let tool_names: Vec<String> = trace
                .tool_sequence_json
                .as_deref()
                .and_then(|json| serde_json::from_str::<Vec<String>>(json).ok())
                .unwrap_or_default();

            if tool_names.is_empty() {
                reviewed += 1;
                last_created_at = last_created_at.max(trace.created_at as u64);
                continue;
            }

            let task_type = trace
                .task_type
                .as_deref()
                .unwrap_or("unknown");

            let duration_ms = trace.duration_ms.unwrap_or(0) as u64;

            // Record in PatternStore
            {
                let mut patterns = self.pattern_store.write().await;
                patterns.record_sequence(&tool_names, task_type, true, now);
            }

            // Check if the pattern crosses the promotion threshold
            let should_promote = {
                let patterns = self.pattern_store.read().await;
                let matching = patterns.find_patterns(
                    task_type,
                    super::learning::patterns::PatternType::SuccessSequence,
                );
                matching
                    .iter()
                    .any(|p| p.tool_sequence == tool_names && p.occurrences >= promotion_threshold)
            };

            if should_promote {
                // Promote each tool in the sequence to HeuristicStore
                {
                    let mut heuristics = self.heuristic_store.write().await;
                    let avg_duration = duration_ms / tool_names.len().max(1) as u64;
                    for tool_name in &tool_names {
                        heuristics.update_tool(tool_name, task_type, true, avg_duration);
                    }
                }

                // Record provenance for the promotion
                self.record_provenance_event(
                    "heuristic_promotion",
                    &format!(
                        "Promoted tool sequence {:?} for task type '{}' (>= {} occurrences)",
                        tool_names, task_type, promotion_threshold
                    ),
                    serde_json::json!({
                        "tool_sequence": tool_names,
                        "task_type": task_type,
                        "threshold": promotion_threshold,
                        "trace_id": trace.id,
                    }),
                    trace.goal_run_id.as_deref(),
                    trace.task_id.as_deref(),
                    None,
                    None,
                    None,
                )
                .await;
            }

            reviewed += 1;
            last_created_at = last_created_at.max(trace.created_at as u64);
        }

        // Update watermark for next run (Pitfall 5: watermark-based pagination)
        if last_created_at > watermark {
            if let Err(e) = self
                .history
                .set_consolidation_state(
                    "trace_review_watermark",
                    &last_created_at.to_string(),
                    now,
                )
                .await
            {
                tracing::warn!(error = %e, "failed to update trace review watermark");
            }
        }

        reviewed
    }

    /// Decay stale memory facts and tombstone those below the configurable threshold
    /// (MEMO-02). Facts with confidence below `fact_decay_supersede_threshold` are
    /// actually tombstoned via `supersede_memory_fact`, not just counted.
    async fn apply_fact_decay(
        &self,
        config: &AgentConfig,
        deadline: &std::time::Instant,
    ) -> usize {
        let half_life = config.consolidation.memory_decay_half_life_hours;
        let threshold = config.consolidation.fact_decay_supersede_threshold;
        let now = now_millis();

        // Read MEMORY.md content
        let memory_path = active_memory_dir(&self.data_dir)
            .join(MemoryTarget::Memory.file_name());
        let content = match tokio::fs::read_to_string(&memory_path).await {
            Ok(c) => c,
            Err(_) => return 0,
        };

        // Extract fact candidates from MEMORY.md
        let facts = extract_memory_fact_candidates(&content);
        if facts.is_empty() {
            return 0;
        }

        // For each fact, look up its last provenance timestamp to approximate
        // last_confirmed_at. Fall back to using the current time minus 30 days
        // if no provenance record exists (treating it as very old).
        let mut decayed_count = 0;

        for fact in &facts {
            if std::time::Instant::now() >= *deadline {
                break;
            }

            // Skip facts that are already superseded
            if fact.display.contains("[SUPERSEDED]") {
                continue;
            }

            // Query provenance for this fact key to get last_confirmed_at
            let last_confirmed_at = match self
                .history
                .memory_provenance_report(Some(MemoryTarget::Memory.label()), 50)
                .await
            {
                Ok(report) => {
                    // Find the most recent provenance entry mentioning this fact key
                    report
                        .entries
                        .iter()
                        .find(|e| e.fact_keys.contains(&fact.key))
                        .map(|e| e.created_at as u64)
                        .unwrap_or(0)
                }
                Err(_) => 0,
            };

            // If no provenance record, skip (we cannot compute meaningful decay)
            if last_confirmed_at == 0 {
                continue;
            }

            let confidence = compute_decay_confidence(last_confirmed_at, now, half_life);

            if confidence < threshold {
                // Actually tombstone the fact via supersede_memory_fact (MEMO-02)
                if let Err(e) = self
                    .supersede_memory_fact(
                        MemoryTarget::Memory,
                        &fact.display,
                        &fact.key,
                        "", // empty replacement -- fact is simply removed from active memory
                        "fact_decay",
                    )
                    .await
                {
                    tracing::warn!(
                        fact_key = %fact.key,
                        error = %e,
                        "failed to tombstone decayed fact"
                    );
                    continue;
                }

                // Record provenance for the decay action (MEMO-04)
                self.record_provenance_event(
                    "fact_decay",
                    &format!(
                        "Decayed fact '{}' (confidence {:.2} < threshold {:.2})",
                        fact.key, confidence, threshold
                    ),
                    serde_json::json!({
                        "fact_key": fact.key,
                        "confidence": confidence,
                        "threshold": threshold,
                        "half_life_hours": half_life,
                    }),
                    None,
                    None,
                    None,
                    None,
                    None,
                )
                .await;

                decayed_count += 1;
            }
        }

        decayed_count
    }

    /// Cleanup expired tombstones older than the configured TTL (MEMO-05).
    async fn cleanup_expired_tombstones(&self, config: &AgentConfig) -> usize {
        let max_age_ms = config.consolidation.tombstone_ttl_days * 24 * 60 * 60 * 1000;
        let now = now_millis();

        match self.history.delete_expired_tombstones(max_age_ms, now).await {
            Ok(count) => {
                if count > 0 {
                    tracing::debug!(
                        purged = count,
                        ttl_days = config.consolidation.tombstone_ttl_days,
                        "purged expired memory tombstones"
                    );
                }
                count
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to delete expired tombstones");
                0
            }
        }
    }

    /// Proactive memory refinement: detect redundant/contradictory facts and merge via LLM.
    /// Per D-12: budget within the 30-second tick. Runs LAST (most expensive sub-task).
    /// Per Pitfall 3: check circuit breaker before LLM call. Skip if open.
    async fn refine_memory_facts(&self, deadline: &std::time::Instant) -> usize {
        // 1. Check remaining budget -- need at least 10 seconds for an LLM call
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.as_secs() < 10 {
            tracing::debug!("skipping memory refinement -- insufficient budget");
            return 0;
        }

        // 2. Check circuit breaker for configured provider
        let config = self.config.read().await.clone();
        if let Err(_e) = self.check_circuit_breaker(&config.provider).await {
            tracing::debug!(
                provider = %config.provider,
                "skipping memory refinement -- circuit breaker open"
            );
            return 0;
        }

        // 3. Read MEMORY.md content
        let memory_path = active_memory_dir(&self.data_dir)
            .join(MemoryTarget::Memory.file_name());
        let content = match tokio::fs::read_to_string(&memory_path).await {
            Ok(c) => c,
            Err(_) => return 0,
        };

        // 4. Extract fact candidates and find contradictions/redundancies
        let candidates = extract_memory_fact_candidates(&content);
        if candidates.len() < 2 {
            return 0; // Need at least 2 facts to find contradictions
        }

        // Find facts with overlapping keys (potential contradictions/redundancies)
        let mut key_groups: std::collections::HashMap<String, Vec<&MemoryFactCandidate>> =
            std::collections::HashMap::new();
        for candidate in &candidates {
            let normalized_key = candidate.key.to_lowercase().trim().to_string();
            key_groups
                .entry(normalized_key)
                .or_default()
                .push(candidate);
        }

        let conflicting: Vec<_> = key_groups.values().filter(|group| group.len() > 1).collect();

        if conflicting.is_empty() {
            return 0; // No contradictions found
        }

        // 5. Build a short LLM prompt to merge the first conflict group
        // Only handle one conflict per tick to stay within budget
        let conflict = &conflicting[0];
        let facts_text: String = conflict
            .iter()
            .map(|f| format!("- {}: {}", f.key, f.display))
            .collect::<Vec<_>>()
            .join("\n");

        let refinement_prompt = format!(
            "You are a memory consolidation agent. These memory facts appear to be about the same topic but may conflict or be redundant.\n\n\
             Facts:\n{}\n\n\
             Merge these into a single, accurate fact. Return ONLY the merged fact as a single line. \
             If they truly conflict, keep the most recent/specific one. If redundant, combine into one concise fact.",
            facts_text
        );

        // 6. Make LLM call with timeout -- following the pattern from memory_flush.rs
        let llm_timeout = remaining.saturating_sub(std::time::Duration::from_secs(2));

        let merged_content = match tokio::time::timeout(
            llm_timeout,
            self.send_refinement_llm_call(&config, &refinement_prompt),
        )
        .await
        {
            Ok(Ok(response)) => response.trim().to_string(),
            Ok(Err(e)) => {
                tracing::warn!(error = %e, "memory refinement LLM call failed");
                return 0;
            }
            Err(_) => {
                tracing::debug!("memory refinement LLM call timed out within budget");
                return 0;
            }
        };

        if merged_content.is_empty() {
            return 0;
        }

        // 7. Supersede the original facts with the merged result
        // Per Pitfall 2: tombstone-before-update via supersede_memory_fact
        let original_content = conflict
            .iter()
            .map(|f| f.display.clone())
            .collect::<Vec<_>>()
            .join("\n");

        match self
            .supersede_memory_fact(
                MemoryTarget::Memory,
                &original_content,
                &conflict[0].key,
                &merged_content,
                "consolidation_refinement",
            )
            .await
        {
            Ok(()) => {
                tracing::info!(key = %conflict[0].key, "refined memory fact via LLM merge");
                1
            }
            Err(e) => {
                tracing::warn!(error = %e, "failed to supersede memory fact during refinement");
                0
            }
        }
    }

    /// Minimal LLM call for memory refinement. Uses the operator's configured provider/model.
    /// Short context, focused prompt -- minimal token cost.
    ///
    /// Pattern follows memory_flush.rs:
    /// 1. Build ApiMessage vec (system + user)
    /// 2. Get ProviderConfig for the configured provider
    /// 3. Call send_completion_request with empty tools, Chat transport
    /// 4. Collect text content from Delta/Done chunks in the stream
    /// 5. Return concatenated response text
    pub(super) async fn send_refinement_llm_call(
        &self,
        config: &AgentConfig,
        user_prompt: &str,
    ) -> anyhow::Result<String> {
        use futures::StreamExt;

        let provider_config = resolve_active_provider_config(config)?;

        let system =
            "You are a concise memory consolidation agent. Respond with ONLY the merged fact, nothing else.";

        let messages = vec![ApiMessage {
            role: "user".to_string(),
            content: ApiContent::Text(user_prompt.to_string()),
            tool_call_id: None,
            name: None,
            tool_calls: None,
        }];

        let mut stream = send_completion_request(
            &self.http_client,
            &config.provider,
            &provider_config,
            system,
            &messages,
            &[], // No tools needed for refinement
            provider_config.api_transport,
            None, // No previous_response_id
            None, // No upstream_thread_id
            RetryStrategy::Bounded {
                max_retries: 1,
                retry_delay_ms: 1000,
            },
        );

        let mut response = String::new();
        while let Some(chunk_result) = stream.next().await {
            match chunk_result {
                Ok(CompletionChunk::Delta { content, .. }) => {
                    response.push_str(&content);
                }
                Ok(CompletionChunk::Done { content, .. }) => {
                    if !content.is_empty() {
                        response.push_str(&content);
                    }
                    break;
                }
                Ok(CompletionChunk::Error { message }) => {
                    return Err(anyhow::anyhow!("refinement LLM error: {}", message));
                }
                Ok(_) => {} // Ignore ToolCalls, TransportFallback, Retry
                Err(e) => {
                    return Err(anyhow::anyhow!("refinement LLM stream error: {}", e));
                }
            }
        }

        Ok(response)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- is_idle_for_consolidation tests --

    #[test]
    fn idle_returns_true_when_all_conditions_met() {
        assert!(is_idle_for_consolidation(
            0,
            0,
            0,
            Some(1000),
            1000 + DEFAULT_IDLE_THRESHOLD_MS,
            DEFAULT_IDLE_THRESHOLD_MS,
        ));
    }

    #[test]
    fn idle_returns_false_with_active_task() {
        assert!(!is_idle_for_consolidation(
            1,
            0,
            0,
            Some(0),
            DEFAULT_IDLE_THRESHOLD_MS + 1,
            DEFAULT_IDLE_THRESHOLD_MS,
        ));
    }

    #[test]
    fn idle_returns_false_with_active_goal_run() {
        assert!(!is_idle_for_consolidation(
            0,
            1,
            0,
            Some(0),
            DEFAULT_IDLE_THRESHOLD_MS + 1,
            DEFAULT_IDLE_THRESHOLD_MS,
        ));
    }

    #[test]
    fn idle_returns_false_with_active_stream() {
        assert!(!is_idle_for_consolidation(
            0,
            0,
            1,
            Some(0),
            DEFAULT_IDLE_THRESHOLD_MS + 1,
            DEFAULT_IDLE_THRESHOLD_MS,
        ));
    }

    #[test]
    fn idle_returns_false_with_recent_presence() {
        // Presence was just 1ms ago -- not idle yet
        assert!(!is_idle_for_consolidation(
            0,
            0,
            0,
            Some(10_000),
            10_001,
            DEFAULT_IDLE_THRESHOLD_MS,
        ));
    }

    #[test]
    fn idle_returns_true_when_no_presence_recorded() {
        // None means no operator has connected -- safe to consolidate
        assert!(is_idle_for_consolidation(
            0,
            0,
            0,
            None,
            1000,
            DEFAULT_IDLE_THRESHOLD_MS,
        ));
    }

    // -- compute_decay_confidence tests --

    #[test]
    fn decay_returns_half_at_half_life() {
        let now = 1_000_000_000u64;
        let half_life_ms = (DEFAULT_HALF_LIFE_HOURS * 3_600_000.0) as u64;
        let last_confirmed = now - half_life_ms;
        let confidence = compute_decay_confidence(last_confirmed, now, DEFAULT_HALF_LIFE_HOURS);
        // Should be ~0.5 at exactly one half-life
        assert!(
            (confidence - 0.5).abs() < 0.01,
            "expected ~0.5, got {confidence}"
        );
    }

    #[test]
    fn decay_returns_near_one_for_just_confirmed() {
        let now = 1_000_000_000u64;
        let confidence = compute_decay_confidence(now, now, DEFAULT_HALF_LIFE_HOURS);
        assert!(
            (confidence - 1.0).abs() < 0.001,
            "expected ~1.0, got {confidence}"
        );
    }

    #[test]
    fn decay_returns_zero_for_zero_timestamp() {
        let confidence = compute_decay_confidence(0, 1_000_000, DEFAULT_HALF_LIFE_HOURS);
        assert_eq!(confidence, 0.0);
    }

    #[test]
    fn decay_returns_zero_for_nonpositive_half_life() {
        let confidence = compute_decay_confidence(500_000, 1_000_000, 0.0);
        assert_eq!(confidence, 0.0);
        let confidence = compute_decay_confidence(500_000, 1_000_000, -5.0);
        assert_eq!(confidence, 0.0);
    }

    #[test]
    fn decay_clamps_to_valid_range() {
        // Even for edge values, confidence should be in [0.0, 1.0]
        let c1 = compute_decay_confidence(1, 2, DEFAULT_HALF_LIFE_HOURS);
        assert!((0.0..=1.0).contains(&c1));

        let c2 = compute_decay_confidence(1, u64::MAX / 2, DEFAULT_HALF_LIFE_HOURS);
        assert!((0.0..=1.0).contains(&c2));
    }

    #[test]
    fn decay_handles_very_large_age_without_panic() {
        // ~5 billion milliseconds = ~58 days
        let confidence = compute_decay_confidence(0, 5_000_000_000, DEFAULT_HALF_LIFE_HOURS);
        assert_eq!(confidence, 0.0); // last_confirmed_at=0 -> always 0.0

        // Large but valid timestamps
        let confidence = compute_decay_confidence(1, 5_000_000_000, DEFAULT_HALF_LIFE_HOURS);
        assert!((0.0..=1.0).contains(&confidence));
        // After many half-lives, confidence should be very close to 0
        assert!(confidence < 0.001);
    }
}
