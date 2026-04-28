#![allow(dead_code)]

use super::*;

#[derive(Debug, PartialEq, Clone, Copy)]
pub(crate) enum ReplayMessageClassification {
    Accepted,
    Duplicate,
    Filtered,
}

fn classify_replay_envelope(
    env: &gateway::ReplayEnvelope,
    seen_ids: &[String],
) -> Option<ReplayMessageClassification> {
    if env.cursor_value.is_empty() || env.channel_id.is_empty() {
        return None;
    }
    if let Some(ref mid) = env.message.message_id {
        if seen_ids.contains(mid) {
            return Some(ReplayMessageClassification::Duplicate);
        }
    }
    if env.message.content.trim().is_empty() {
        return Some(ReplayMessageClassification::Filtered);
    }
    Some(ReplayMessageClassification::Accepted)
}

fn update_in_memory_replay_cursor(
    platform: &str,
    gw: &mut gateway::GatewayState,
    channel_id: &str,
    cursor_value: &str,
) {
    match platform {
        "telegram" => {
            if let Ok(v) = cursor_value.parse::<i64>() {
                gw.telegram_replay_cursor = Some(v);
            }
        }
        "slack" => {
            gw.slack_replay_cursors
                .insert(channel_id.to_string(), cursor_value.to_string());
        }
        "discord" => {
            gw.discord_replay_cursors
                .insert(channel_id.to_string(), cursor_value.to_string());
        }
        "whatsapp" => {
            gw.whatsapp_replay_cursors
                .insert(channel_id.to_string(), cursor_value.to_string());
        }
        _ => {}
    }
}

pub(crate) async fn process_replay_result(
    history: &crate::history::HistoryStore,
    platform: &str,
    result: gateway::ReplayFetchResult,
    gw: &mut gateway::GatewayState,
    seen_ids: &mut Vec<String>,
) -> (Vec<gateway::IncomingMessage>, bool) {
    match result {
        gateway::ReplayFetchResult::InitializeBoundary {
            channel_id,
            cursor_value,
            cursor_type,
        } => {
            if let Err(e) = history
                .save_gateway_replay_cursor(platform, &channel_id, &cursor_value, cursor_type)
                .await
            {
                tracing::warn!(
                    platform,
                    channel_id,
                    "replay: failed to persist init boundary: {e}"
                );
            }
            update_in_memory_replay_cursor(platform, gw, &channel_id, &cursor_value);
            (Vec::new(), true)
        }
        gateway::ReplayFetchResult::Replay(envelopes) => {
            let mut messages = Vec::new();
            for env in envelopes {
                match classify_replay_envelope(&env, seen_ids) {
                    None => {
                        tracing::warn!(
                            platform,
                            cursor_value = %env.cursor_value,
                            channel_id = %env.channel_id,
                            "replay: malformed envelope, stopping replay"
                        );
                        return (messages, false);
                    }
                    Some(
                        ReplayMessageClassification::Duplicate
                        | ReplayMessageClassification::Filtered,
                    ) => {
                        if let Err(e) = history
                            .save_gateway_replay_cursor(
                                platform,
                                &env.channel_id,
                                &env.cursor_value,
                                env.cursor_type,
                            )
                            .await
                        {
                            tracing::warn!(
                                platform,
                                channel_id = %env.channel_id,
                                "replay: cursor persist failed: {e}"
                            );
                        }
                        update_in_memory_replay_cursor(
                            platform,
                            gw,
                            &env.channel_id,
                            &env.cursor_value,
                        );
                    }
                    Some(ReplayMessageClassification::Accepted) => {
                        if let Some(ref mid) = env.message.message_id {
                            seen_ids.push(mid.clone());
                            if seen_ids.len() > 200 {
                                let excess = seen_ids.len() - 200;
                                seen_ids.drain(..excess);
                            }
                        }
                        if let Err(e) = history
                            .save_gateway_replay_cursor(
                                platform,
                                &env.channel_id,
                                &env.cursor_value,
                                env.cursor_type,
                            )
                            .await
                        {
                            tracing::warn!(
                                platform,
                                channel_id = %env.channel_id,
                                "replay: cursor persist failed: {e}"
                            );
                        }
                        update_in_memory_replay_cursor(
                            platform,
                            gw,
                            &env.channel_id,
                            &env.cursor_value,
                        );
                        messages.push(env.message);
                    }
                }
            }
            (messages, true)
        }
    }
}

impl AgentEngine {
    pub(crate) async fn apply_replay_results(
        &self,
        platform_results: Vec<(String, Vec<gateway::ReplayFetchResult>, bool)>,
        gw: &mut gateway::GatewayState,
    ) -> Vec<gateway::IncomingMessage> {
        let mut seen_ids_snap = self.gateway_seen_ids.lock().await.clone();
        let mut replay_msgs: Vec<gateway::IncomingMessage> = Vec::new();

        for (platform, channel_results, fetch_complete) in platform_results {
            let mut all_completed = true;
            let mut platform_msgs: Vec<gateway::IncomingMessage> = Vec::new();

            for result in channel_results {
                let (msgs, completed) =
                    process_replay_result(&self.history, &platform, result, gw, &mut seen_ids_snap)
                        .await;
                platform_msgs.extend(msgs);
                if !completed {
                    all_completed = false;
                    break;
                }
            }

            if all_completed && fetch_complete {
                gw.replay_cycle_active.remove(platform.as_str());
                tracing::info!(
                    platform = %platform,
                    replay_count = platform_msgs.len(),
                    "gateway: replay cycle complete"
                );
            }
            replay_msgs.extend(platform_msgs);
        }
        replay_msgs
    }
}
