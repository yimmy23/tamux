//! Message and thread metadata parsing/building helpers.

use super::*;

pub(super) struct ParsedMessageMetadata {
    pub content_blocks: Vec<AgentContentBlock>,
    pub tool_call_id: Option<String>,
    pub tool_name: Option<String>,
    pub tool_arguments: Option<String>,
    pub tool_status: Option<String>,
    pub weles_review: Option<WelesReviewMeta>,
    pub api_transport: Option<ApiTransport>,
    pub response_id: Option<String>,
    pub upstream_message: Option<CompletionUpstreamMessage>,
    pub provider_final_result: Option<CompletionProviderFinalResult>,
    pub author_agent_id: Option<String>,
    pub author_agent_name: Option<String>,
    pub message_kind: AgentMessageKind,
    pub compaction_strategy: Option<CompactionStrategy>,
    pub compaction_payload: Option<String>,
    pub offloaded_payload_id: Option<String>,
    pub tool_output_preview_path: Option<String>,
    pub structural_refs: Vec<String>,
    pub pinned_for_compaction: bool,
}

pub(super) struct ParsedThreadMetadata {
    pub identity: Option<ThreadIdentityMetadata>,
    pub client_surface: Option<amux_protocol::ClientSurface>,
    pub execution_profile: Option<ThreadExecutionProfile>,
    pub upstream_thread_id: Option<String>,
    pub upstream_transport: Option<ApiTransport>,
    pub upstream_provider: Option<String>,
    pub upstream_model: Option<String>,
    pub upstream_assistant_id: Option<String>,
    pub handoff_state: Option<ThreadHandoffState>,
    pub thread_participants: Vec<ThreadParticipantState>,
    pub thread_participant_suggestions: Vec<ThreadParticipantSuggestion>,
    pub latest_skill_discovery_state: Option<LatestSkillDiscoveryState>,
    pub prompt_memory_injection_state: Option<PromptMemoryInjectionState>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(super) struct ThreadIdentityMetadata {
    pub thread_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal_run_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub goal_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_task_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_thread_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reserved_at: Option<u64>,
}

impl ThreadIdentityMetadata {
    pub(super) fn for_goal_thread(thread_id: &str, goal_run_id: &str) -> Self {
        Self {
            thread_id: thread_id.to_string(),
            goal_run_id: Some(goal_run_id.to_string()),
            goal_id: Some(goal_run_id.to_string()),
            task_id: None,
            parent_task_id: None,
            parent_thread_id: None,
            source: Some("goal_run".to_string()),
            reserved_at: Some(now_millis()),
        }
    }

    pub(super) fn from_task(thread_id: &str, task: &AgentTask) -> Self {
        Self {
            thread_id: thread_id.to_string(),
            goal_run_id: task.goal_run_id.clone(),
            goal_id: task.goal_run_id.clone(),
            task_id: Some(task.id.clone()),
            parent_task_id: task.parent_task_id.clone(),
            parent_thread_id: task.parent_thread_id.clone(),
            source: Some(task.source.clone()),
            reserved_at: Some(now_millis()),
        }
    }

    fn normalized(mut self) -> Self {
        if self.goal_run_id.is_none() {
            self.goal_run_id = self.goal_id.clone();
        }
        if self.goal_id.is_none() {
            self.goal_id = self.goal_run_id.clone();
        }
        self
    }
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
    let _get_string_vec = |key: &str| -> Vec<String> {
        metadata
            .as_ref()
            .and_then(|value| value.get(key))
            .and_then(|value| serde_json::from_value::<Vec<String>>(value.clone()).ok())
            .unwrap_or_default()
    };
    let api_transport = metadata
        .as_ref()
        .and_then(|value| value.get("api_transport"))
        .and_then(|value| serde_json::from_value::<ApiTransport>(value.clone()).ok());
    let weles_review = metadata
        .as_ref()
        .and_then(|value| value.get("weles_review"))
        .and_then(|value| serde_json::from_value::<WelesReviewMeta>(value.clone()).ok());
    let upstream_message = metadata
        .as_ref()
        .and_then(|value| value.get("upstream_message"))
        .and_then(|value| serde_json::from_value::<CompletionUpstreamMessage>(value.clone()).ok());
    let provider_final_result = metadata
        .as_ref()
        .and_then(|value| value.get("provider_final_result"))
        .and_then(|value| {
            serde_json::from_value::<CompletionProviderFinalResult>(value.clone()).ok()
        });
    let message_kind = metadata
        .as_ref()
        .and_then(|value| value.get("message_kind"))
        .and_then(|value| serde_json::from_value::<AgentMessageKind>(value.clone()).ok())
        .unwrap_or_default();
    let compaction_strategy = metadata
        .as_ref()
        .and_then(|value| value.get("compaction_strategy"))
        .and_then(|value| serde_json::from_value::<CompactionStrategy>(value.clone()).ok());
    let structural_refs = metadata
        .as_ref()
        .and_then(|value| value.get("structural_refs"))
        .and_then(|value| serde_json::from_value::<Vec<String>>(value.clone()).ok())
        .unwrap_or_default();
    let content_blocks = metadata
        .as_ref()
        .and_then(|value| value.get("content_blocks"))
        .or_else(|| {
            metadata
                .as_ref()
                .and_then(|value| value.get("contentBlocks"))
        })
        .and_then(|value| serde_json::from_value::<Vec<AgentContentBlock>>(value.clone()).ok())
        .unwrap_or_default();

    ParsedMessageMetadata {
        content_blocks,
        tool_call_id: get_str("tool_call_id"),
        tool_name: get_str("tool_name"),
        tool_arguments: get_str("tool_arguments"),
        tool_status: get_str("tool_status"),
        weles_review,
        api_transport,
        response_id: get_str("response_id"),
        upstream_message,
        provider_final_result,
        author_agent_id: get_str("author_agent_id").or_else(|| get_str("authorAgentId")),
        author_agent_name: get_str("author_agent_name").or_else(|| get_str("authorAgentName")),
        message_kind,
        compaction_strategy,
        compaction_payload: get_str("compaction_payload"),
        offloaded_payload_id: get_str("offloaded_payload_id"),
        tool_output_preview_path: get_str("tool_output_preview_path")
            .or_else(|| get_str("toolOutputPreviewPath")),
        structural_refs,
        pinned_for_compaction: metadata
            .as_ref()
            .and_then(|value| value.get("pinned_for_compaction"))
            .and_then(|value| value.as_bool())
            .or_else(|| {
                metadata
                    .as_ref()
                    .and_then(|value| value.get("pinnedForCompaction"))
                    .and_then(|value| value.as_bool())
            })
            .unwrap_or(false),
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
    let execution_profile = metadata
        .as_ref()
        .and_then(|value| value.get("execution_profile"))
        .or_else(|| {
            metadata
                .as_ref()
                .and_then(|value| value.get("thread_profile"))
        })
        .and_then(|value| serde_json::from_value::<ThreadExecutionProfile>(value.clone()).ok());
    let identity = metadata
        .as_ref()
        .and_then(|value| value.get("identity"))
        .and_then(|value| serde_json::from_value::<ThreadIdentityMetadata>(value.clone()).ok())
        .or_else(|| {
            let thread_id = get_str("thread_id")?;
            Some(ThreadIdentityMetadata {
                thread_id,
                goal_run_id: get_str("goal_run_id").or_else(|| get_str("goal_id")),
                goal_id: get_str("goal_id").or_else(|| get_str("goal_run_id")),
                task_id: get_str("task_id"),
                parent_task_id: get_str("parent_task_id"),
                parent_thread_id: get_str("parent_thread_id"),
                source: get_str("source"),
                reserved_at: metadata
                    .as_ref()
                    .and_then(|value| value.get("reserved_at"))
                    .and_then(|value| value.as_u64()),
            })
        })
        .map(ThreadIdentityMetadata::normalized);

    ParsedThreadMetadata {
        identity,
        client_surface,
        execution_profile,
        upstream_thread_id: get_str("upstream_thread_id"),
        upstream_transport,
        upstream_provider: get_str("upstream_provider"),
        upstream_model: get_str("upstream_model"),
        upstream_assistant_id: get_str("upstream_assistant_id"),
        thread_participants: metadata
            .as_ref()
            .and_then(|value| value.get("thread_participants"))
            .and_then(|value| {
                serde_json::from_value::<Vec<ThreadParticipantState>>(value.clone()).ok()
            })
            .map(normalize_thread_participants)
            .unwrap_or_default(),
        thread_participant_suggestions: metadata
            .as_ref()
            .and_then(|value| value.get("thread_participant_suggestions"))
            .and_then(|value| {
                serde_json::from_value::<Vec<ThreadParticipantSuggestion>>(value.clone()).ok()
            })
            .unwrap_or_default(),
        latest_skill_discovery_state: metadata
            .as_ref()
            .and_then(|value| value.get("latest_skill_discovery_state"))
            .and_then(|value| {
                serde_json::from_value::<LatestSkillDiscoveryState>(value.clone()).ok()
            }),
        prompt_memory_injection_state: metadata
            .as_ref()
            .and_then(|value| value.get("prompt_memory_injection_state"))
            .and_then(|value| {
                serde_json::from_value::<PromptMemoryInjectionState>(value.clone()).ok()
            }),
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
        "content_blocks": message.content_blocks,
        "contentBlocks": message.content_blocks,
        "toolCallId": message.tool_call_id,
        "toolArguments": message.tool_arguments,
        "toolStatus": message.tool_status,
        "weles_review": message.weles_review,
        "api_transport": message.api_transport,
        "response_id": message.response_id,
        "upstream_message": message.upstream_message,
        "provider_final_result": message.provider_final_result,
        "author_agent_id": message.author_agent_id,
        "authorAgentId": message.author_agent_id,
        "author_agent_name": message.author_agent_name,
        "authorAgentName": message.author_agent_name,
        "message_kind": message.message_kind,
        "compaction_strategy": message.compaction_strategy,
        "compaction_payload": message.compaction_payload,
        "offloaded_payload_id": message.offloaded_payload_id,
        "offloadedPayloadId": message.offloaded_payload_id,
        "tool_output_preview_path": message.tool_output_preview_path,
        "toolOutputPreviewPath": message.tool_output_preview_path,
        "structural_refs": message.structural_refs,
        "structuralRefs": message.structural_refs,
        "pinned_for_compaction": message.pinned_for_compaction,
        "pinnedForCompaction": message.pinned_for_compaction,
    }))
    .ok()
}

pub(super) fn build_thread_metadata_json(
    thread: &AgentThread,
    identity: Option<&ThreadIdentityMetadata>,
    client_surface: Option<amux_protocol::ClientSurface>,
    execution_profile: Option<&ThreadExecutionProfile>,
    handoff_state: Option<&ThreadHandoffState>,
    thread_participants: &[ThreadParticipantState],
    thread_participant_suggestions: &[ThreadParticipantSuggestion],
    latest_skill_discovery_state: Option<&LatestSkillDiscoveryState>,
    prompt_memory_injection_state: Option<&PromptMemoryInjectionState>,
) -> Option<String> {
    serde_json::to_string(&serde_json::json!({
        "identity": identity,
        "thread_id": identity.map(|identity| identity.thread_id.clone()),
        "threadId": identity.map(|identity| identity.thread_id.clone()),
        "goal_run_id": identity.and_then(|identity| identity.goal_run_id.clone()),
        "goalRunId": identity.and_then(|identity| identity.goal_run_id.clone()),
        "goal_id": identity.and_then(|identity| identity.goal_id.clone()),
        "goalId": identity.and_then(|identity| identity.goal_id.clone()),
        "task_id": identity.and_then(|identity| identity.task_id.clone()),
        "taskId": identity.and_then(|identity| identity.task_id.clone()),
        "parent_task_id": identity.and_then(|identity| identity.parent_task_id.clone()),
        "parentTaskId": identity.and_then(|identity| identity.parent_task_id.clone()),
        "parent_thread_id": identity.and_then(|identity| identity.parent_thread_id.clone()),
        "parentThreadId": identity.and_then(|identity| identity.parent_thread_id.clone()),
        "source": identity.and_then(|identity| identity.source.clone()),
        "reserved_at": identity.and_then(|identity| identity.reserved_at),
        "reservedAt": identity.and_then(|identity| identity.reserved_at),
        "client_surface": client_surface,
        "clientSurface": client_surface,
        "execution_profile": execution_profile,
        "thread_profile": execution_profile,
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
        "thread_participants": thread_participants,
        "thread_participant_suggestions": thread_participant_suggestions,
        "latest_skill_discovery_state": latest_skill_discovery_state,
        "prompt_memory_injection_state": prompt_memory_injection_state,
    }))
    .ok()
}

impl AgentEngine {
    pub(super) async fn set_thread_identity_metadata(
        &self,
        thread_id: &str,
        identity: ThreadIdentityMetadata,
    ) {
        let mut identities = self.thread_identity_metadata.write().await;
        let reserved_at = identities
            .get(thread_id)
            .and_then(|existing| existing.reserved_at)
            .or(identity.reserved_at)
            .or_else(|| Some(now_millis()));
        let mut identity = identity.normalized();
        identity.thread_id = thread_id.to_string();
        identity.reserved_at = reserved_at;
        identities.insert(thread_id.to_string(), identity);
    }

    pub(super) async fn set_thread_identity_from_task(&self, thread_id: &str, task: &AgentTask) {
        self.set_thread_identity_metadata(
            thread_id,
            ThreadIdentityMetadata::from_task(thread_id, task),
        )
        .await;
    }

    pub(super) async fn set_thread_execution_profile(
        &self,
        thread_id: &str,
        profile: Option<ThreadExecutionProfile>,
    ) {
        let mut profiles = self.thread_execution_profiles.write().await;
        match profile {
            Some(profile) => {
                profiles.insert(thread_id.to_string(), profile);
            }
            None => {
                profiles.remove(thread_id);
            }
        }
    }
}
