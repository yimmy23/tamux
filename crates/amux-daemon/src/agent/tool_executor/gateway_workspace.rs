// ---------------------------------------------------------------------------
// Gateway messaging — execute via CLI subprocess
// ---------------------------------------------------------------------------

/// Helper: get current epoch millis for last_response_at tracking.
fn now_epoch_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

fn should_use_linked_whatsapp_transport(
    wa_link_state: &str,
    has_native_client: bool,
    has_sidecar_process: bool,
) -> bool {
    wa_link_state == "connected" || has_native_client || has_sidecar_process
}

pub(in crate::agent) async fn execute_gateway_message(
    tool_name: &str,
    args: &serde_json::Value,
    agent: &AgentEngine,
    http_client: &reqwest::Client,
) -> Result<String> {
    let message = args
        .get("message")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'message' argument"))?;
    let gateway = agent.get_config().await.gateway;
    let first_csv =
        |val: &str| -> String { val.split(',').next().unwrap_or("").trim().to_string() };

    match tool_name {
        "send_slack_message" => {
            let channel = args
                .get("channel")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| first_csv(&gateway.slack_channel_filter));
            if channel.is_empty() {
                return Err(anyhow::anyhow!(
                    "No channel specified and no default Slack channel filter in gateway settings"
                ));
            }
            let channel = channel.as_str();

            // Thread context: auto-inject thread_ts from reply_contexts or agent args
            let thread_ts = args
                .get("thread_ts")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .or_else(|| {
                    // Look up auto-injected thread context from gateway state
                    let gw_lock = agent.gateway_state.try_lock().ok()?;
                    let gw = gw_lock.as_ref()?;
                    let ctx = gw.reply_contexts.get(&format!("Slack:{channel}"))?;
                    ctx.slack_thread_ts.clone()
                });

            tracing::info!(
                platform = "slack",
                channel = %channel,
                thread_ts = ?thread_ts,
                "gateway: queueing send request via standalone runtime"
            );

            let result = agent
                .request_gateway_send(amux_protocol::GatewaySendRequest {
                    correlation_id: format!("slack-send-{}", uuid::Uuid::new_v4()),
                    platform: "slack".to_string(),
                    channel_id: channel.to_string(),
                    thread_id: thread_ts,
                    content: message.to_string(),
                })
                .await?;
            if !result.ok {
                return Err(anyhow::anyhow!(
                    "Slack gateway send failed: {}",
                    result.error.unwrap_or_else(|| "unknown error".to_string())
                ));
            }

            Ok(format!("Slack message sent to #{channel}"))
        }
        "send_discord_message" => {
            let mut channel_id = args
                .get("channel_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let mut user_id = args
                .get("user_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            // Fall back to defaults from gateway settings
            if channel_id.is_empty() && user_id.is_empty() {
                let default_channel = first_csv(&gateway.discord_channel_filter);
                if !default_channel.is_empty() {
                    channel_id = default_channel;
                } else {
                    let default_user = first_csv(&gateway.discord_allowed_users);
                    if !default_user.is_empty() {
                        user_id = default_user;
                    }
                }
            }
            let target_channel = if !channel_id.is_empty() {
                channel_id.clone()
            } else if !user_id.is_empty() {
                format!("user:{user_id}")
            } else {
                return Err(anyhow::anyhow!("Either channel_id or user_id is required"));
            };
            let reply_context_channel = if !channel_id.is_empty() {
                target_channel.clone()
            } else {
                let gw_lock = agent.gateway_state.try_lock().ok();
                gw_lock
                    .as_ref()
                    .and_then(|gw| gw.as_ref())
                    .and_then(|gw| gw.discord_dm_channels_by_user.get(&target_channel))
                    .cloned()
                    .unwrap_or_else(|| target_channel.clone())
            };

            // Thread context: auto-inject message_reference from reply_contexts or agent args
            let reply_msg_id = args
                .get("reply_to_message_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .or_else(|| {
                    let gw_lock = agent.gateway_state.try_lock().ok()?;
                    let gw = gw_lock.as_ref()?;
                    let ctx = gw
                        .reply_contexts
                        .get(&format!("Discord:{reply_context_channel}"))?;
                    ctx.discord_message_id.clone()
                });

            tracing::info!(
                platform = "discord",
                channel = %target_channel,
                reply_to = ?reply_msg_id,
                "gateway: queueing send request via standalone runtime"
            );
            let result = agent
                .request_gateway_send(amux_protocol::GatewaySendRequest {
                    correlation_id: format!("discord-send-{}", uuid::Uuid::new_v4()),
                    platform: "discord".to_string(),
                    channel_id: target_channel.clone(),
                    thread_id: reply_msg_id,
                    content: message.to_string(),
                })
                .await?;
            if !result.ok {
                return Err(anyhow::anyhow!(
                    "Discord gateway send failed: {}",
                    result.error.unwrap_or_else(|| "unknown error".to_string())
                ));
            }

            Ok(format!("Discord message sent to {target_channel}"))
        }
        "send_telegram_message" => {
            let chat_id = args
                .get("chat_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| first_csv(&gateway.telegram_allowed_chats));
            if chat_id.is_empty() {
                return Err(anyhow::anyhow!(
                    "No chat_id specified and no default Telegram chat in gateway settings"
                ));
            }
            let chat_id = chat_id.as_str();

            // Thread context: auto-inject reply_to_message_id from reply_contexts or agent args
            let reply_to_id = args
                .get("reply_to_message_id")
                .and_then(|v| v.as_i64())
                .or_else(|| {
                    let gw_lock = agent.gateway_state.try_lock().ok()?;
                    let gw = gw_lock.as_ref()?;
                    let ctx = gw.reply_contexts.get(&format!("Telegram:{chat_id}"))?;
                    ctx.telegram_message_id
                });

            tracing::info!(
                platform = "telegram",
                chat_id = %chat_id,
                reply_to = ?reply_to_id,
                "gateway: queueing send request via standalone runtime"
            );
            let result = agent
                .request_gateway_send(amux_protocol::GatewaySendRequest {
                    correlation_id: format!("telegram-send-{}", uuid::Uuid::new_v4()),
                    platform: "telegram".to_string(),
                    channel_id: chat_id.to_string(),
                    thread_id: reply_to_id.map(|value| value.to_string()),
                    content: message.to_string(),
                })
                .await?;
            if !result.ok {
                return Err(anyhow::anyhow!(
                    "Telegram gateway send failed: {}",
                    result.error.unwrap_or_else(|| "unknown error".to_string())
                ));
            }

            Ok(format!("Telegram message sent to {chat_id}"))
        }
        "send_whatsapp_message" => {
            let phone = args
                .get("phone")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| first_csv(&gateway.whatsapp_allowed_contacts));
            if phone.is_empty() {
                return Err(anyhow::anyhow!(
                    "No phone specified and no default WhatsApp contact in gateway settings"
                ));
            }
            let phone = phone.as_str();
            let wa_link_state = agent.whatsapp_link.status_snapshot().await.state;
            let has_native_client = agent.whatsapp_link.has_native_client().await;
            let has_sidecar_process = agent.whatsapp_link.has_sidecar_process().await;
            if should_use_linked_whatsapp_transport(
                &wa_link_state,
                has_native_client,
                has_sidecar_process,
            ) {
                agent.whatsapp_link.send_message(phone, message).await?;
                {
                    let mut gw_lock = agent.gateway_state.lock().await;
                    if let Some(gw) = gw_lock.as_mut() {
                        gw.last_response_at
                            .insert(format!("WhatsApp:{phone}"), now_epoch_millis());
                    }
                }
                return Ok(format!("WhatsApp linked message sent to {phone}"));
            }
            let wa_token = gateway.whatsapp_token.as_str();
            let phone_id = gateway.whatsapp_phone_id.as_str();
            if wa_token.is_empty() || phone_id.is_empty() {
                return Err(anyhow::anyhow!(
                    "WhatsApp token/phone number ID not configured in gateway settings"
                ));
            }
            tracing::info!(platform = "whatsapp", phone = %phone, "gateway: sending message");
            let url = format!("https://graph.facebook.com/v18.0/{phone_id}/messages");
            let resp = http_client
                .post(&url)
                .bearer_auth(wa_token)
                .json(&serde_json::json!({
                    "messaging_product": "whatsapp",
                    "to": phone,
                    "type": "text",
                    "text": { "body": message }
                }))
                .send()
                .await?;
            if resp.status().is_success() {
                Ok(format!("WhatsApp message sent to {phone}"))
            } else {
                let body = resp.text().await.unwrap_or_default();
                Err(anyhow::anyhow!("WhatsApp API error: {body}"))
            }
        }
        _ => Err(anyhow::anyhow!("unknown gateway tool")),
    }
}

// ---------------------------------------------------------------------------
// Workspace/snippet tools — read/write persistence files
// ---------------------------------------------------------------------------

async fn execute_workspace_tool(
    tool_name: &str,
    args: &serde_json::Value,
    event_tx: &broadcast::Sender<AgentEvent>,
) -> Result<String> {
    let data_dir = super::agent_data_dir()
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .to_path_buf();

    match tool_name {
        "list_workspaces" => {
            let session_path = data_dir.join("session.json");
            match tokio::fs::read_to_string(&session_path).await {
                Ok(raw) => {
                    let parsed: serde_json::Value = serde_json::from_str(&raw)?;
                    let workspaces = parsed.get("workspaces").and_then(|w| w.as_array());
                    match workspaces {
                        Some(ws) => {
                            let mut lines = Vec::new();
                            for w in ws {
                                let name = w.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                                let id = w.get("id").and_then(|v| v.as_str()).unwrap_or("?");
                                let surfaces = w
                                    .get("surfaces")
                                    .and_then(|v| v.as_array())
                                    .map(|s| s.len())
                                    .unwrap_or(0);
                                lines.push(format!("- {name} (id: {id}, {surfaces} surfaces)"));
                            }
                            Ok(lines.join("\n"))
                        }
                        None => Ok("No workspaces found.".into()),
                    }
                }
                Err(_) => Ok("No session file found (app may not have saved state yet).".into()),
            }
        }
        "list_snippets" => {
            let snippets_path = data_dir.join("snippets.json");
            match tokio::fs::read_to_string(&snippets_path).await {
                Ok(raw) => {
                    let parsed: serde_json::Value = serde_json::from_str(&raw)?;
                    let snippets = parsed.as_array();
                    match snippets {
                        Some(ss) => {
                            let mut lines = Vec::new();
                            for s in ss {
                                let name = s.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                                let content =
                                    s.get("content").and_then(|v| v.as_str()).unwrap_or("");
                                let preview: String = content.chars().take(60).collect();
                                lines.push(format!("- {name}: {preview}"));
                            }
                            Ok(lines.join("\n"))
                        }
                        None => Ok("No snippets found.".into()),
                    }
                }
                Err(_) => Ok("No snippets file found.".into()),
            }
        }
        // Mutation tools — emit WorkspaceCommand event for frontend execution
        other => {
            let _ = event_tx.send(AgentEvent::WorkspaceCommand {
                command: other.to_string(),
                args: args.clone(),
            });
            Ok(format!("Executed {other}"))
        }
    }
}

fn strip_ansi_codes(text: &str) -> String {
    // Simple ANSI escape stripping
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            // Skip escape sequence
            if let Some(&next) = chars.peek() {
                if next == '[' {
                    chars.next();
                    // Skip until terminator (letter)
                    while let Some(&c) = chars.peek() {
                        chars.next();
                        if c.is_ascii_alphabetic() || c == '~' {
                            break;
                        }
                    }
                } else if next == ']' {
                    chars.next();
                    // Skip OSC until BEL or ST
                    while let Some(c) = chars.next() {
                        if c == '\x07' {
                            break;
                        }
                        if c == '\x1b' {
                            if chars.peek() == Some(&'\\') {
                                chars.next();
                                break;
                            }
                        }
                    }
                }
            }
        } else {
            result.push(ch);
        }
    }
    result
}

fn strip_html_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut in_script = false;
    let mut in_style = false;

    let lower = html.to_lowercase();
    let chars: Vec<char> = html.chars().collect();
    let lower_chars: Vec<char> = lower.chars().collect();

    let mut i = 0;
    while i < chars.len() {
        if !in_tag && chars[i] == '<' {
            // Check for script/style
            let remaining: String = lower_chars[i..].iter().take(10).collect();
            if remaining.starts_with("<script") {
                in_script = true;
            } else if remaining.starts_with("<style") {
                in_style = true;
            } else if remaining.starts_with("</script") {
                in_script = false;
            } else if remaining.starts_with("</style") {
                in_style = false;
            }
            in_tag = true;
        } else if in_tag && chars[i] == '>' {
            in_tag = false;
        } else if !in_tag && !in_script && !in_style {
            result.push(chars[i]);
        }
        i += 1;
    }

    // Collapse whitespace
    let mut collapsed = String::new();
    let mut last_was_space = false;
    for ch in result.chars() {
        if ch.is_whitespace() {
            if !last_was_space {
                collapsed.push(if ch == '\n' { '\n' } else { ' ' });
                last_was_space = true;
            }
        } else {
            collapsed.push(ch);
            last_was_space = false;
        }
    }

    collapsed.trim().to_string()
}

// Minimal URL encoding (only used for web_search query)
mod urlencoding {
    pub fn encode(s: &str) -> String {
        let mut result = String::new();
        for byte in s.bytes() {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    result.push(byte as char);
                }
                b' ' => result.push('+'),
                _ => {
                    result.push('%');
                    result.push_str(&format!("{:02X}", byte));
                }
            }
        }
        result
    }
}
