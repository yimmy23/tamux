//! Message and thread metadata parsing/building helpers.

use super::*;

pub(super) struct ParsedMessageMetadata {
    pub tool_call_id: Option<String>,
    pub tool_name: Option<String>,
    pub tool_arguments: Option<String>,
    pub tool_status: Option<String>,
    pub weles_review: Option<WelesReviewMeta>,
    pub api_transport: Option<ApiTransport>,
    pub response_id: Option<String>,
    pub message_kind: AgentMessageKind,
    pub compaction_strategy: Option<CompactionStrategy>,
    pub compaction_payload: Option<String>,
}

pub(super) struct ParsedThreadMetadata {
    pub client_surface: Option<amux_protocol::ClientSurface>,
    pub upstream_thread_id: Option<String>,
    pub upstream_transport: Option<ApiTransport>,
    pub upstream_provider: Option<String>,
    pub upstream_model: Option<String>,
    pub upstream_assistant_id: Option<String>,
    pub handoff_state: Option<ThreadHandoffState>,
}

pub(super) fn parse_message_metadata(metadata_json: Option<&str>) -> ParsedMessageMetadata {
    let metadata = metadata_json.and_then(|json| {
        serde_json::from_str::<serde_json::Value>(json)
            .ok()
            .map(|mut value| {
                super::config::normalize_config_keys_to_snake_case(&mut value);
                value
            })
    });
    let get_str = |key: &str| -> Option<String> {
        metadata
            .as_ref()
            .and_then(|value| value.get(key).and_then(|entry| entry.as_str()))
            .map(ToOwned::to_owned)
    };
    let api_transport = metadata
        .as_ref()
        .and_then(|value| value.get("api_transport"))
        .and_then(|value| serde_json::from_value::<ApiTransport>(value.clone()).ok());
    let weles_review = metadata
        .as_ref()
        .and_then(|value| value.get("weles_review"))
        .and_then(|value| serde_json::from_value::<WelesReviewMeta>(value.clone()).ok());
    let message_kind = metadata
        .as_ref()
        .and_then(|value| value.get("message_kind"))
        .and_then(|value| serde_json::from_value::<AgentMessageKind>(value.clone()).ok())
        .unwrap_or_default();
    let compaction_strategy = metadata
        .as_ref()
        .and_then(|value| value.get("compaction_strategy"))
        .and_then(|value| serde_json::from_value::<CompactionStrategy>(value.clone()).ok());

    ParsedMessageMetadata {
        tool_call_id: get_str("tool_call_id"),
        tool_name: get_str("tool_name"),
        tool_arguments: get_str("tool_arguments"),
        tool_status: get_str("tool_status"),
        weles_review,
        api_transport,
        response_id: get_str("response_id"),
        message_kind,
        compaction_strategy,
        compaction_payload: get_str("compaction_payload"),
    }
}

pub(super) fn parse_thread_metadata(metadata_json: Option<&str>) -> ParsedThreadMetadata {
    let metadata = metadata_json.and_then(|json| {
        serde_json::from_str::<serde_json::Value>(json)
            .ok()
            .map(|mut value| {
                super::config::normalize_config_keys_to_snake_case(&mut value);
                value
            })
    });
    let get_str = |key: &str| -> Option<String> {
        metadata
            .as_ref()
            .and_then(|value| value.get(key).and_then(|entry| entry.as_str()))
            .map(ToOwned::to_owned)
    };
    let upstream_transport = metadata
        .as_ref()
        .and_then(|value| value.get("upstream_transport"))
        .and_then(|value| serde_json::from_value::<ApiTransport>(value.clone()).ok());
    let client_surface = metadata
        .as_ref()
        .and_then(|value| value.get("client_surface"))
        .and_then(|value| {
            serde_json::from_value::<amux_protocol::ClientSurface>(value.clone()).ok()
        });

    ParsedThreadMetadata {
        client_surface,
        upstream_thread_id: get_str("upstream_thread_id"),
        upstream_transport,
        upstream_provider: get_str("upstream_provider"),
        upstream_model: get_str("upstream_model"),
        upstream_assistant_id: get_str("upstream_assistant_id"),
        handoff_state: metadata.as_ref().and_then(|value| {
            let origin_agent_id = value
                .get("origin_agent_id")
                .and_then(|entry| entry.as_str())
                .map(ToOwned::to_owned)?;
            let active_agent_id = value
                .get("active_agent_id")
                .and_then(|entry| entry.as_str())
                .map(ToOwned::to_owned)?;
            Some(ThreadHandoffState {
                origin_agent_id,
                active_agent_id,
                responder_stack: value
                    .get("handoff_stack")
                    .and_then(|entry| {
                        serde_json::from_value::<Vec<ThreadResponderFrame>>(entry.clone()).ok()
                    })
                    .unwrap_or_default(),
                events: value
                    .get("handoff_events")
                    .and_then(|entry| {
                        serde_json::from_value::<Vec<ThreadHandoffEvent>>(entry.clone()).ok()
                    })
                    .unwrap_or_default(),
                pending_approval_id: get_str("pending_handoff_approval_id"),
            })
        }),
    }
}

pub(super) fn build_message_metadata_json(message: &AgentMessage) -> Option<String> {
    serde_json::to_string(&serde_json::json!({
        "tool_call_id": message.tool_call_id,
        "tool_name": message.tool_name,
        "toolName": message.tool_name,
        "toolCallId": message.tool_call_id,
        "toolArguments": message.tool_arguments,
        "toolStatus": message.tool_status,
        "weles_review": message.weles_review,
        "api_transport": message.api_transport,
        "response_id": message.response_id,
        "message_kind": message.message_kind,
        "compaction_strategy": message.compaction_strategy,
        "compaction_payload": message.compaction_payload,
    }))
    .ok()
}

pub(super) fn build_thread_metadata_json(
    thread: &AgentThread,
    client_surface: Option<amux_protocol::ClientSurface>,
    handoff_state: Option<&ThreadHandoffState>,
) -> Option<String> {
    serde_json::to_string(&serde_json::json!({
        "client_surface": client_surface,
        "clientSurface": client_surface,
        "upstream_thread_id": thread.upstream_thread_id,
        "upstreamThreadId": thread.upstream_thread_id,
        "upstream_transport": thread.upstream_transport,
        "upstreamTransport": thread.upstream_transport,
        "upstream_provider": thread.upstream_provider,
        "upstreamProvider": thread.upstream_provider,
        "upstream_model": thread.upstream_model,
        "upstreamModel": thread.upstream_model,
        "upstream_assistant_id": thread.upstream_assistant_id,
        "upstreamAssistantId": thread.upstream_assistant_id,
        "origin_agent_id": handoff_state.map(|state| state.origin_agent_id.clone()),
        "active_agent_id": handoff_state.map(|state| state.active_agent_id.clone()),
        "handoff_stack": handoff_state.map(|state| state.responder_stack.clone()),
        "handoff_events": handoff_state.map(|state| state.events.clone()),
        "pending_handoff_approval_id": handoff_state.and_then(|state| state.pending_approval_id.clone()),
    }))
    .ok()
}
