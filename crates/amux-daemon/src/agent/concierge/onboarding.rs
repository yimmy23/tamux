use super::*;

impl ConciergeEngine {
    pub async fn deliver_onboarding(
        &self,
        tier: super::super::capability_tier::CapabilityTier,
        threads: &RwLock<std::collections::HashMap<String, AgentThread>>,
    ) -> Result<()> {
        let config = self.config.read().await;
        let detail_level = config.concierge.detail_level;
        drop(config);

        let content = if detail_level == ConciergeDetailLevel::Minimal {
            onboarding_template_fallback(tier)
        } else {
            match self.generate_onboarding_llm(tier).await {
                Ok(text) => text,
                Err(e) => {
                    tracing::warn!(error = %e, "concierge: LLM onboarding failed, using template fallback");
                    onboarding_template_fallback(tier)
                }
            }
        };

        self.replace_welcome_message(threads, &content).await;

        let _ = self.event_tx.send(AgentEvent::ConciergeWelcome {
            thread_id: CONCIERGE_THREAD_ID.to_string(),
            content,
            detail_level,
            actions: self.onboarding_actions(tier),
        });

        Ok(())
    }

    async fn generate_onboarding_llm(
        &self,
        tier: super::super::capability_tier::CapabilityTier,
    ) -> Result<String> {
        let config = self.config.read().await;
        let provider_config = resolve_concierge_provider(&config)?;
        let provider_id = config
            .concierge
            .provider
            .as_deref()
            .unwrap_or(&config.provider)
            .to_string();
        drop(config);

        let system_prompt = format!(
            "{}\n\n{}",
            onboarding_system_prompt(tier),
            super::super::build_concierge_runtime_identity_prompt(
                &provider_id,
                &provider_config.model,
            )
        );

        let messages = vec![ApiMessage {
            role: "user".into(),
            content: ApiContent::Text("This is my first time using tamux.".into()),
            reasoning: None,
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
            anyhow::bail!("Empty LLM onboarding response");
        }

        Ok(full_content)
    }

    fn onboarding_actions(
        &self,
        tier: super::super::capability_tier::CapabilityTier,
    ) -> Vec<ConciergeAction> {
        use super::super::capability_tier::CapabilityTier;
        match tier {
            CapabilityTier::Newcomer => vec![
                ConciergeAction {
                    label: "Send a message".into(),
                    action_type: ConciergeActionType::FocusChat,
                    thread_id: None,
                },
                ConciergeAction {
                    label: "Skip onboarding".into(),
                    action_type: ConciergeActionType::DismissWelcome,
                    thread_id: None,
                },
            ],
            CapabilityTier::Familiar => vec![
                ConciergeAction {
                    label: "Start a goal run".into(),
                    action_type: ConciergeActionType::StartGoalRun,
                    thread_id: None,
                },
                ConciergeAction {
                    label: "Skip".into(),
                    action_type: ConciergeActionType::DismissWelcome,
                    thread_id: None,
                },
            ],
            CapabilityTier::PowerUser => vec![
                ConciergeAction {
                    label: "Open settings".into(),
                    action_type: ConciergeActionType::OpenSettings,
                    thread_id: None,
                },
                ConciergeAction {
                    label: "Skip".into(),
                    action_type: ConciergeActionType::DismissWelcome,
                    thread_id: None,
                },
            ],
            CapabilityTier::Expert => vec![],
        }
    }

    pub async fn announce_tier_transition(
        &self,
        previous_tier: &str,
        new_tier: &str,
    ) -> Result<()> {
        let message = format!(
            "I've noticed you've been getting more comfortable with tamux. \
             I've adjusted your experience from {} to {} — \
             you'll see some new features becoming available. \
             I'll introduce them one at a time over the next few sessions.",
            previous_tier.replace('_', " "),
            new_tier.replace('_', " "),
        );

        let _ = self.event_tx.send(AgentEvent::WorkflowNotice {
            thread_id: CONCIERGE_THREAD_ID.to_string(),
            kind: "tier-transition".to_string(),
            message,
            details: None,
        });

        Ok(())
    }

    pub async fn deliver_next_disclosure(
        &self,
        queue: &mut super::super::capability_tier::DisclosureQueue,
        current_session: u64,
    ) -> Result<()> {
        if let Some(feature) = queue.next_disclosure(current_session) {
            let message = format!(
                "New feature unlocked: **{}**\n\n{}",
                feature.title, feature.description,
            );
            let feature_id = feature.feature_id.clone();

            let _ = self.event_tx.send(AgentEvent::WorkflowNotice {
                thread_id: CONCIERGE_THREAD_ID.to_string(),
                kind: "feature-disclosure".to_string(),
                message,
                details: None,
            });

            queue.mark_disclosed(&feature_id, current_session);
        }
        Ok(())
    }
}

fn onboarding_template_fallback(tier: super::super::capability_tier::CapabilityTier) -> String {
    use super::super::capability_tier::CapabilityTier;
    match tier {
        CapabilityTier::Newcomer => {
            "Welcome to tamux! I'm your AI agent — I can help with tasks, answer questions, \
             and even work on things in the background while you're away.\n\n\
             Try sending me a message to get started. Type something like \
             \"Help me organize my project\" and I'll take it from there."
                .to_string()
        }
        CapabilityTier::Familiar => {
            "Welcome to tamux! If you've used AI chatbots before, you'll feel right at home — \
             but I can do more. I remember our conversations, run background tasks, and \
             complete multi-step goals autonomously.\n\n\
             Try starting a goal run: just describe what you want to accomplish and I'll \
             plan and execute the steps."
                .to_string()
        }
        CapabilityTier::PowerUser => {
            "Welcome to tamux. Your workspace is ready with terminal sessions, task queue, \
             goal runs, and gateway integrations.\n\n\
             Check settings for provider config, sub-agent management, and automation \
             preferences. I adapt to how you work over time."
                .to_string()
        }
        CapabilityTier::Expert => "Config loaded. Daemon running. All capabilities unlocked.\n\n\
             Operator model tracks your patterns. Skills evolve from usage. \
             Memory consolidates during idle time."
            .to_string(),
    }
}

fn onboarding_system_prompt(tier: super::super::capability_tier::CapabilityTier) -> String {
    use super::super::capability_tier::CapabilityTier;
    let tier_context = match tier {
        CapabilityTier::Newcomer => {
            "The user is new to AI agents. Be warm and encouraging. \
             Explain what tamux can do in simple terms. Walk them through \
             sending their first message. Avoid jargon."
        }
        CapabilityTier::Familiar => {
            "The user has used chatbots before. Highlight what makes tamux \
             different: persistent memory, goal runs, background work. \
             Suggest trying a simple goal run."
        }
        CapabilityTier::PowerUser => {
            "The user runs automations. Give a quick overview of the workspace: \
             terminal sessions, task queue, goal runs, sub-agents. Point to \
             settings for customization."
        }
        CapabilityTier::Expert => {
            "The user builds agent systems. Be brief: config loaded, daemon running, \
             all features unlocked. Mention the operator model and skill system."
        }
    };
    format!(
        "You are {}, {}'s concierge, operating in tamux. This is the user's first session. \
         {tier_context}\n\n\
         Keep it under 150 words. Be conversational, not robotic. \
         End with one concrete action the user can try right now.",
        CONCIERGE_AGENT_NAME, MAIN_AGENT_NAME,
    )
}
