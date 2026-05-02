use super::*;

pub(crate) const HEURISTIC_COMPACTION_VISIBLE_TEXT: &str = "rule based";
pub(crate) const COMPACTION_NOTICE_KIND: &str = "auto-compaction";
pub(crate) const MANUAL_COMPACTION_NOTICE_KIND: &str = "manual-compaction";
pub(crate) const COMPACTION_EXACT_MESSAGE_MAX: usize = 24;
pub(crate) const COMPACTION_MODEL_RECENT_CONTENT_MESSAGES: usize = 6;
pub(crate) const COMPACTION_MODEL_REQUEST_HEADROOM_TOKENS: usize = 8_192;
pub(crate) const COMPACTION_RECENT_SIGNAL_MESSAGES: usize = 8;
pub(crate) const CODING_COMPACTION_STRUCTURAL_ENTRY_LIMIT: usize = 6;
pub(crate) const CODING_COMPACTION_GRAPH_NEIGHBOR_LIMIT: usize = 8;
pub(crate) const CODING_COMPACTION_OFFLOAD_REFERENCE_LIMIT: usize = 4;
pub(crate) const COMPACTION_MESSAGE_TRUNCATION_NOTICE: &str =
    "\n\n[Older compaction input truncated to fit the model budget.]";
pub(crate) const COMPACTION_PAYLOAD_TRUNCATION_NOTICE: &str =
    "\n\n[Compaction checkpoint truncated to fit continuity budget.]";
pub(crate) const COMPACTION_MODEL_SYSTEM_PROMPT: &str = "You compress older conversation context into a deterministic execution checkpoint for future continuity. Follow the mandatory thread compaction protocol exactly. Preserve goals, constraints, decisions, tool outcomes, unresolved issues, failed paths, and the immediate next step. Return exactly one markdown block matching the required schema. Do not add commentary outside the schema.";
pub(crate) const COMPACTION_CHECKPOINT_SCHEMA: &str = r#"# 🤖 Agent Context: State Checkpoint

## 🎯 Primary Objective
> [1-2 sentences strictly defining the end goal.]

## 🗺️ Execution Map
* **✅ Completed Phase:** [...]
* **⏳ Current Phase:** [...]
* **⏭️ Pending Phases:** [...]

## 📁 Working Environment State
* **Active Directory:** `...`
* **Files Modified (Uncommitted/Pending):**
    * `...` - (...)
* **Read-Only Context Files:**
    * `...` - (...)

## 🧠 Acquired Knowledge & Constraints
* [...]

## 🚫 Dead Ends & Resolved Errors
* **Failed:** [...]
    * *Resolution:* [...]

## 🛠️ Recent Action Summary (Last 3-5 Turns)
1.  `tool_or_step(...)` -> [...]

## 🎯 Immediate Next Step
[Strict single-action instruction]
"#;
pub(crate) const COMPACTION_UNKNOWN_DIRECTORY: &str = "unknown (not captured in older context)";
pub(crate) const COMPACTION_NO_FILES_CAPTURED: &str =
    "* `none` - (No explicit file edits were captured in the compacted slice.)\n";
pub(crate) const COMPACTION_NO_READONLY_CAPTURED: &str =
    "* `none` - (No explicit reference files were captured in the compacted slice.)\n";
pub(crate) const COMPACTION_NO_DEAD_ENDS_CAPTURED: &str = "* **Failed:** No earlier failed path was preserved in this compacted slice.\n    * *Resolution:* Continue from the retained recent context instead of re-expanding discarded history.\n";
pub(crate) const CODING_COMPACTION_FALLBACK_NOTICE: &str =
    "Structured coding compaction failed; fell back to checkpoint summary.";

pub(crate) struct PreparedLlmRequest {
    pub messages: Vec<ApiMessage>,
    pub transport: ApiTransport,
    pub previous_response_id: Option<String>,
    pub upstream_thread_id: Option<String>,
    pub force_connection_close: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CompactionCandidate {
    pub split_at: usize,
    pub target_tokens: usize,
    pub trigger: CompactionTrigger,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompactionTrigger {
    MessageCount,
    TokenThreshold,
    MessageCountAndTokenThreshold,
    ManualRequest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompactionCandidateMode {
    Automatic,
    Forced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuleBasedCompactionMode {
    Conversational,
    Coding,
}

pub(crate) struct RuleBasedCompactionPayload {
    pub(crate) payload: String,
    pub(crate) structural_refs: Vec<String>,
    pub(crate) fallback_notice: Option<String>,
}

pub(crate) fn message_is_compaction_summary(message: &AgentMessage) -> bool {
    let content = message.content.trim_start();
    message.message_kind == AgentMessageKind::CompactionArtifact
        || content.starts_with("[Compacted earlier context]")
        || content.starts_with("Pre-compaction context:")
}

pub(crate) fn latest_compaction_artifact_index(messages: &[AgentMessage]) -> Option<usize> {
    messages.iter().rposition(message_is_compaction_summary)
}

pub(crate) fn active_compaction_window(messages: &[AgentMessage]) -> (usize, &[AgentMessage]) {
    match latest_compaction_artifact_index(messages) {
        Some(index) => (index, &messages[index..]),
        None => (0, messages),
    }
}

pub(crate) fn compaction_runtime_content<'a>(message: &'a AgentMessage) -> &'a str {
    if message_is_compaction_summary(message) {
        message
            .compaction_payload
            .as_deref()
            .filter(|payload| !payload.trim().is_empty())
            .unwrap_or_else(|| message.content.as_str())
    } else {
        message.content.as_str()
    }
}

pub(crate) fn materialize_compaction_message(message: &AgentMessage) -> AgentMessage {
    let mut materialized = message.clone();
    materialized.content = compaction_runtime_content(message).to_string();
    materialized
}

pub(crate) fn trailing_dangling_tool_turn_start(messages: &[AgentMessage]) -> Option<usize> {
    let (assistant_index, assistant_message) =
        messages.iter().enumerate().rev().find(|(_, message)| {
            message.role == MessageRole::Assistant
                && message
                    .tool_calls
                    .as_ref()
                    .is_some_and(|tool_calls| !tool_calls.is_empty())
        })?;

    let tool_calls = assistant_message.tool_calls.as_ref()?;
    let expected_ids: std::collections::HashSet<&str> = tool_calls
        .iter()
        .map(|tool_call| tool_call.id.as_str())
        .collect();
    if expected_ids.is_empty() {
        return None;
    }

    let trailing = &messages[assistant_index + 1..];
    if trailing
        .iter()
        .any(|message| message.role != MessageRole::Tool)
    {
        return None;
    }

    let matched_ids: std::collections::HashSet<&str> = trailing
        .iter()
        .filter_map(|message| message.tool_call_id.as_deref())
        .filter(|tool_call_id| expected_ids.contains(*tool_call_id))
        .collect();

    if !trailing.is_empty() && matched_ids.len() == expected_ids.len() {
        None
    } else {
        Some(assistant_index)
    }
}

pub(crate) fn hidden_dangling_tool_turn(
    messages: &[AgentMessage],
    window_start: usize,
) -> Vec<AgentMessage> {
    if window_start == 0 {
        return Vec::new();
    }

    let hidden_messages = &messages[..window_start];
    let Some(start) = trailing_dangling_tool_turn_start(hidden_messages) else {
        return Vec::new();
    };

    hidden_messages[start..]
        .iter()
        .map(materialize_compaction_message)
        .collect()
}

pub(crate) fn active_request_messages(messages: &[AgentMessage]) -> Vec<AgentMessage> {
    let (window_start, active_messages) = active_compaction_window(messages);
    let repaired_hidden_turn = hidden_dangling_tool_turn(messages, window_start);

    if repaired_hidden_turn.is_empty() {
        return active_messages
            .iter()
            .map(materialize_compaction_message)
            .collect();
    }

    let mut active_iter = active_messages.iter();
    let mut request_messages = Vec::new();

    if let Some(first_message) = active_iter.next() {
        request_messages.push(materialize_compaction_message(first_message));
    }

    request_messages.extend(repaired_hidden_turn);
    request_messages.extend(active_iter.map(materialize_compaction_message));
    request_messages
}
