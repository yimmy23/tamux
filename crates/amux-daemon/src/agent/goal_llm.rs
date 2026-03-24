//! Goal LLM interactions — plan generation, replanning, reflection, and structured output.

use super::*;

impl AgentEngine {
    pub(super) async fn request_goal_plan(&self, goal_run: &GoalRun) -> Result<GoalPlanResponse> {
        let prompt = format!(
            "You are planning a durable autonomous goal runner inside tamux.\n\
             Produce strict JSON only with the shape:\n\
             {{\"title\":\"...\",\"summary\":\"...\",\"steps\":[{{\"title\":\"...\",\"instructions\":\"...\",\"kind\":\"reason|command|research|memory|skill\",\"success_criteria\":\"...\",\"session_id\":null}}]}}\n\
             Requirements:\n\
             - 2 to 6 steps.\n\
             - Keep each step actionable and narrow.\n\
             - Use kind=command only when the step should execute via the daemon task queue.\n\
             - Use skill only only if a reusable workflow artifact should be generated at the end.\n\
             - Prefer one terminal session unless the goal clearly requires otherwise.\n\
             - All work should be done inside the workspace directory. Do not cd above it.\n\
             Goal title: {}\n\
             Goal:\n{}",
            goal_run.title, goal_run.goal
        );
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
        Ok(plan)
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
             {{\"title\":\"...\",\"summary\":\"...\",\"steps\":[{{\"title\":\"...\",\"instructions\":\"...\",\"kind\":\"reason|command|research|memory|skill\",\"success_criteria\":\"...\",\"session_id\":null}}]}}\n\
             Return only the revised remaining steps, not the full history.\n\
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
             The kind in brackets must be one of: command, research, reason, memory, skill\n\n\
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
        let config = self.config.read().await.clone();
        if config.agent_backend != AgentBackend::Daemon {
            anyhow::bail!("goal runs currently require the built-in daemon agent backend");
        }
        let mut provider_config = self.resolve_provider_config(&config)?;
        provider_config.response_schema = Some(goal_plan_json_schema());
        tracing::info!(
            provider = %config.provider,
            model = %provider_config.model,
            "goal planning LLM call"
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
