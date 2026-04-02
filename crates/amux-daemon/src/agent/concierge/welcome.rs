use super::*;

impl ConciergeEngine {
    pub(super) async fn compose_welcome(
        &self,
        detail_level: ConciergeDetailLevel,
        context: &WelcomeContext,
    ) -> (String, Vec<ConciergeAction>) {
        let actions = self.build_welcome_actions(detail_level, context);
        let signature = build_welcome_signature(detail_level, context);
        if let Some(cached) = self.cached_welcome(&signature).await {
            return cached;
        }

        let content = if detail_level == ConciergeDetailLevel::Minimal {
            if let Some(last) = context.recent_threads.first() {
                format!(
                    "Welcome back! Last session: **{}** ({}). {} messages.",
                    last.title,
                    format_timestamp(last.updated_at),
                    last.message_count
                )
            } else {
                "Welcome to tamux! Ready to start your first session.".into()
            }
        } else {
            match self.call_llm_for_welcome(detail_level, context).await {
                Ok(response) => strip_trailing_actions(&response),
                Err(e) => {
                    tracing::warn!("concierge LLM call failed, falling back to template: {e}");
                    self.template_fallback(context)
                }
            }
        };

        self.cache_welcome(&signature, &content, &actions).await;
        (content, actions)
    }

    pub(super) fn build_welcome_actions(
        &self,
        _detail_level: ConciergeDetailLevel,
        context: &WelcomeContext,
    ) -> Vec<ConciergeAction> {
        let mut actions = Vec::new();
        if let Some(last) = context.recent_threads.first() {
            actions.push(ConciergeAction {
                label: format!("Continue: {}", truncate_str(&last.title, 40)),
                action_type: ConciergeActionType::ContinueSession,
                thread_id: Some(last.id.clone()),
            });
        }
        actions.push(ConciergeAction {
            label: "Start new session".into(),
            action_type: ConciergeActionType::StartNew,
            thread_id: None,
        });
        actions.push(ConciergeAction {
            label: "Search history".into(),
            action_type: ConciergeActionType::Search,
            thread_id: None,
        });
        actions.push(ConciergeAction {
            label: "Dismiss".into(),
            action_type: ConciergeActionType::Dismiss,
            thread_id: None,
        });
        actions
    }

    async fn call_llm_for_welcome(
        &self,
        detail_level: ConciergeDetailLevel,
        context: &WelcomeContext,
    ) -> Result<String> {
        let config = self.config.read().await;
        let provider_config = fast_concierge_provider_config(&resolve_concierge_provider(&config)?);
        let provider_id = config
            .concierge
            .provider
            .as_deref()
            .unwrap_or(&config.provider)
            .to_string();
        drop(config);

        let system_prompt = format!(
            "{}\n\n{}",
            concierge_system_prompt(),
            super::super::build_concierge_runtime_identity_prompt(
                &provider_id,
                &provider_config.model,
            )
        );
        let user_prompt = self.build_llm_prompt(detail_level, context);
        let messages = vec![ApiMessage {
            role: "user".into(),
            content: ApiContent::Text(user_prompt),
            tool_call_id: None,
            name: None,
            tool_calls: None,
        }];

        self.check_circuit_breaker(&provider_id).await?;

        let stream = llm_client::send_completion_request(
            &self.http_client,
            &provider_id,
            &provider_config,
            &system_prompt,
            &messages,
            &[],
            provider_config.api_transport,
            None,
            None,
            RetryStrategy::Bounded {
                max_retries: 1,
                retry_delay_ms: 1000,
            },
        );

        let mut full_content = String::new();
        let mut stream = std::pin::pin!(stream);
        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(CompletionChunk::Delta { content, .. }) => full_content.push_str(&content),
                Ok(CompletionChunk::Done { content, .. }) => {
                    self.record_llm_outcome(&provider_id, true).await;
                    if !content.is_empty() {
                        full_content = content;
                    }
                    break;
                }
                Ok(CompletionChunk::Error { message }) => {
                    self.record_llm_outcome(&provider_id, false).await;
                    anyhow::bail!("LLM error: {message}");
                }
                Ok(_) => {}
                Err(e) => {
                    self.record_llm_outcome(&provider_id, false).await;
                    anyhow::bail!("Stream error: {e}");
                }
            }
        }

        if full_content.trim().is_empty() {
            anyhow::bail!("Empty LLM response");
        }

        Ok(full_content)
    }

    fn build_llm_prompt(
        &self,
        detail_level: ConciergeDetailLevel,
        context: &WelcomeContext,
    ) -> String {
        let mut prompt = String::new();
        prompt.push_str(
            "Generate a concise welcome greeting for the user who just opened tamux.\n\
             IMPORTANT: Do NOT include action buttons, next steps, or numbered suggestions \
             at the end — the UI renders interactive buttons separately. \
             Just provide the status summary and context.\n\n",
        );

        if let Some(last) = context.recent_threads.first() {
            prompt.push_str(&format!(
                "Last session: \"{}\" ({}, {} messages)\n",
                last.title,
                format_timestamp(last.updated_at),
                last.message_count
            ));
            if let Some(opening_message) = &last.opening_message {
                prompt.push_str(&format!(
                    "Conversation opened with:\n  {}\n",
                    opening_message
                ));
            }
            if !last.last_messages.is_empty() {
                prompt.push_str("Recent conversation:\n");
                for msg in &last.last_messages {
                    prompt.push_str(&format!("  {}\n", msg));
                }
            }
        } else {
            prompt.push_str("This is the user's first session — no history yet.\n");
        }

        if context.recent_threads.len() > 1 {
            prompt.push_str("\nOther recent sessions:\n");
            for t in &context.recent_threads[1..] {
                prompt.push_str(&format!(
                    "  - \"{}\" ({}, {} msgs)\n",
                    t.title,
                    format_timestamp(t.updated_at),
                    t.message_count
                ));
            }
        }

        if context.pending_task_total > 0 {
            prompt.push_str(&format!(
                "\nUnresolved tasks: {} total\n",
                context.pending_task_total
            ));
            for task in &context.pending_tasks {
                prompt.push_str(&format!("{task}\n"));
            }
        }

        match detail_level {
            ConciergeDetailLevel::ContextSummary => {
                prompt.push_str(
                    "\nSummarize what the user was working on in 1-2 sentences. Mention the most relevant open work if helpful. Then ask what they'd like to do.",
                );
            }
            ConciergeDetailLevel::ProactiveTriage => {
                prompt.push_str("\nProvide a smart triage: summarize the last session, mention any pending tasks or unfinished work, and suggest 2-3 prioritized next steps.");
            }
            ConciergeDetailLevel::DailyBriefing => {
                prompt.push_str("\nProvide a full operational briefing: session summary, pending tasks, and actionable recommendations. Be comprehensive but concise.");
            }
            ConciergeDetailLevel::Minimal => unreachable!(),
        }

        prompt
    }

    fn template_fallback(&self, context: &WelcomeContext) -> String {
        if let Some(last) = context.recent_threads.first() {
            let mut parts = vec![format!(
                "**Last session:** {} ({}, {} messages)",
                last.title,
                format_timestamp(last.updated_at),
                last.message_count
            )];
            if context.pending_task_total > 0 {
                parts.push(format!("**Pending tasks:** {}", context.pending_task_total));
            }
            parts.push("What would you like to work on?".into());
            parts.join("\n")
        } else {
            "Welcome to tamux! Ready to start your first session.".into()
        }
    }
}

pub(crate) fn fast_concierge_provider_config(config: &ProviderConfig) -> ProviderConfig {
    let mut fast = config.clone();
    if fast.reasoning_effort.trim().is_empty() {
        fast.reasoning_effort = "off".to_string();
    }
    fast
}

pub(crate) fn concierge_system_prompt() -> String {
    format!(
        "You are the tamux concierge — a lightweight operational assistant named {}. \
         You handle greetings, session navigation, status checks, housekeeping, \
         and quick lookups. For coding tasks, deep analysis, or complex work, \
         coordinate with {}, the main agent, when needed instead of pretending you did the deeper work yourself.\n\n\
         Be concise. One paragraph max for greetings. Use bullet points for \
         status summaries. Always offer 2-3 actionable next steps.",
        CONCIERGE_AGENT_NAME,
        MAIN_AGENT_NAME,
    )
}

pub(crate) fn resolve_concierge_provider(config: &AgentConfig) -> Result<ProviderConfig> {
    let provider_id = config
        .concierge
        .provider
        .as_deref()
        .unwrap_or(&config.provider);
    let mut resolved =
        resolve_provider_config_for(config, provider_id, config.concierge.model.as_deref())?;
    resolved.reasoning_effort = config
        .concierge
        .reasoning_effort
        .clone()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "off".to_string());
    Ok(resolved)
}

fn strip_trailing_actions(content: &str) -> String {
    let patterns = [
        "\nNext Steps",
        "\n**Next Steps",
        "\nRecommended Next Steps",
        "\n**Recommended Next Steps",
        "\nRecommended Actions",
        "\n**Recommended Actions",
        "\nSuggested Actions",
        "\n**Suggested Actions",
        "\nAction Items",
        "\n**Action Items",
    ];
    let mut result = content.to_string();
    for pat in &patterns {
        if let Some(pos) = result.find(pat) {
            result.truncate(pos);
        }
    }
    result.trim_end().to_string()
}

fn truncate_str(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{truncated}\u{2026}")
    }
}

pub(super) fn format_timestamp(ts: u64) -> String {
    use std::time::{Duration, UNIX_EPOCH};
    let dt = UNIX_EPOCH + Duration::from_millis(ts);
    let now = std::time::SystemTime::now();
    let elapsed = now.duration_since(dt).unwrap_or_default();
    let secs = elapsed.as_secs();
    if secs < 60 {
        "just now".to_string()
    } else if secs < 3600 {
        format!("{}m ago", secs / 60)
    } else if secs < 86400 {
        format!("{}h ago", secs / 3600)
    } else {
        format!("{}d ago", secs / 86400)
    }
}

pub(super) fn build_welcome_signature(
    detail_level: ConciergeDetailLevel,
    context: &WelcomeContext,
) -> String {
    let thread_sig = context
        .recent_threads
        .iter()
        .map(|thread| {
            format!(
                "{}|{}|{}|{}|{}",
                thread.id,
                thread.title,
                thread.updated_at,
                thread.message_count,
                thread.opening_message.as_deref().unwrap_or("")
            )
        })
        .collect::<Vec<_>>()
        .join(";");
    let task_sig = format!(
        "{}|{}",
        context.pending_task_total,
        context.pending_tasks.join(";")
    );
    format!("{detail_level:?}::{thread_sig}::{task_sig}")
}
