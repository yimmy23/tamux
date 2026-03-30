//! Goal LLM interactions — plan generation, replanning, reflection, and structured output.

use super::*;

impl AgentEngine {
    pub(super) async fn request_orchestrator_policy_decision(
        &self,
        prompt: &str,
    ) -> Result<Option<super::orchestrator_policy::PolicyDecision>> {
        let raw = self
            .run_goal_llm_json_with_schema(
                prompt,
                orchestrator_policy_json_schema(),
                "orchestrator policy LLM call",
            )
            .await?;

        Ok(parse_json_block::<super::orchestrator_policy::PolicyDecision>(&raw).ok())
    }

    pub(super) async fn request_goal_plan(&self, goal_run: &GoalRun) -> Result<GoalPlanResponse> {
        // Surface relevant past episodes before planning (Phase 1: Memory Foundation - EPIS-03)
        let episodic_context = match self.retrieve_relevant_episodes(&goal_run.goal, 5).await {
            Ok(episodes) if !episodes.is_empty() => {
                let config = self.config.read().await;
                let max_tokens = config.episodic.max_injection_tokens;
                drop(config);
                super::episodic::retrieval::format_episodic_context(&episodes, max_tokens)
            }
            Ok(_) => String::new(),
            Err(e) => {
                tracing::warn!("Episodic retrieval failed for goal plan: {e}");
                String::new()
            }
        };

        let mut prompt = format!(
            "You are planning a durable autonomous goal runner inside tamux.\n\
             Produce strict JSON only with the shape:\n\
             {{\"title\":\"...\",\"summary\":\"...\",\"steps\":[{{\"title\":\"...\",\"instructions\":\"...\",\"kind\":\"reason|command|research|memory|skill|divergent\",\"success_criteria\":\"...\",\"session_id\":null,\"llm_confidence\":\"confident|likely|uncertain|guessing\",\"llm_confidence_rationale\":\"...\"}}],\"rejected_alternatives\":[\"...\"]}}\n\
             Requirements:\n\
             - 2 to 6 steps.\n\
             - Keep each step actionable and narrow.\n\
             - Use kind=command only when the step should execute via the daemon task queue.\n\
             - Use kind=divergent when a step involves exploring multiple perspectives or tradeoff analysis.\n\
             - Use skill only only if a reusable workflow artifact should be generated at the end.\n\
             - Prefer one terminal session unless the goal clearly requires otherwise.\n\
             - All work should be done inside the workspace directory. Do not cd above it.\n\
             - For each step, include `llm_confidence` and `llm_confidence_rationale` based on your own self-assessment.\n\
             - Also include \"rejected_alternatives\": a list of 1-3 alternative approaches you considered but rejected, each with a brief reason why it was not chosen.\n\
             Goal title: {}\n\
             Goal:\n{}",
            goal_run.title, goal_run.goal
        );

        if !episodic_context.is_empty() {
            prompt.push_str("\n\n");
            prompt.push_str(&episodic_context);
            prompt.push_str("\nConsider the above past experiences when planning. Avoid approaches that previously failed unless circumstances have changed.\n");
        }

        // Surface negative knowledge constraints before planning (Phase 1: Memory Foundation - NKNO-03)
        let negative_constraints_text =
            match self.query_active_constraints(Some(&goal_run.goal)).await {
                Ok(constraints) if !constraints.is_empty() => {
                    let now_ms = super::now_millis();
                    super::episodic::negative_knowledge::format_negative_constraints(
                        &constraints,
                        now_ms,
                    )
                }
                _ => String::new(),
            };
        if !negative_constraints_text.is_empty() {
            prompt.push_str("\n\n");
            prompt.push_str(&negative_constraints_text);
        }

        let mut plan = self
            .run_goal_structured::<GoalPlanResponse>(&prompt)
            .await?;

        // Loop with the model to fix validation issues
        for attempt in 0..10 {
            let issues = collect_plan_issues(&plan);
            if issues.is_empty() {
                break;
            }
            tracing::warn!(attempt, issues = %issues.join("; "), "goal plan has issues, asking model to fix");
            let fix_prompt = format!(
                "Your goal plan has the following issues that need fixing:\n{}\n\n\
                 Here is your current plan as JSON:\n{}\n\n\
                 Please return the COMPLETE corrected plan as JSON with all issues fixed.",
                issues
                    .iter()
                    .enumerate()
                    .map(|(i, issue)| format!("{}. {}", i + 1, issue))
                    .collect::<Vec<_>>()
                    .join("\n"),
                serde_json::to_string_pretty(&plan).unwrap_or_default()
            );
            match self
                .run_goal_structured::<GoalPlanResponse>(&fix_prompt)
                .await
            {
                Ok(fixed) => plan = fixed,
                Err(e) => {
                    tracing::warn!(attempt, error = %e, "fix attempt failed to parse");
                    continue;
                }
            }
        }

        apply_plan_defaults(&mut plan);

        // Annotate plan steps with confidence labels (UNCR-01, Phase v3.0)
        self.annotate_plan_steps_with_confidence(
            &mut plan.steps,
            &goal_run.goal,
            goal_run.thread_id.as_deref(),
        )
        .await;

        Ok(plan)
    }

    /// Annotate each plan step with a confidence label [HIGH/MEDIUM/LOW] (UNCR-01).
    ///
    /// Uses hybrid signals (UNCR-06): structural signals plus optional LLM
    /// self-assessment preserved on the plan step.
    async fn annotate_plan_steps_with_confidence(
        &self,
        steps: &mut Vec<GoalPlanStepResponse>,
        goal_text: &str,
        thread_id: Option<&str>,
    ) {
        let config = self.config.read().await;
        if !config.uncertainty.enabled {
            return;
        }
        let thresholds = config.uncertainty.domain_thresholds.clone();
        drop(config);

        // 1. Tool success rate from awareness window (AWAR-01 signal)
        let tool_success_rate = {
            let monitor = self.awareness.read().await;
            monitor.aggregate_short_term_success_rate()
        };

        // Compute operator urgency from real thread pacing signals (EMBD-02).
        let (recent_message_count, avg_gap_secs) = if let Some(thread_id) = thread_id {
            let now = super::now_millis();
            let window_ms = 5 * 60 * 1000;
            let threads = self.threads.read().await;
            if let Some(thread) = threads.get(thread_id) {
                let recent_message_count = thread
                    .messages
                    .iter()
                    .filter(|m| {
                        matches!(m.role, MessageRole::User)
                            && now.saturating_sub(m.timestamp) <= window_ms
                    })
                    .count() as u32;

                let mut last_user_timestamps: Vec<u64> = thread
                    .messages
                    .iter()
                    .filter(|m| matches!(m.role, MessageRole::User))
                    .rev()
                    .take(5)
                    .map(|m| m.timestamp)
                    .collect();
                last_user_timestamps.reverse();

                let avg_gap_secs = if last_user_timestamps.len() < 2 {
                    0
                } else {
                    let gap_sum_ms: u64 = last_user_timestamps
                        .windows(2)
                        .map(|pair| pair[1].saturating_sub(pair[0]))
                        .sum();
                    let avg_gap_ms = gap_sum_ms / (last_user_timestamps.len() as u64 - 1);
                    avg_gap_ms / 1000
                };

                (recent_message_count, avg_gap_secs)
            } else {
                (0, 0)
            }
        } else {
            (0, 0)
        };
        let temperature =
            super::embodied::dimensions::compute_temperature(recent_message_count, avg_gap_secs);

        for step in steps.iter_mut() {
            // 2. Episodic familiarity: count FTS5 hits for step instructions
            let episodic_familiarity = {
                let query = &step.instructions;
                match self.retrieve_relevant_episodes(query, 5).await {
                    Ok(episodes) => {
                        super::embodied::dimensions::compute_familiarity(episodes.len())
                    }
                    Err(_) => 0.5, // default to moderate familiarity on error
                }
            };

            // Compute difficulty from awareness window error rate (EMBD-01)
            let difficulty = {
                let monitor = self.awareness.read().await;
                let error_rate = 1.0 - monitor.aggregate_short_term_success_rate();
                super::embodied::dimensions::compute_difficulty(error_rate, 0)
            };

            // Compute weight from step kind (EMBD-03)
            // GoalRunStepKind variants: Reason, Command, Research, Memory, Skill, Specialist, Unknown
            let weight = {
                let tool_name = match &step.kind {
                    GoalRunStepKind::Command => "execute_command",
                    GoalRunStepKind::Research => "web_search",
                    GoalRunStepKind::Reason => "read_file",
                    GoalRunStepKind::Memory => "read_file",
                    GoalRunStepKind::Skill => "execute_command",
                    GoalRunStepKind::Specialist(_) => "execute_command",
                    GoalRunStepKind::Divergent => "read_file",
                    GoalRunStepKind::Unknown => "unknown",
                };
                super::embodied::dimensions::compute_weight(tool_name)
            };

            tracing::trace!(
                difficulty,
                weight,
                temperature,
                recent_message_count,
                avg_gap_secs,
                "embodied dimensions computed for step"
            );

            // 3. Domain classification + blast radius from step kind
            let domain = super::uncertainty::domains::classify_step_kind(&step.kind);
            let blast_radius_score = {
                let domain_score = match domain {
                    super::uncertainty::domains::DomainClassification::Safety => 0.8,
                    super::uncertainty::domains::DomainClassification::Reliability => 0.5,
                    _ => 0.2,
                };
                // Blend domain classification with embodied weight (EMBD-04),
                // then adjust with operator urgency temperature (EMBD-02).
                let base_blast_radius = 0.6 * domain_score + 0.4 * weight;
                (0.85 * base_blast_radius + 0.15 * temperature).clamp(0.0, 1.0)
            };

            // 4. Approach novelty: check counter-who for similar approaches
            let approach_novelty = {
                let scope_id = crate::agent::agent_identity::current_agent_scope_id();
                let stores = self.episodic_store.read().await;
                let store = stores.get(&scope_id).cloned().unwrap_or_default();
                let kind_str = format!("{:?}", step.kind);
                let hash = super::episodic::counter_who::compute_approach_hash(
                    &kind_str,
                    &step.instructions.chars().take(100).collect::<String>(),
                );
                let matching = store
                    .counter_who
                    .tried_approaches
                    .iter()
                    .filter(|a| a.approach_hash == hash)
                    .count();
                super::uncertainty::confidence::approach_novelty_score(matching)
            };

            let signals = super::uncertainty::confidence::ConfidenceSignals {
                tool_success_rate,
                episodic_familiarity,
                blast_radius_score,
                approach_novelty,
                llm_self_assessment: step
                    .llm_confidence
                    .as_deref()
                    .and_then(crate::agent::explanation::ConfidenceBand::from_str),
            };

            let assessment = super::uncertainty::confidence::compute_step_confidence(
                &signals,
                domain,
                &thresholds,
            );

            // Apply calibration adjustment (UNCR-07)
            let calibrated_band = {
                let tracker = self.calibration_tracker.read().await;
                tracker.get_calibrated_band(assessment.band)
            };
            let calibrated_label =
                super::uncertainty::confidence::confidence_label(calibrated_band);

            // Prepend confidence label to step title (locked decision: "[HIGH] Step title")
            step.title = format!("[{}] {}", calibrated_label, step.title);

            // Add confidence evidence to step instructions if not HIGH
            if calibrated_label != "HIGH" && !assessment.evidence.is_empty() {
                step.instructions = format!(
                    "{}\n\nConfidence note: {}",
                    step.instructions,
                    assessment.evidence.join("; ")
                );
            }
        }

        tracing::debug!(
            steps = steps.len(),
            goal = goal_text.chars().take(50).collect::<String>(),
            "annotated plan steps with confidence labels"
        );
    }

    pub(super) async fn request_goal_replan(
        &self,
        goal_run: &GoalRun,
        failure: &str,
    ) -> Result<GoalPlanResponse> {
        let completed = goal_run
            .steps
            .iter()
            .take(goal_run.current_step_index.saturating_add(1))
            .map(|step| {
                format!(
                    "- {} [{}]",
                    step.title,
                    goal_run_step_status_label(step.status)
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let prompt = format!(
            "You are replanning a tamux goal runner after a failed step.\n\
             Produce strict JSON only with the shape:\n\
             {{\"title\":\"...\",\"summary\":\"...\",\"steps\":[{{\"title\":\"...\",\"instructions\":\"...\",\"kind\":\"reason|command|research|memory|skill|divergent\",\"success_criteria\":\"...\",\"session_id\":null,\"llm_confidence\":\"confident|likely|uncertain|guessing\",\"llm_confidence_rationale\":\"...\"}}],\"rejected_alternatives\":[\"...\"]}}\n\
             Return only the revised remaining steps, not the full history.\n\
             For each step, include `llm_confidence` and `llm_confidence_rationale` based on your own self-assessment.\n\
             Goal: {}\n\
             Failure: {}\n\
             Completed / attempted steps:\n{}\n",
            goal_run.goal,
            failure,
            if completed.is_empty() { "- none".into() } else { completed }
        );
        let mut plan = self
            .run_goal_structured::<GoalPlanResponse>(&prompt)
            .await?;

        for attempt in 0..10 {
            let issues = collect_plan_issues(&plan);
            if issues.is_empty() {
                break;
            }
            tracing::warn!(attempt, issues = %issues.join("; "), "goal replan has issues, asking model to fix");
            let fix_prompt = format!(
                "Your revised plan has issues:\n{}\n\nCurrent plan:\n{}\n\nReturn the COMPLETE corrected plan as JSON.",
                issues.iter().enumerate().map(|(i, s)| format!("{}. {}", i+1, s)).collect::<Vec<_>>().join("\n"),
                serde_json::to_string_pretty(&plan).unwrap_or_default()
            );
            match self
                .run_goal_structured::<GoalPlanResponse>(&fix_prompt)
                .await
            {
                Ok(fixed) => plan = fixed,
                Err(e) => {
                    tracing::warn!(attempt, error = %e, "replan fix attempt failed");
                    continue;
                }
            }
        }

        apply_plan_defaults(&mut plan);
        self.annotate_plan_steps_with_confidence(
            &mut plan.steps,
            &goal_run.goal,
            goal_run.thread_id.as_deref(),
        )
        .await;
        Ok(plan)
    }

    pub(super) async fn request_goal_reflection(
        &self,
        goal_run: &GoalRun,
    ) -> Result<GoalReflectionResponse> {
        let step_summaries = goal_run
            .steps
            .iter()
            .map(|step| {
                format!(
                    "- {} [{}]: {}",
                    step.title,
                    goal_run_step_status_label(step.status),
                    step.summary
                        .as_deref()
                        .or(step.error.as_deref())
                        .unwrap_or("no summary")
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let prompt = format!(
            "You are reflecting on a completed tamux goal runner.\n\
             Produce strict JSON only with the shape:\n\
             {{\"summary\":\"...\",\"stable_memory_update\":null,\"generate_skill\":false,\"skill_title\":null}}\n\
             `stable_memory_update` must be null unless you learned a durable operator preference or stable workspace fact worth appending to MEMORY.md.\n\
             Goal: {}\n\
             Step outcomes:\n{}\n",
            goal_run.goal,
            if step_summaries.is_empty() {
                "- no steps recorded".into()
            } else {
                step_summaries
            }
        );
        self.run_goal_structured::<GoalReflectionResponse>(&prompt)
            .await
    }

    /// Run a structured goal LLM call with cascade:
    /// 1. JSON -> 2. retry JSON -> 3. YAML -> 4. retry YAML -> 5. markdown parse
    pub(super) async fn run_goal_structured<T: serde::de::DeserializeOwned>(
        &self,
        prompt: &str,
    ) -> Result<T> {
        // 1. Try JSON
        let raw1 = self.run_goal_llm_json(prompt).await?;
        if let Ok(parsed) = parse_json_block::<T>(&raw1) {
            tracing::info!("goal structured: parsed on first JSON attempt");
            return Ok(parsed);
        }
        tracing::warn!(raw_len = raw1.len(), raw = %raw1, "goal structured: JSON attempt 1 failed");

        // 2. Retry JSON with correction
        let retry_json_prompt = build_json_retry_prompt(prompt, &raw1);
        if let Ok(raw2) = self.run_goal_llm_json(&retry_json_prompt).await {
            if let Ok(parsed) = parse_json_block::<T>(&raw2) {
                tracing::info!("goal structured: parsed on JSON retry");
                return Ok(parsed);
            }
            tracing::warn!(raw_len = raw2.len(), raw = %raw2, "goal structured: JSON attempt 2 failed");
        }

        // 3. Try YAML
        let yaml_prompt = format!(
            "{}\n\n\
             IMPORTANT: Return ONLY valid YAML (not JSON). Use proper YAML indentation.\n\
             Do not wrap in code fences. Do not include any text outside the YAML.",
            prompt
        );
        let raw3 = self.run_goal_llm_raw(&yaml_prompt).await?;
        if let Ok(parsed) = parse_yaml_block::<T>(&raw3) {
            tracing::info!("goal structured: parsed on YAML attempt");
            return Ok(parsed);
        }
        tracing::warn!(raw_len = raw3.len(), raw = %raw3, "goal structured: YAML attempt 1 failed");

        // 4. Retry YAML with correction
        let retry_yaml_prompt = format!(
            "Your previous response could not be parsed.\n\
             Here is what you returned:\n---\n{}\n---\n\n\
             Please return ONLY valid YAML. Use proper indentation. No code fences.\n\n\
             Original request:\n{}",
            raw3.chars().take(2000).collect::<String>(),
            prompt
        );
        let raw4 = self.run_goal_llm_raw(&retry_yaml_prompt).await?;
        if let Ok(parsed) = parse_yaml_block::<T>(&raw4) {
            tracing::info!("goal structured: parsed on YAML retry");
            return Ok(parsed);
        }
        tracing::warn!(raw_len = raw4.len(), raw = %raw4, "goal structured: YAML attempt 2 failed");

        // 5. Markdown fallback — ask for a simple numbered list and parse it
        tracing::warn!("goal structured: trying markdown fallback");
        let md_prompt = format!(
            "I need you to break down a goal into steps. Return ONLY a numbered list.\n\
             Each line must follow this exact format:\n\
             1. [command] Step title: Step instructions. Success: criteria here.\n\n\
             The kind in brackets must be one of: command, research, reason, memory, skill, divergent\n\n\
             Goal: {}\n\n\
             Return ONLY the numbered list, nothing else.",
            prompt.lines().last().unwrap_or(prompt)
        );
        let raw5 = self.run_goal_llm_raw(&md_prompt).await?;
        tracing::info!(raw = %raw5, "goal structured: markdown fallback output");
        if let Ok(parsed) = parse_markdown_steps::<T>(&raw5) {
            tracing::info!("goal structured: parsed via markdown fallback");
            return Ok(parsed);
        }

        tracing::error!("goal structured: all 5 parse attempts failed");
        anyhow::bail!("failed to parse goal plan after JSON, YAML, and markdown attempts")
    }

    /// Raw LLM call without json_mode/schema — used for YAML attempts.
    pub(super) async fn run_goal_llm_raw(&self, prompt: &str) -> Result<String> {
        let config = self.config.read().await.clone();
        if config.agent_backend != AgentBackend::Daemon {
            anyhow::bail!("goal runs currently require the built-in daemon agent backend");
        }
        let provider_config = self.resolve_provider_config(&config)?;
        let messages = vec![ApiMessage {
            role: "user".into(),
            content: ApiContent::Text(prompt.to_string()),
            tool_call_id: None,
            name: None,
            tool_calls: None,
        }];
        self.check_circuit_breaker(&config.provider).await?;
        let mut stream = send_completion_request(
            &self.http_client,
            &config.provider,
            &provider_config,
            "Return structured data only. No markdown fences. No explanation.",
            &messages,
            &[],
            provider_config.api_transport,
            None,
            None,
            RetryStrategy::DurableRateLimited,
        );
        let mut content = String::new();
        let mut reasoning = String::new();
        while let Some(chunk) = stream.next().await {
            let chunk = match chunk {
                Ok(c) => c,
                Err(e) => {
                    self.record_llm_outcome(&config.provider, false).await;
                    return Err(e);
                }
            };
            match chunk {
                CompletionChunk::Delta {
                    content: delta,
                    reasoning: r,
                } => {
                    content.push_str(&delta);
                    if let Some(r) = r {
                        reasoning.push_str(&r);
                    }
                }
                CompletionChunk::Done {
                    content: done,
                    reasoning: r,
                    ..
                } => {
                    self.record_llm_outcome(&config.provider, true).await;
                    if let Some(r) = r {
                        reasoning = r;
                    }
                    let final_content = if done.is_empty() { content } else { done };
                    if !final_content.trim().is_empty() {
                        return Ok(final_content);
                    }
                    if !reasoning.trim().is_empty() {
                        return Ok(reasoning);
                    }
                    anyhow::bail!("goal LLM returned empty output");
                }
                CompletionChunk::Error { message } => {
                    self.record_llm_outcome(&config.provider, false).await;
                    anyhow::bail!(message);
                }
                CompletionChunk::TransportFallback { .. } => {}
                CompletionChunk::Retry { .. } => {}
                CompletionChunk::ToolCalls { .. } => {
                    self.record_llm_outcome(&config.provider, true).await;
                    anyhow::bail!("goal planning unexpectedly returned tool calls");
                }
            }
        }
        if !content.trim().is_empty() {
            return Ok(content);
        }
        anyhow::bail!("goal LLM returned empty output")
    }

    pub(super) async fn run_goal_llm_json(&self, prompt: &str) -> Result<String> {
        self.run_goal_llm_json_with_schema(prompt, goal_plan_json_schema(), "goal planning LLM call")
            .await
    }

    pub(super) async fn run_goal_llm_json_with_schema(
        &self,
        prompt: &str,
        schema: serde_json::Value,
        log_label: &str,
    ) -> Result<String> {
        let config = self.config.read().await.clone();
        if config.agent_backend != AgentBackend::Daemon {
            anyhow::bail!("goal runs currently require the built-in daemon agent backend");
        }
        let mut provider_config = self.resolve_provider_config(&config)?;
        provider_config.response_schema = Some(schema);
        tracing::info!(
            provider = %config.provider,
            model = %provider_config.model,
            operation = log_label,
            "structured LLM call"
        );
        let messages = vec![ApiMessage {
            role: "user".into(),
            content: ApiContent::Text(prompt.to_string()),
            tool_call_id: None,
            name: None,
            tool_calls: None,
        }];
        self.check_circuit_breaker(&config.provider).await?;
        let mut stream = send_completion_request(
            &self.http_client,
            &config.provider,
            &provider_config,
            "Return strict JSON only. Do not call tools. Do not wrap the answer in markdown.",
            &messages,
            &[],
            provider_config.api_transport,
            None,
            None,
            RetryStrategy::DurableRateLimited,
        );
        let mut content = String::new();
        let mut reasoning = String::new();
        while let Some(chunk) = stream.next().await {
            let chunk = match chunk {
                Ok(c) => c,
                Err(e) => {
                    self.record_llm_outcome(&config.provider, false).await;
                    return Err(e);
                }
            };
            match chunk {
                CompletionChunk::Delta {
                    content: delta,
                    reasoning: r,
                } => {
                    content.push_str(&delta);
                    if let Some(r) = r {
                        reasoning.push_str(&r);
                    }
                }
                CompletionChunk::Done {
                    content: done,
                    reasoning: r,
                    ..
                } => {
                    self.record_llm_outcome(&config.provider, true).await;
                    if let Some(r) = r {
                        reasoning = r;
                    }
                    let final_content = if done.is_empty() { content } else { done };
                    // Prefer content, fall back to reasoning if content has no JSON
                    if !final_content.trim().is_empty() && final_content.contains('{') {
                        return Ok(final_content);
                    }
                    // Model may have put JSON inside reasoning output
                    if !reasoning.trim().is_empty() && reasoning.contains('{') {
                        tracing::info!("goal plan: extracting JSON from reasoning output");
                        return Ok(reasoning);
                    }
                    if !final_content.trim().is_empty() {
                        return Ok(final_content);
                    }
                    if !reasoning.trim().is_empty() {
                        return Ok(reasoning);
                    }
                    anyhow::bail!("goal planning returned empty output");
                }
                CompletionChunk::Error { message } => {
                    self.record_llm_outcome(&config.provider, false).await;
                    anyhow::bail!(message);
                }
                CompletionChunk::TransportFallback { .. } => {}
                CompletionChunk::Retry { .. } => {}
                CompletionChunk::ToolCalls { .. } => {
                    self.record_llm_outcome(&config.provider, true).await;
                    anyhow::bail!("goal planning unexpectedly returned tool calls");
                }
            }
        }
        // Stream ended without Done chunk
        let final_content = content;
        if !final_content.trim().is_empty() && final_content.contains('{') {
            return Ok(final_content);
        }
        if !reasoning.trim().is_empty() && reasoning.contains('{') {
            return Ok(reasoning);
        }
        if !final_content.trim().is_empty() {
            return Ok(final_content);
        }
        anyhow::bail!("goal planning returned empty output")
    }

    pub(super) async fn append_goal_memory_update(
        &self,
        goal_run_id: &str,
        update: &str,
    ) -> Result<()> {
        append_goal_memory_note(&self.data_dir, &self.history, update, Some(goal_run_id)).await?;
        self.refresh_memory_cache().await;
        Ok(())
    }

    pub(super) async fn goal_thread_summary(&self, thread_id: &str) -> Option<String> {
        let threads = self.threads.read().await;
        threads.get(thread_id).and_then(|thread| {
            thread
                .messages
                .iter()
                .rev()
                .find(|message| {
                    message.role == MessageRole::Assistant && !message.content.trim().is_empty()
                })
                .map(|message| summarize_text(&message.content, 320))
        })
    }
}

fn orchestrator_policy_json_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "enum": ["continue", "pivot", "escalate", "halt_retries"]
            },
            "reason": {
                "type": "string",
                "maxLength": 240
            },
            "strategy_hint": {
                "type": ["string", "null"],
                "maxLength": 160
            }
        },
        "required": ["action", "reason", "strategy_hint"],
        "additionalProperties": false
    })
}
