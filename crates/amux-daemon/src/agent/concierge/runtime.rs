use super::*;

impl ConciergeEngine {
    pub fn new(
        config: Arc<RwLock<AgentConfig>>,
        event_tx: broadcast::Sender<AgentEvent>,
        http_client: reqwest::Client,
        circuit_breakers: Arc<CircuitBreakerRegistry>,
    ) -> Self {
        Self {
            config,
            event_tx,
            http_client,
            circuit_breakers,
            welcome_cache: Arc::new(RwLock::new(None)),
            recovery_investigations: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub(crate) async fn maybe_start_recovery_investigation(
        &self,
        agent: &super::super::AgentEngine,
        thread_id: &str,
        signature: &str,
        failure_class: &str,
        summary: &str,
        diagnostics: &Value,
    ) -> Option<String> {
        let key = format!("{thread_id}::{signature}");

        {
            let mut investigations = self.recovery_investigations.lock().await;
            if let Some(existing_task_id) = investigations.get(&key).cloned() {
                let tasks = agent.tasks.lock().await;
                let still_running = tasks
                    .iter()
                    .find(|task| task.id == existing_task_id)
                    .is_some_and(|task| {
                        !super::super::task_scheduler::is_task_terminal_status(task.status)
                    });
                drop(tasks);
                if still_running {
                    return None;
                }
                investigations.remove(&key);
            }
        }

        let task = agent
            .enqueue_task(
                format!("Investigate daemon recovery: {signature}"),
                build_recovery_investigation_description(
                    thread_id,
                    signature,
                    failure_class,
                    summary,
                    diagnostics,
                ),
                "normal",
                None,
                None,
                Vec::new(),
                None,
                "concierge_recovery",
                None,
                None,
                Some(thread_id.to_string()),
                Some("daemon".to_string()),
            )
            .await;
        let task = agent.retarget_task_to_weles(&task.id).await.unwrap_or(task);

        self.recovery_investigations
            .lock()
            .await
            .insert(key, task.id.clone());

        Some(task.id)
    }

    pub async fn initialize(
        &self,
        threads: &RwLock<std::collections::HashMap<String, AgentThread>>,
    ) {
        let mut threads_guard = threads.write().await;
        if let Some(thread) = threads_guard.get_mut(CONCIERGE_THREAD_ID) {
            thread.agent_name = Some(CONCIERGE_AGENT_NAME.to_string());
            thread.messages.clear();
            tracing::info!("concierge: cleared stale messages from existing thread");
        } else {
            let now = super::super::now_millis();
            let thread = AgentThread {
                id: CONCIERGE_THREAD_ID.to_string(),
                agent_name: Some(CONCIERGE_AGENT_NAME.to_string()),
                title: "Concierge".to_string(),
                created_at: now,
                updated_at: now,
                messages: Vec::new(),
                pinned: true,
                upstream_thread_id: None,
                upstream_transport: None,
                upstream_provider: None,
                upstream_model: None,
                upstream_assistant_id: None,
                total_input_tokens: 0,
                total_output_tokens: 0,
            };
            threads_guard.insert(CONCIERGE_THREAD_ID.to_string(), thread);
            tracing::info!("concierge: created pinned thread");
        }
    }

    pub async fn on_client_connected(
        &self,
        threads: &RwLock<std::collections::HashMap<String, AgentThread>>,
        tasks: &tokio::sync::Mutex<std::collections::VecDeque<AgentTask>>,
    ) {
        tracing::info!("concierge: on_client_connected called");
        let config = self.config.read().await;
        if !config.concierge.enabled {
            tracing::info!("concierge: disabled in config, skipping");
            return;
        }

        let detail_level = config.concierge.detail_level;
        tracing::info!("concierge: gathering context at level {:?}", detail_level);
        drop(config);

        let context = self.gather_context(threads, tasks, detail_level).await;
        tracing::info!(
            "concierge: gathered {} threads, {} tasks",
            context.recent_threads.len(),
            context.pending_tasks.len()
        );
        let (content, actions) = if let Some(existing) = self
            .reuse_welcome_from_history(threads, detail_level, &context)
            .await
        {
            tracing::info!("concierge: reusing persisted welcome payload");
            existing
        } else {
            self.compose_welcome(detail_level, &context).await
        };

        if content.is_empty() {
            tracing::warn!("concierge: empty welcome content, skipping emit");
            return;
        }

        self.replace_welcome_message(threads, &content).await;

        let send_result = self.event_tx.send(AgentEvent::ConciergeWelcome {
            thread_id: CONCIERGE_THREAD_ID.to_string(),
            content,
            detail_level,
            actions,
        });
        tracing::info!(
            "concierge: ConciergeWelcome event emitted, receivers={}",
            send_result.unwrap_or(0)
        );
    }

    pub async fn generate_welcome(
        &self,
        threads: &RwLock<std::collections::HashMap<String, AgentThread>>,
        tasks: &tokio::sync::Mutex<std::collections::VecDeque<AgentTask>>,
    ) -> Option<(String, ConciergeDetailLevel, Vec<ConciergeAction>)> {
        let config = self.config.read().await;
        if !config.concierge.enabled {
            tracing::info!("concierge: disabled, skipping generate_welcome");
            return None;
        }
        let detail_level = config.concierge.detail_level;
        drop(config);

        let context = self.gather_context(threads, tasks, detail_level).await;
        let (content, actions) = if let Some(existing) = self
            .reuse_welcome_from_history(threads, detail_level, &context)
            .await
        {
            existing
        } else {
            self.compose_welcome(detail_level, &context).await
        };

        if content.is_empty() {
            return None;
        }

        self.replace_welcome_message(threads, &content).await;
        Some((content, detail_level, actions))
    }

    pub async fn prune_welcome_messages(
        &self,
        threads: &RwLock<std::collections::HashMap<String, AgentThread>>,
    ) {
        let mut threads_guard = threads.write().await;
        if let Some(thread) = threads_guard.get_mut(CONCIERGE_THREAD_ID) {
            let before = thread.messages.len();
            thread.messages.retain(|msg| {
                !(msg.role == MessageRole::Assistant
                    && msg.provider.as_deref() == Some("concierge"))
            });
            if thread.messages.len() != before {
                thread.updated_at = super::super::now_millis();
            }
        }
        *self.welcome_cache.write().await = None;
    }

    pub(super) async fn replace_welcome_message(
        &self,
        threads: &RwLock<std::collections::HashMap<String, AgentThread>>,
        content: &str,
    ) {
        let mut threads_guard = threads.write().await;
        if let Some(thread) = threads_guard.get_mut(CONCIERGE_THREAD_ID) {
            thread.messages.clear();
            thread.messages.push(AgentMessage {
                id: generate_message_id(),
                role: MessageRole::Assistant,
                content: content.to_string(),
                content_blocks: Vec::new(),
                tool_calls: None,
                tool_call_id: None,
                tool_name: None,
                tool_arguments: None,
                tool_status: None,
                weles_review: None,
                input_tokens: 0,
                output_tokens: 0,
                cost: None,
                provider: Some("concierge".into()),
                model: None,
                api_transport: None,
                response_id: None,
                upstream_message: None,
                provider_final_result: None,
                author_agent_id: None,
                author_agent_name: None,
                reasoning: None,
                message_kind: AgentMessageKind::Normal,
                compaction_strategy: None,
                compaction_payload: None,
                offloaded_payload_id: None,
                structural_refs: Vec::new(),
                pinned_for_compaction: false,
                timestamp: super::super::now_millis(),
            });
            thread.updated_at = super::super::now_millis();
        }
    }

    pub(super) async fn reuse_welcome_from_history(
        &self,
        threads: &RwLock<std::collections::HashMap<String, AgentThread>>,
        detail_level: ConciergeDetailLevel,
        context: &WelcomeContext,
    ) -> Option<(String, Vec<ConciergeAction>)> {
        let threads_guard = threads.read().await;
        let concierge_thread = threads_guard.get(CONCIERGE_THREAD_ID)?;
        let latest_welcome = concierge_thread.messages.iter().rev().find(|msg| {
            msg.role == MessageRole::Assistant && msg.provider.as_deref() == Some("concierge")
        })?;

        let now = super::super::now_millis();
        if now.saturating_sub(latest_welcome.timestamp) >= WELCOME_REUSE_WINDOW_MS {
            return None;
        }

        let has_user_message_after_welcome = threads_guard
            .values()
            .filter(|thread| !is_heartbeat_thread(thread))
            .flat_map(|thread| thread.messages.iter())
            .any(|msg| msg.role == MessageRole::User && msg.timestamp > latest_welcome.timestamp);
        if has_user_message_after_welcome {
            return None;
        }

        Some((
            latest_welcome.content.clone(),
            self.build_welcome_actions(detail_level, context),
        ))
    }

    pub(super) async fn cached_welcome(
        &self,
        signature: &str,
    ) -> Option<(String, Vec<ConciergeAction>)> {
        let cache = self.welcome_cache.read().await;
        let entry = cache.as_ref()?;
        if entry.signature != signature || entry.created_at.elapsed() > WELCOME_CACHE_TTL {
            return None;
        }
        Some((entry.content.clone(), entry.actions.clone()))
    }

    pub(super) async fn cache_welcome(
        &self,
        signature: &str,
        content: &str,
        actions: &[ConciergeAction],
    ) {
        *self.welcome_cache.write().await = Some(WelcomeCacheEntry {
            signature: signature.to_string(),
            content: content.to_string(),
            actions: actions.to_vec(),
            created_at: std::time::Instant::now(),
        });
    }
}
