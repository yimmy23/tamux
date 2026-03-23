//! Concierge agent — proactive welcome greetings and lightweight ops assistant.

use super::llm_client::{self, ApiContent, ApiMessage, RetryStrategy};
use super::provider_resolution::resolve_provider_config_for;
use super::types::*;
use anyhow::Result;
use futures::StreamExt;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

/// Well-known thread ID for the concierge.
pub const CONCIERGE_THREAD_ID: &str = "concierge";

/// Result of concierge triage on a gateway message.
pub enum GatewayTriage {
    /// Simple message — concierge handled it, here's the response text.
    Simple(String),
    /// Complex message — route to full agent loop.
    Complex,
}

pub struct ConciergeEngine {
    config: Arc<RwLock<AgentConfig>>,
    event_tx: broadcast::Sender<AgentEvent>,
    http_client: reqwest::Client,
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

        tracing::info!("concierge: generate_welcome at level {:?}", detail_level);
        let context = self.gather_context(threads, tasks, detail_level).await;
        let (content, actions) = self.compose_welcome(detail_level, &context).await;

        if content.is_empty() {
            return None;
        }

        self.replace_welcome_message(threads, &content).await;

        tracing::info!(
            "concierge: generate_welcome done, {} chars, {} actions",
            content.len(),
            actions.len()
        );
        Some((content, detail_level, actions))
    }

    /// Prune concierge-generated welcome messages from the concierge thread.
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
                thread.updated_at = super::now_millis();
            }
        }
    }

    async fn replace_welcome_message(
        &self,
        threads: &RwLock<std::collections::HashMap<String, AgentThread>>,
        content: &str,
    ) {
        let mut threads_guard = threads.write().await;
        if let Some(thread) = threads_guard.get_mut(CONCIERGE_THREAD_ID) {
            thread.messages.retain(|msg| {
                !(msg.role == MessageRole::Assistant
                    && msg.provider.as_deref() == Some("concierge"))
            });
            thread.messages.push(AgentMessage {
                role: MessageRole::Assistant,
                content: content.to_string(),
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

    // ── Gateway Triage ────────────────────────────────────────────────────

    /// Triage an incoming gateway message.
    /// Returns `Simple(response)` for lightweight messages the concierge can
    /// handle, or `Complex` to route to the full agent loop.
    pub async fn triage_gateway_message(
        &self,
        platform: &str,
        sender: &str,
        content: &str,
        threads: &RwLock<std::collections::HashMap<String, AgentThread>>,
        tasks: &tokio::sync::Mutex<std::collections::VecDeque<AgentTask>>,
    ) -> GatewayTriage {
        let config = self.config.read().await;
        if !config.concierge.enabled {
            return GatewayTriage::Complex;
        }
        let provider_config = match resolve_concierge_provider(&config) {
            Ok(pc) => pc,
            Err(e) => {
                tracing::warn!("concierge: triage provider resolution failed: {e}");
                return GatewayTriage::Complex;
            }
        };
        let provider_id = config
            .concierge
            .provider
            .as_deref()
            .unwrap_or(&config.provider)
            .to_string();
        drop(config);

        let context = self
            .gather_context(threads, tasks, ConciergeDetailLevel::ContextSummary)
            .await;

        let user_prompt = build_gateway_triage_prompt(platform, sender, content, &context);

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
            GATEWAY_TRIAGE_SYSTEM_PROMPT,
            &messages,
            &[],
            provider_config.api_transport,
            None,
            None,
            RetryStrategy::Bounded {
                max_retries: 1,
                retry_delay_ms: 500,
            },
        );

        let mut full_content = String::new();
        let mut stream = std::pin::pin!(stream);
        while let Some(chunk) = stream.next().await {
            match chunk {
                Ok(CompletionChunk::Delta { content, .. }) => full_content.push_str(&content),
                Ok(CompletionChunk::Done { content, .. }) => {
                    if !content.is_empty() {
                        full_content = content;
                    }
                    break;
                }
                Ok(CompletionChunk::Error { message }) => {
                    tracing::warn!("concierge: triage LLM error: {message}");
                    return GatewayTriage::Complex;
                }
                Err(e) => {
                    tracing::warn!("concierge: triage stream error: {e}");
                    return GatewayTriage::Complex;
                }
                Ok(_) => {}
            }
        }

        let trimmed = full_content.trim();
        if trimmed.starts_with("[SIMPLE]") {
            let response = trimmed.trim_start_matches("[SIMPLE]").trim().to_string();
            if response.is_empty() {
                tracing::info!(
                    "concierge: triage returned empty SIMPLE response, routing to agent"
                );
                GatewayTriage::Complex
            } else {
                tracing::info!(
                    platform = %platform,
                    "concierge: triage classified as SIMPLE"
                );
                GatewayTriage::Simple(response)
            }
        } else {
            tracing::info!(
                platform = %platform,
                "concierge: triage classified as COMPLEX"
            );
            GatewayTriage::Complex
        }
    }
}

// ── Gateway triage prompts ──────────────────────────────────────────────

const GATEWAY_TRIAGE_SYSTEM_PROMPT: &str = "\
You are the tamux concierge triage agent. You receive messages from external platforms \
(Slack, Discord, Telegram, WhatsApp) and decide whether to handle them yourself or \
route them to the full agent.\n\n\
SIMPLE messages (handle yourself): greetings, casual chat, status inquiries, \
quick factual lookups, acknowledgments, scheduling questions, thank-yous.\n\
COMPLEX messages (route to agent): code requests, file operations, debugging, \
multi-step tasks, anything requiring tools, technical analysis, project work.\n\n\
If SIMPLE: respond with [SIMPLE] followed by your concise, friendly reply.\n\
If COMPLEX: respond with just [COMPLEX].\n\
Be fast. One sentence for simple replies. Never hallucinate tool usage.";

fn build_gateway_triage_prompt(
    platform: &str,
    sender: &str,
    content: &str,
    context: &WelcomeContext,
) -> String {
    let mut prompt = format!("[{platform} message from {sender}]: {content}\n");
    if let Some(last) = context.recent_threads.first() {
        prompt.push_str(&format!(
            "\nContext: Last session was \"{}\" ({}).",
            last.title,
            format_timestamp(last.updated_at),
        ));
    }
    if !context.pending_tasks.is_empty() {
        prompt.push_str(&format!(" {} pending tasks.", context.pending_tasks.len()));
    }
    prompt
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
    resolve_provider_config_for(config, provider_id, config.concierge.model.as_deref())
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn concierge_thread(messages: Vec<AgentMessage>) -> AgentThread {
        AgentThread {
            id: CONCIERGE_THREAD_ID.to_string(),
            title: "Concierge".to_string(),
            created_at: 1,
            updated_at: 1,
            messages,
            pinned: true,
            upstream_thread_id: None,
            upstream_transport: None,
            upstream_provider: None,
            upstream_model: None,
            upstream_assistant_id: None,
            total_input_tokens: 0,
            total_output_tokens: 0,
        }
    }

    #[tokio::test]
    async fn prune_welcome_messages_removes_all_concierge_welcomes() {
        let config = Arc::new(RwLock::new(AgentConfig::default()));
        let (event_tx, _) = broadcast::channel(8);
        let engine = ConciergeEngine::new(config, event_tx, reqwest::Client::new());
        let threads = RwLock::new(HashMap::from([(
            CONCIERGE_THREAD_ID.to_string(),
            concierge_thread(vec![
                AgentMessage {
                    role: MessageRole::Assistant,
                    content: "hello".to_string(),
                    tool_calls: None,
                    tool_call_id: None,
                    tool_name: None,
                    tool_arguments: None,
                    tool_status: None,
                    input_tokens: 0,
                    output_tokens: 0,
                    provider: None,
                    model: None,
                    api_transport: None,
                    response_id: None,
                    reasoning: None,
                    timestamp: 1,
                },
                AgentMessage {
                    role: MessageRole::Assistant,
                    content: "welcome 1".to_string(),
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
                    timestamp: 2,
                },
                AgentMessage {
                    role: MessageRole::Assistant,
                    content: "welcome 2".to_string(),
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
                    timestamp: 3,
                },
            ]),
        )]));

        engine.prune_welcome_messages(&threads).await;

        let guard = threads.read().await;
        let thread = guard.get(CONCIERGE_THREAD_ID).unwrap();
        assert_eq!(thread.messages.len(), 1);
        assert_eq!(thread.messages[0].content, "hello");
    }

    #[test]
    fn resolve_concierge_provider_uses_shared_resolution_path() {
        let mut config = AgentConfig::default();
        config.provider = "openai".to_string();
        config.base_url = "https://api.openai.com/v1".to_string();
        config.model = "gpt-5.4".to_string();
        config.reasoning_effort = "high".to_string();
        config.context_window_tokens = 123_000;
        config.assistant_id = "assistant-root".to_string();
        config.concierge.provider = Some("alibaba-coding-plan".to_string());
        config.concierge.model = Some("qwen3.5-plus".to_string());
        config.providers.insert(
            "alibaba-coding-plan".to_string(),
            ProviderConfig {
                base_url: String::new(),
                model: String::new(),
                api_key: "dashscope-key".to_string(),
                assistant_id: String::new(),
                auth_source: AuthSource::ApiKey,
                api_transport: ApiTransport::Responses,
                context_window_tokens: 0,
                reasoning_effort: String::new(),
                response_schema: None,
            },
        );

        let resolved = resolve_concierge_provider(&config).expect("concierge provider should resolve");
        let shared = resolve_provider_config_for(
            &config,
            "alibaba-coding-plan",
            Some("qwen3.5-plus"),
        )
        .expect("shared provider resolution should succeed");
        assert_eq!(resolved.base_url, shared.base_url);
        assert_eq!(resolved.model, shared.model);
        assert_eq!(resolved.api_key, shared.api_key);
        assert_eq!(resolved.reasoning_effort, shared.reasoning_effort);
        assert_eq!(resolved.assistant_id, shared.assistant_id);
        assert_eq!(resolved.context_window_tokens, shared.context_window_tokens);
        assert_eq!(resolved.api_transport, shared.api_transport);
    }
}
