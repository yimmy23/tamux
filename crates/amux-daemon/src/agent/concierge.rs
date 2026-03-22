//! Concierge agent — proactive welcome greetings and lightweight ops assistant.

use super::llm_client::{self, ApiContent, ApiMessage, RetryStrategy};
use super::types::*;
use anyhow::Result;
use futures::StreamExt;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

/// Well-known thread ID for the concierge.
pub const CONCIERGE_THREAD_ID: &str = "concierge";

pub struct ConciergeEngine {
    config: Arc<RwLock<AgentConfig>>,
    event_tx: broadcast::Sender<AgentEvent>,
    http_client: reqwest::Client,
    pending_welcome_count: RwLock<usize>,
}

impl ConciergeEngine {
    pub fn new(
        config: Arc<RwLock<AgentConfig>>,
        event_tx: broadcast::Sender<AgentEvent>,
        http_client: reqwest::Client,
    ) -> Self {
        Self {
            config,
            event_tx,
            http_client,
            pending_welcome_count: RwLock::new(0),
        }
    }

    /// Initialize the concierge — ensure the pinned thread exists.
    pub async fn initialize(
        &self,
        threads: &RwLock<std::collections::HashMap<String, AgentThread>>,
    ) {
        let mut threads_guard = threads.write().await;
        if !threads_guard.contains_key(CONCIERGE_THREAD_ID) {
            let now = super::now_millis();
            let thread = AgentThread {
                id: CONCIERGE_THREAD_ID.to_string(),
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

    /// Called when a client subscribes to agent events.
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

        self.prune_welcome_messages(threads).await;

        let detail_level = config.concierge.detail_level;
        tracing::info!("concierge: gathering context at level {:?}", detail_level);
        drop(config);

        let context = self.gather_context(threads, tasks, detail_level).await;
        tracing::info!(
            "concierge: gathered {} threads, {} tasks",
            context.recent_threads.len(),
            context.pending_tasks.len()
        );
        let (content, actions) = self.compose_welcome(detail_level, &context).await;

        if content.is_empty() {
            tracing::warn!("concierge: empty welcome content, skipping emit");
            return;
        }
        tracing::info!(
            "concierge: welcome composed, {} chars, {} actions",
            content.len(),
            actions.len()
        );

        // Add welcome message to the concierge thread.
        {
            let mut threads_guard = threads.write().await;
            if let Some(thread) = threads_guard.get_mut(CONCIERGE_THREAD_ID) {
                thread.messages.push(AgentMessage {
                    role: MessageRole::Assistant,
                    content: content.clone(),
                    tool_calls: None,
                    tool_call_id: None,
                    tool_name: None,
                    tool_arguments: None,
                    tool_status: None,
                    input_tokens: 0,
                    output_tokens: 0,
                    provider: Some("concierge".into()),
                    model: None,
                    api_transport: None,
                    response_id: None,
                    reasoning: None,
                    timestamp: super::now_millis(),
                });
                thread.updated_at = super::now_millis();
            }
        }

        *self.pending_welcome_count.write().await += 1;

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

    /// Generate a welcome and return the data directly (for inline sending).
    /// Also adds the message to the concierge thread, but does NOT emit via event_tx.
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

        self.prune_welcome_messages(threads).await;

        tracing::info!("concierge: generate_welcome at level {:?}", detail_level);
        let context = self.gather_context(threads, tasks, detail_level).await;
        let (content, actions) = self.compose_welcome(detail_level, &context).await;

        if content.is_empty() {
            return None;
        }

        // Add to concierge thread.
        {
            let mut threads_guard = threads.write().await;
            if let Some(thread) = threads_guard.get_mut(CONCIERGE_THREAD_ID) {
                thread.messages.push(AgentMessage {
                    role: MessageRole::Assistant,
                    content: content.clone(),
                    tool_calls: None,
                    tool_call_id: None,
                    tool_name: None,
                    tool_arguments: None,
                    tool_status: None,
                    input_tokens: 0,
                    output_tokens: 0,
                    provider: Some("concierge".into()),
                    model: None,
                    api_transport: None,
                    response_id: None,
                    reasoning: None,
                    timestamp: super::now_millis(),
                });
                thread.updated_at = super::now_millis();
            }
        }
        *self.pending_welcome_count.write().await += 1;

        tracing::info!(
            "concierge: generate_welcome done, {} chars, {} actions",
            content.len(),
            actions.len()
        );
        Some((content, detail_level, actions))
    }

    /// Prune pending welcome messages from the concierge thread.
    pub async fn prune_welcome_messages(
        &self,
        threads: &RwLock<std::collections::HashMap<String, AgentThread>>,
    ) {
        let count = {
            let mut guard = self.pending_welcome_count.write().await;
            let c = *guard;
            *guard = 0;
            c
        };
        if count == 0 {
            return;
        }
        let mut threads_guard = threads.write().await;
        if let Some(thread) = threads_guard.get_mut(CONCIERGE_THREAD_ID) {
            let mut removed = 0;
            thread.messages.retain(|msg| {
                if removed < count
                    && msg.role == MessageRole::Assistant
                    && msg.provider.as_deref() == Some("concierge")
                {
                    removed += 1;
                    false
                } else {
                    true
                }
            });
        }
    }

    // ── Context Gathering ────────────────────────────────────────────────

    async fn gather_context(
        &self,
        threads: &RwLock<std::collections::HashMap<String, AgentThread>>,
        tasks: &tokio::sync::Mutex<std::collections::VecDeque<AgentTask>>,
        detail_level: ConciergeDetailLevel,
    ) -> WelcomeContext {
        let threads_guard = threads.read().await;

        let mut recent_threads: Vec<ThreadSummary> = threads_guard
            .values()
            .filter(|t| t.id != CONCIERGE_THREAD_ID && !t.messages.is_empty())
            .map(|t| {
                let last_messages: Vec<String> = t
                    .messages
                    .iter()
                    .rev()
                    .take(5)
                    .map(|m| {
                        let role = match m.role {
                            MessageRole::User => "User",
                            MessageRole::Assistant => "Assistant",
                            _ => "System",
                        };
                        let snippet: String = m.content.chars().take(120).collect();
                        format!("{}: {}", role, snippet)
                    })
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect();
                ThreadSummary {
                    id: t.id.clone(),
                    title: t.title.clone(),
                    updated_at: t.updated_at,
                    message_count: t.messages.len(),
                    last_messages,
                }
            })
            .collect();
        recent_threads.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        recent_threads.truncate(5);
        drop(threads_guard);

        // Gather tasks for ProactiveTriage and DailyBriefing.
        let pending_tasks = if matches!(
            detail_level,
            ConciergeDetailLevel::ProactiveTriage | ConciergeDetailLevel::DailyBriefing
        ) {
            let tasks_guard = tasks.lock().await;
            tasks_guard
                .iter()
                .filter(|t| {
                    matches!(
                        t.status,
                        TaskStatus::Queued | TaskStatus::InProgress | TaskStatus::Blocked
                    )
                })
                .map(|t| {
                    format!(
                        "- [{}] {} ({})",
                        format!("{:?}", t.status),
                        t.title,
                        format_timestamp(t.created_at)
                    )
                })
                .collect()
        } else {
            Vec::new()
        };

        WelcomeContext {
            recent_threads,
            pending_tasks,
        }
    }

    // ── Welcome Composition ──────────────────────────────────────────────

    async fn compose_welcome(
        &self,
        detail_level: ConciergeDetailLevel,
        context: &WelcomeContext,
    ) -> (String, Vec<ConciergeAction>) {
        let mut actions = Vec::new();

        // Always add actions based on session history.
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

        // Minimal: pure template, no LLM call.
        if detail_level == ConciergeDetailLevel::Minimal {
            let content = if let Some(last) = context.recent_threads.first() {
                format!(
                    "Welcome back! Last session: **{}** ({}). {} messages.",
                    last.title,
                    format_timestamp(last.updated_at),
                    last.message_count
                )
            } else {
                "Welcome to tamux! Ready to start your first session.".into()
            };
            return (content, actions);
        }

        // ContextSummary / ProactiveTriage / DailyBriefing: LLM call.
        let content = match self.call_llm_for_welcome(detail_level, context).await {
            Ok(response) => response,
            Err(e) => {
                tracing::warn!("concierge LLM call failed, falling back to template: {e}");
                // Fallback to a template if LLM fails.
                self.template_fallback(context)
            }
        };

        (content, actions)
    }

    /// Make an LLM call to generate the welcome message.
    async fn call_llm_for_welcome(
        &self,
        detail_level: ConciergeDetailLevel,
        context: &WelcomeContext,
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

        let system_prompt = CONCIERGE_SYSTEM_PROMPT;

        // Build the user prompt with gathered context.
        let user_prompt = self.build_llm_prompt(detail_level, context);

        let messages = vec![ApiMessage {
            role: "user".into(),
            content: ApiContent::Text(user_prompt),
            tool_call_id: None,
            name: None,
            tool_calls: None,
        }];

        let stream = llm_client::send_completion_request(
            &self.http_client,
            &provider_id,
            &provider_config,
            system_prompt,
            &messages,
            &[], // no tools for welcome generation
            provider_config.api_transport,
            None,
            None,
            RetryStrategy::Bounded {
                max_retries: 1,
                retry_delay_ms: 1000,
            },
        );

        // Collect the full response from the stream.
        let mut full_content = String::new();
        let mut stream = std::pin::pin!(stream);
        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(CompletionChunk::Delta { content, .. }) => {
                    full_content.push_str(&content);
                }
                Ok(CompletionChunk::Done { content, .. }) => {
                    if !content.is_empty() {
                        full_content = content;
                    }
                    break;
                }
                Ok(CompletionChunk::Error { message }) => {
                    anyhow::bail!("LLM error: {message}");
                }
                Ok(_) => {} // TransportFallback, Retry, etc.
                Err(e) => {
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
            "Generate a concise welcome greeting for the user who just opened tamux.\n\n",
        );

        // Session context.
        if let Some(last) = context.recent_threads.first() {
            prompt.push_str(&format!(
                "Last session: \"{}\" ({}, {} messages)\n",
                last.title,
                format_timestamp(last.updated_at),
                last.message_count
            ));
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

        match detail_level {
            ConciergeDetailLevel::ContextSummary => {
                prompt.push_str("\nSummarize what the user was working on in 1-2 sentences. Then ask what they'd like to do.");
            }
            ConciergeDetailLevel::ProactiveTriage => {
                if !context.pending_tasks.is_empty() {
                    prompt.push_str("\nPending tasks:\n");
                    for task in &context.pending_tasks {
                        prompt.push_str(&format!("{}\n", task));
                    }
                }
                prompt.push_str("\nProvide a smart triage: summarize the last session, mention any pending tasks or unfinished work, and suggest 2-3 prioritized next steps.");
            }
            ConciergeDetailLevel::DailyBriefing => {
                if !context.pending_tasks.is_empty() {
                    prompt.push_str("\nPending tasks:\n");
                    for task in &context.pending_tasks {
                        prompt.push_str(&format!("{}\n", task));
                    }
                }
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
            if !context.pending_tasks.is_empty() {
                parts.push(format!(
                    "**Pending tasks:** {}",
                    context.pending_tasks.len()
                ));
            }
            parts.push("What would you like to work on?".into());
            parts.join("\n")
        } else {
            "Welcome to tamux! Ready to start your first session.".into()
        }
    }
}

// ── Supporting types ─────────────────────────────────────────────────────

struct WelcomeContext {
    recent_threads: Vec<ThreadSummary>,
    pending_tasks: Vec<String>,
}

struct ThreadSummary {
    id: String,
    title: String,
    updated_at: u64,
    message_count: usize,
    last_messages: Vec<String>,
}

const CONCIERGE_SYSTEM_PROMPT: &str = "\
You are the tamux concierge — a lightweight operational assistant. \
You handle greetings, session navigation, status checks, housekeeping, \
and quick lookups. For coding tasks, deep analysis, or complex work, \
tell the user to switch to the main agent thread.\n\n\
Be concise. One paragraph max for greetings. Use bullet points for \
status summaries. Always offer 2-3 actionable next steps.";

// ── Provider resolution ──────────────────────────────────────────────────

/// Resolve the provider config for the concierge.
/// Checks concierge-specific provider first, falls back to main agent.
fn resolve_concierge_provider(config: &AgentConfig) -> Result<ProviderConfig> {
    let provider_id = config
        .concierge
        .provider
        .as_deref()
        .unwrap_or(&config.provider);
    let model = config.concierge.model.as_deref().unwrap_or(&config.model);

    // Check named providers first.
    if let Some(pc) = config.providers.get(provider_id) {
        let mut resolved = pc.clone();
        if resolved.model.is_empty() {
            resolved.model = model.to_string();
        }
        if !provider_supports_transport(provider_id, resolved.api_transport) {
            resolved.api_transport = default_api_transport_for_provider(provider_id);
        }
        return Ok(resolved);
    }

    // Fall back to top-level config.
    if config.base_url.is_empty() {
        anyhow::bail!(
            "No credentials configured for concierge provider '{}'. Set up in Auth settings.",
            provider_id
        );
    }

    let api_transport = if provider_supports_transport(provider_id, config.api_transport) {
        config.api_transport
    } else {
        default_api_transport_for_provider(provider_id)
    };

    Ok(ProviderConfig {
        base_url: config.base_url.clone(),
        model: model.to_string(),
        api_key: config.api_key.clone(),
        assistant_id: config.assistant_id.clone(),
        auth_source: config.auth_source,
        api_transport,
        reasoning_effort: config.reasoning_effort.clone(),
        context_window_tokens: config.context_window_tokens,
        response_schema: None,
    })
}

// ── Utilities ────────────────────────────────────────────────────────────

fn truncate_str(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{truncated}\u{2026}")
    }
}

fn format_timestamp(ts: u64) -> String {
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
