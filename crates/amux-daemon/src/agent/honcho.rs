//! Optional Honcho cross-session memory integration.

use serde_json::{json, Value};

use super::*;

const HONCHO_DEFAULT_BASE_URL: &str = "https://api.honcho.dev";
const HONCHO_API_VERSION: &str = "v3";
const HONCHO_ASSISTANT_PEER_ID: &str = "tamux";
const HONCHO_OPERATOR_PEER_ID: &str = "operator";
const HONCHO_SYNC_CACHE_MAX: usize = 10_000;
const HONCHO_CONTEXT_MAX_CHARS: usize = 4_000;

#[derive(Default)]
pub(super) struct HonchoSyncState {
    ordered_ids: VecDeque<String>,
    seen_ids: HashSet<String>,
}

#[derive(Clone)]
struct HonchoClientConfig {
    api_key: String,
    base_url: String,
    workspace_id: String,
}

struct PendingHonchoMessage {
    id: String,
    body: Value,
}

impl HonchoSyncState {
    fn contains(&self, id: &str) -> bool {
        self.seen_ids.contains(id)
    }

    fn remember(&mut self, ids: impl IntoIterator<Item = String>) {
        for id in ids {
            if !self.seen_ids.insert(id.clone()) {
                continue;
            }
            self.ordered_ids.push_back(id);
        }
        while self.ordered_ids.len() > HONCHO_SYNC_CACHE_MAX {
            if let Some(oldest) = self.ordered_ids.pop_front() {
                self.seen_ids.remove(&oldest);
            }
        }
    }
}

impl AgentEngine {
    pub(super) async fn maybe_sync_thread_to_honcho(&self, thread_id: &str) -> Result<()> {
        let config_guard = self.config.read().await;
        let Some(config) = honcho_client_config(&config_guard) else {
            return Ok(());
        };
        drop(config_guard);
        let thread = {
            let threads = self.threads.read().await;
            threads.get(thread_id).cloned()
        };
        let Some(thread) = thread else {
            return Ok(());
        };

        let pending = {
            let state = self.honcho_sync.lock().await;
            thread
                .messages
                .iter()
                .enumerate()
                .filter_map(|(index, message)| {
                    let id = format!("{thread_id}:{index}");
                    if state.contains(&id) {
                        return None;
                    }
                    honcho_message_body(thread_id, &id, message)
                })
                .collect::<Vec<_>>()
        };

        if pending.is_empty() {
            return Ok(());
        }

        ensure_honcho_session(&self.http_client, &config, thread_id).await?;
        create_honcho_messages(
            &self.http_client,
            &config,
            thread_id,
            &pending
                .iter()
                .map(|entry| entry.body.clone())
                .collect::<Vec<_>>(),
        )
        .await?;

        self.honcho_sync
            .lock()
            .await
            .remember(pending.into_iter().map(|entry| entry.id));
        Ok(())
    }

    pub(super) async fn maybe_build_honcho_context(
        &self,
        thread_id: &str,
        query: &str,
    ) -> Result<Option<String>> {
        let config_guard = self.config.read().await;
        let Some(config) = honcho_client_config(&config_guard) else {
            return Ok(None);
        };
        drop(config_guard);
        let query = query.trim();
        if query.is_empty() {
            return Ok(None);
        }
        self.maybe_sync_thread_to_honcho(thread_id).await?;
        let context =
            get_honcho_session_context(&self.http_client, &config, thread_id, query).await?;
        let normalized = normalize_honcho_text(&context);
        if normalized.is_empty() {
            Ok(None)
        } else {
            Ok(Some(truncate_chars(&normalized, HONCHO_CONTEXT_MAX_CHARS)))
        }
    }

    pub(super) async fn query_honcho_memory(&self, query: &str) -> Result<String> {
        let config_guard = self.config.read().await;
        let Some(config) = honcho_client_config(&config_guard) else {
            anyhow::bail!("Honcho memory is not configured.");
        };
        drop(config_guard);
        let query = query.trim();
        if query.is_empty() {
            anyhow::bail!("query must not be empty");
        }
        let response = post_honcho_json(
            &self.http_client,
            &config,
            &format!(
                "/{HONCHO_API_VERSION}/workspaces/{}/peers/{HONCHO_ASSISTANT_PEER_ID}/chat",
                config.workspace_id
            ),
            &json!({
                "query": query,
                "stream": false,
            }),
        )
        .await?;
        let normalized = normalize_honcho_text(&response);
        if normalized.is_empty() {
            Ok("No relevant Honcho memory found.".to_string())
        } else {
            Ok(truncate_chars(&normalized, HONCHO_CONTEXT_MAX_CHARS))
        }
    }
}

fn honcho_client_config(config: &AgentConfig) -> Option<HonchoClientConfig> {
    if !config.enable_honcho_memory || config.honcho_api_key.trim().is_empty() {
        return None;
    }
    Some(HonchoClientConfig {
        api_key: config.honcho_api_key.trim().to_string(),
        base_url: if config.honcho_base_url.trim().is_empty() {
            HONCHO_DEFAULT_BASE_URL.to_string()
        } else {
            config
                .honcho_base_url
                .trim()
                .trim_end_matches('/')
                .to_string()
        },
        workspace_id: if config.honcho_workspace_id.trim().is_empty() {
            "tamux".to_string()
        } else {
            config.honcho_workspace_id.trim().to_string()
        },
    })
}

fn honcho_message_body(
    thread_id: &str,
    id: &str,
    message: &AgentMessage,
) -> Option<PendingHonchoMessage> {
    let content = match message.role {
        MessageRole::User | MessageRole::Assistant => message.content.trim().to_string(),
        MessageRole::Tool => {
            let body = message.content.trim();
            if body.is_empty() {
                return None;
            }
            let tool_name = message.tool_name.as_deref().unwrap_or("tool");
            format!("[tool:{tool_name}] {body}")
        }
        MessageRole::System => return None,
    };
    if content.is_empty() {
        return None;
    }
    let peer_id = match message.role {
        MessageRole::User => HONCHO_OPERATOR_PEER_ID,
        _ => HONCHO_ASSISTANT_PEER_ID,
    };
    Some(PendingHonchoMessage {
        id: id.to_string(),
        body: json!({
            "peer_id": peer_id,
            "content": content,
            "metadata": {
                "thread_id": thread_id,
                "message_id": id,
                "role": match message.role {
                    MessageRole::User => "user",
                    MessageRole::Assistant => "assistant",
                    MessageRole::Tool => "tool",
                    MessageRole::System => "system",
                },
            },
        }),
    })
}

async fn ensure_honcho_session(
    client: &reqwest::Client,
    config: &HonchoClientConfig,
    thread_id: &str,
) -> Result<()> {
    post_honcho_json(
        client,
        config,
        &format!("/{HONCHO_API_VERSION}/workspaces"),
        &json!({ "id": config.workspace_id }),
    )
    .await?;
    post_honcho_json(
        client,
        config,
        &format!(
            "/{HONCHO_API_VERSION}/workspaces/{}/sessions",
            config.workspace_id
        ),
        &json!({ "id": thread_id }),
    )
    .await?;
    post_honcho_json(
        client,
        config,
        &format!(
            "/{HONCHO_API_VERSION}/workspaces/{}/sessions/{thread_id}/peers",
            config.workspace_id
        ),
        &json!({
            HONCHO_OPERATOR_PEER_ID: {},
            HONCHO_ASSISTANT_PEER_ID: {},
        }),
    )
    .await?;
    Ok(())
}

async fn create_honcho_messages(
    client: &reqwest::Client,
    config: &HonchoClientConfig,
    thread_id: &str,
    messages: &[Value],
) -> Result<()> {
    post_honcho_json(
        client,
        config,
        &format!(
            "/{HONCHO_API_VERSION}/workspaces/{}/sessions/{thread_id}/messages",
            config.workspace_id
        ),
        &json!({ "messages": messages }),
    )
    .await
    .map(|_| ())
}

async fn get_honcho_session_context(
    client: &reqwest::Client,
    config: &HonchoClientConfig,
    thread_id: &str,
    query: &str,
) -> Result<Value> {
    ensure_honcho_session(client, config, thread_id).await?;
    let response = client
        .get(format!(
            "{}/{HONCHO_API_VERSION}/workspaces/{}/sessions/{thread_id}/context",
            config.base_url, config.workspace_id
        ))
        .bearer_auth(&config.api_key)
        .query(&[("search_query", query)])
        .send()
        .await?
        .error_for_status()?;
    Ok(response.json::<Value>().await?)
}

async fn post_honcho_json(
    client: &reqwest::Client,
    config: &HonchoClientConfig,
    path: &str,
    body: &Value,
) -> Result<Value> {
    let response = client
        .post(format!("{}{}", config.base_url, path))
        .bearer_auth(&config.api_key)
        .json(body)
        .send()
        .await?
        .error_for_status()?;
    Ok(response.json::<Value>().await.unwrap_or(Value::Null))
}

fn normalize_honcho_text(value: &Value) -> String {
    if let Some(text) = value.as_str() {
        return text.trim().to_string();
    }
    if let Some(text) = value
        .get("content")
        .or_else(|| value.get("response"))
        .or_else(|| value.get("text"))
        .or_else(|| value.get("summary"))
        .and_then(Value::as_str)
    {
        return text.trim().to_string();
    }
    if let Some(messages) = value.get("messages").and_then(Value::as_array) {
        let rendered = messages
            .iter()
            .filter_map(|message| {
                let role = message
                    .get("role")
                    .and_then(Value::as_str)
                    .unwrap_or("memory");
                let content = message
                    .get("content")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|content| !content.is_empty())?;
                Some(format!("{role}: {content}"))
            })
            .collect::<Vec<_>>()
            .join("\n");
        if !rendered.is_empty() {
            return rendered;
        }
    }
    serde_json::to_string_pretty(value).unwrap_or_default()
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        value.to_string()
    } else {
        value.chars().take(max_chars).collect::<String>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_honcho_text_prefers_direct_content() {
        let value = json!({ "content": "  learned preference  " });
        assert_eq!(normalize_honcho_text(&value), "learned preference");
    }

    #[test]
    fn honcho_config_requires_api_key_and_flag() {
        let mut config = AgentConfig::default();
        assert!(honcho_client_config(&config).is_none());
        config.enable_honcho_memory = true;
        config.honcho_api_key = "hc_test".to_string();
        assert!(honcho_client_config(&config).is_some());
    }

    #[test]
    fn agent_config_defaults_enable_chat_capabilities_except_honcho() {
        let config = AgentConfig::default();

        assert!(!config.enable_honcho_memory);
        assert!(config.anticipatory.enabled);
        assert!(config.anticipatory.morning_brief);
        assert!(config.anticipatory.predictive_hydration);
        assert!(config.anticipatory.stuck_detection);
        assert!(config.operator_model.enabled);
        assert!(config.operator_model.allow_message_statistics);
        assert!(config.operator_model.allow_approval_learning);
        assert!(config.operator_model.allow_attention_tracking);
        assert!(config.operator_model.allow_implicit_feedback);
        assert!(config.collaboration.enabled);
        assert!(config.compliance.sign_all_events);
        assert!(config.tool_synthesis.enabled);
        assert!(config.tool_synthesis.require_activation);
    }
}
