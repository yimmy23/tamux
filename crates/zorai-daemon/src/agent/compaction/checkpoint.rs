use super::*;

pub(crate) fn build_compaction_summary(messages: &[AgentMessage], target_tokens: usize) -> String {
    build_compaction_summary_with_scope(messages, target_tokens, None)
}

pub(crate) fn build_compaction_summary_with_scope(
    messages: &[AgentMessage],
    target_tokens: usize,
    scope: Option<&CompactionScopeSnapshot>,
) -> String {
    if messages.is_empty() {
        return String::new();
    }

    let scoped_messages = materialize_compaction_messages_with_scope(messages, scope);
    let messages = scoped_messages.as_slice();

    let scope_packet = render_compaction_scope_packet(scope)
        .map(|packet| format!("## 🔎 Scope Identity\n{packet}\n\n"))
        .unwrap_or_default();
    let primary_objective = checkpoint_primary_objective_with_scope(messages, scope);
    let completed_phase = checkpoint_completed_phase(messages);
    let current_phase = checkpoint_current_phase(messages);
    let pending_phases = checkpoint_pending_phases(messages);
    let active_directory = checkpoint_active_directory(messages)
        .unwrap_or_else(|| COMPACTION_UNKNOWN_DIRECTORY.to_string());
    let files_modified = checkpoint_files_modified(messages);
    let read_only_context = checkpoint_read_only_context(messages);
    let acquired_knowledge = checkpoint_acquired_knowledge(messages);
    let dead_ends = checkpoint_dead_ends(messages);
    let recent_actions = checkpoint_recent_actions(messages);
    let tool_evidence_pointers = checkpoint_tool_evidence_pointers(messages);
    let immediate_next_step = checkpoint_immediate_next_step(messages);

    let summary = format!(
        "# 🤖 Agent Context: State Checkpoint\n\n{}## 🎯 Primary Objective\n> {}\n\n## 🗺️ Execution Map\n* **✅ Completed Phase:** {}\n* **⏳ Current Phase:** {}\n* **⏭️ Pending Phases:** {}\n\n## 📁 Working Environment State\n* **Active Directory:** `{}`\n* **Files Modified (Uncommitted/Pending):**\n{}* **Read-Only Context Files:**\n{}## 🧠 Acquired Knowledge & Constraints\n{}## 🚫 Dead Ends & Resolved Errors\n{}## 🛠️ Recent Action Summary (Last 3-5 Turns)\n{}\n## 🔗 Tool Evidence Pointers\n{}## 🎯 Immediate Next Step\n{}\n",
        scope_packet,
        primary_objective,
        completed_phase,
        current_phase,
        pending_phases,
        active_directory,
        files_modified,
        read_only_context,
        acquired_knowledge,
        dead_ends,
        recent_actions,
        tool_evidence_pointers,
        immediate_next_step,
    );

    fit_compaction_payload_to_budget(summary, target_tokens).0
}

pub(crate) fn summarize_compacted_message(message: &AgentMessage) -> String {
    let role = match message.role {
        MessageRole::System => "SYSTEM",
        MessageRole::User => "USER",
        MessageRole::Assistant => "ASSISTANT",
        MessageRole::Tool => "TOOL",
    };

    let mut details = String::new();
    if let Some(name) = message
        .tool_name
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        details.push_str(name);
        if let Some(status) = message
            .tool_status
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            details.push(' ');
            details.push_str(status);
        }
    } else if let Some(tool_calls) = &message.tool_calls {
        let names = tool_calls
            .iter()
            .map(|call| call.function.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        if !names.is_empty() {
            details.push_str(&format!("calls: {names}"));
        }
    }

    let content =
        super::goal_parsing::summarize_text(compaction_summary_content(message).as_ref(), 160);
    if details.is_empty() {
        format!("{role}: {content}")
    } else {
        format!("{role} [{details}]: {content}")
    }
}

pub(crate) fn checkpoint_primary_objective(messages: &[AgentMessage]) -> String {
    checkpoint_primary_objective_with_scope(messages, None)
}

pub(crate) fn checkpoint_primary_objective_with_scope(
    messages: &[AgentMessage],
    scope: Option<&CompactionScopeSnapshot>,
) -> String {
    if let Some(scope) = scope {
        let title = scope
            .goal_title
            .as_deref()
            .or(scope.goal_run_id.as_deref())
            .unwrap_or("active goal");
        if let Some(goal) = scope
            .goal
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            return format!(
                "{}: {}",
                title,
                super::goal_parsing::summarize_text(goal, 220)
            );
        }
        if let Some(step) = scope
            .current_step_title
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            return format!("{title}: continue current step `{step}`.");
        }
    }

    let first_user = messages
        .iter()
        .find(|message| message.role == MessageRole::User)
        .map(|message| {
            super::goal_parsing::summarize_text(compaction_summary_content(message).as_ref(), 180)
        });
    let latest_user = messages
        .iter()
        .rev()
        .find(|message| message.role == MessageRole::User)
        .map(|message| {
            super::goal_parsing::summarize_text(compaction_summary_content(message).as_ref(), 180)
        });

    match (first_user, latest_user) {
        (Some(first), Some(latest)) if first != latest => {
            format!("{} Latest carried-forward ask: {}", first, latest)
        }
        (Some(first), _) => first,
        (_, Some(latest)) => latest,
        _ => "Continue the active workstream using the retained recent context and preserved checkpoint facts.".to_string(),
    }
}

pub(crate) fn checkpoint_completed_phase(messages: &[AgentMessage]) -> String {
    messages
        .iter()
        .rev()
        .find(|message| message.role != MessageRole::User)
        .map(|message| format!("Captured prior progress: {}", summarize_compacted_message(message)))
        .unwrap_or_else(|| "Captured the earlier slice of conversation so the active work can continue without replaying raw history.".to_string())
}

pub(crate) fn checkpoint_current_phase(messages: &[AgentMessage]) -> String {
    messages
        .last()
        .map(|message| match message.role {
            MessageRole::User => format!(
                "Resume from the latest carried-forward user request: {}",
                super::goal_parsing::summarize_text(
                    compaction_summary_content(message).as_ref(),
                    180,
                )
            ),
            MessageRole::Assistant => format!(
                "Continue from the latest assistant state: {}",
                super::goal_parsing::summarize_text(
                    compaction_summary_content(message).as_ref(),
                    180,
                )
            ),
            MessageRole::Tool => format!(
                "Continue after the last tool outcome: {}",
                summarize_compacted_message(message)
            ),
            MessageRole::System => format!(
                "Honor the preserved system guidance: {}",
                super::goal_parsing::summarize_text(
                    compaction_summary_content(message).as_ref(),
                    180,
                )
            ),
        })
        .unwrap_or_else(|| {
            "Resume execution from the retained recent context without replaying the discarded raw history.".to_string()
        })
}

pub(crate) fn checkpoint_pending_phases(messages: &[AgentMessage]) -> String {
    let latest_user = messages
        .iter()
        .rev()
        .find(|message| message.role == MessageRole::User)
        .map(|message| {
            super::goal_parsing::summarize_text(compaction_summary_content(message).as_ref(), 140)
        });
    match latest_user {
        Some(latest_user) => format!(
            "Continue the active request, validate the affected slice, and close any unresolved risks around: {}",
            latest_user
        ),
        None => "Continue the active task, validate the affected slice, and surface any unresolved risks before expanding scope.".to_string(),
    }
}

pub(crate) fn checkpoint_active_directory(messages: &[AgentMessage]) -> Option<String> {
    messages
        .iter()
        .rev()
        .find_map(|message| extract_labeled_path(compaction_summary_content(message).as_ref()))
}

pub(crate) fn checkpoint_files_modified(messages: &[AgentMessage]) -> String {
    let files = collect_context_paths(messages, true);
    if files.is_empty() {
        return COMPACTION_NO_FILES_CAPTURED.to_string();
    }

    files
        .into_iter()
        .map(|file| format!("* `{file}` - (Referenced as part of the active compacted work.)\n"))
        .collect()
}

pub(crate) fn checkpoint_read_only_context(messages: &[AgentMessage]) -> String {
    let files = collect_context_paths(messages, false);
    if files.is_empty() {
        return COMPACTION_NO_READONLY_CAPTURED.to_string();
    }

    files
        .into_iter()
        .map(|file| format!("* `{file}` - (Context referenced in the compacted history.)\n"))
        .collect()
}

pub(crate) fn checkpoint_acquired_knowledge(messages: &[AgentMessage]) -> String {
    let items = messages
        .iter()
        .filter(|message| message.role != MessageRole::System)
        .map(|message| summarize_compacted_message(message))
        .filter(|summary| !summary.trim().is_empty())
        .collect::<Vec<_>>();

    unique_bullets(
        &items,
        4,
        "Continue from the retained recent context; no additional older constraints were preserved in this slice.",
    )
}

pub(crate) fn checkpoint_dead_ends(messages: &[AgentMessage]) -> String {
    let dead_ends = messages
        .iter()
        .filter_map(|message| {
            let content = compaction_summary_content(message);
            let lowered = content.to_ascii_lowercase();
            let is_failure = lowered.contains("error")
                || lowered.contains("failed")
                || lowered.contains("timeout")
                || lowered.contains("blocked")
                || lowered.contains("unsupported");
            is_failure.then(|| {
                format!(
                    "* **Failed:** {}\n    * *Resolution:* Preserve the failure and avoid replaying the discarded path without new evidence.\n",
                    summarize_compacted_message(message)
                )
            })
        })
        .take(3)
        .collect::<String>();

    if dead_ends.is_empty() {
        COMPACTION_NO_DEAD_ENDS_CAPTURED.to_string()
    } else {
        dead_ends
    }
}

pub(crate) fn checkpoint_recent_actions(messages: &[AgentMessage]) -> String {
    let actions = messages.iter().rev().take(5).collect::<Vec<_>>();
    let mut ordered = actions;
    ordered.reverse();

    ordered
        .into_iter()
        .enumerate()
        .map(|(index, message)| {
            let action = match message.role {
                MessageRole::Tool => format!(
                    "{}({})",
                    message.tool_name.as_deref().unwrap_or("tool"),
                    super::goal_parsing::summarize_text(
                        message.tool_arguments.as_deref().unwrap_or("{}"),
                        80,
                    )
                ),
                MessageRole::Assistant => "assistant_step(...)".to_string(),
                MessageRole::User => "user_request(...)".to_string(),
                MessageRole::System => "system_context(...)".to_string(),
            };
            format!(
                "{}. `{}` -> {}\n",
                index + 1,
                action,
                summarize_compacted_message(message)
            )
        })
        .collect()
}

pub(crate) fn checkpoint_tool_evidence_pointers(messages: &[AgentMessage]) -> String {
    let pointers = messages
        .iter()
        .filter(|message| message.role == MessageRole::Tool)
        .filter_map(|message| {
            let content = compaction_runtime_content(message);
            content
                .starts_with("Tool Evidence Pointer")
                .then(|| super::goal_parsing::summarize_text(content, 260))
        })
        .collect::<Vec<_>>();

    unique_bullets(
        &pointers,
        4,
        "No offloaded or broad tool payload pointers were captured in this compacted slice.",
    )
}

pub(crate) fn checkpoint_immediate_next_step(messages: &[AgentMessage]) -> String {
    messages
        .iter()
        .rev()
        .find(|message| message.role == MessageRole::User)
        .map(|message| {
            format!(
                "Answer the latest carried-forward user request: {}",
                super::goal_parsing::summarize_text(
                    compaction_summary_content(message).as_ref(),
                    180,
                )
            )
        })
        .unwrap_or_else(|| {
            "Read the retained recent messages and continue the active task without replaying discarded history.".to_string()
        })
}

pub(crate) fn unique_bullets(items: &[String], max_items: usize, fallback: &str) -> String {
    let mut deduped = Vec::new();
    for item in items {
        if deduped.iter().any(|existing: &String| existing == item) {
            continue;
        }
        deduped.push(item.clone());
        if deduped.len() >= max_items {
            break;
        }
    }

    if deduped.is_empty() {
        return format!("* {}\n", fallback);
    }

    deduped
        .into_iter()
        .map(|item| format!("* {}\n", item))
        .collect()
}

pub(crate) fn collect_context_paths(
    messages: &[AgentMessage],
    prefer_modified: bool,
) -> Vec<String> {
    let mut paths = Vec::new();
    for message in messages {
        let content = compaction_summary_content(message);
        if let Some(path) = extract_labeled_path(content.as_ref()) {
            if !paths.iter().any(|existing| existing == &path) {
                paths.push(path);
            }
        }

        let tool_name = message.tool_name.as_deref().unwrap_or_default();
        let modified_tool = matches!(
            tool_name,
            zorai_protocol::tool_names::WRITE_FILE
                | zorai_protocol::tool_names::CREATE_FILE
                | zorai_protocol::tool_names::APPLY_PATCH
                | "rename"
                | "delete"
        );
        if modified_tool != prefer_modified {
            continue;
        }
        if let Some(path) =
            extract_path_token(message.tool_arguments.as_deref().unwrap_or_default())
        {
            if !paths.iter().any(|existing| existing == &path) {
                paths.push(path);
            }
        }
        if paths.len() >= 3 {
            break;
        }
    }
    paths
}

pub(crate) fn extract_labeled_path(text: &str) -> Option<String> {
    for label in [
        "Working directory:",
        "working directory:",
        "Active Directory:",
        "active directory:",
        "Dir:",
        "dir:",
        "Cwd:",
        "cwd:",
    ] {
        if let Some(index) = text.find(label) {
            let remainder = text[index + label.len()..].trim_start();
            if let Some(path) = extract_path_token(remainder) {
                return Some(path);
            }
        }
    }
    None
}

pub(crate) fn extract_path_token(text: &str) -> Option<String> {
    let trimmed = text.trim_start();
    let trimmed = trimmed.strip_prefix('`').unwrap_or(trimmed);
    let mut path = String::new();
    for ch in trimmed.chars() {
        if ch.is_whitespace() || matches!(ch, ',' | ';' | ')' | ']' | '|' | '*') {
            break;
        }
        path.push(ch);
    }
    let path = path
        .trim_matches(|ch| matches!(ch, '`' | '"' | '\''))
        .to_string();
    path.starts_with('/').then_some(path)
}
