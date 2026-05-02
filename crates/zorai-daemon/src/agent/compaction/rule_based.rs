use super::*;

pub(crate) fn build_checkpoint_compaction_payload(
    messages: &[AgentMessage],
    target_tokens: usize,
) -> String {
    let summary = build_compaction_summary(messages, target_tokens);
    if summary.trim().is_empty() {
        "Older context compacted for continuity.".to_string()
    } else {
        summary
    }
}

pub(crate) fn determine_rule_based_compaction_mode(
    structural_memory: Option<&ThreadStructuralMemory>,
    messages: &[AgentMessage],
) -> RuleBasedCompactionMode {
    if structural_memory.is_none_or(|memory| !memory.has_structural_nodes()) {
        return RuleBasedCompactionMode::Conversational;
    }

    let recent_messages = messages
        .iter()
        .rev()
        .take(COMPACTION_RECENT_SIGNAL_MESSAGES);
    for message in recent_messages {
        if message_uses_coding_tool(message) || message_contains_coding_signal(message) {
            return RuleBasedCompactionMode::Coding;
        }
    }

    RuleBasedCompactionMode::Conversational
}

pub(crate) fn message_uses_coding_tool(message: &AgentMessage) -> bool {
    message
        .tool_name
        .as_deref()
        .is_some_and(is_coding_tool_name)
        || message.tool_calls.as_ref().is_some_and(|tool_calls| {
            tool_calls
                .iter()
                .any(|call| is_coding_tool_name(call.function.name.as_str()))
        })
}

pub(crate) fn is_coding_tool_name(tool_name: &str) -> bool {
    matches!(
        tool_name,
        "read_file"
            | "replace_in_file"
            | "apply_patch"
            | "create_file"
            | "list_files"
            | "list_dir"
            | "write_file"
            | "append_to_file"
            | "apply_file_patch"
    )
}

pub(crate) fn message_contains_coding_signal(message: &AgentMessage) -> bool {
    text_contains_coding_signal(compaction_runtime_content(message))
        || message
            .tool_arguments
            .as_deref()
            .is_some_and(text_contains_coding_signal)
}

pub(crate) fn text_contains_coding_signal(text: &str) -> bool {
    let lowered = text.to_ascii_lowercase();
    if text.contains("```")
        || text.contains("*** Begin Patch")
        || text.contains("diff --git")
        || text.contains("\n@@")
        || text
            .lines()
            .any(|line| line.starts_with("+++ ") || line.starts_with("--- "))
    {
        return true;
    }

    if [
        "error[",
        "test result:",
        "assertion failed",
        "failures:",
        "cargo test",
        "cargo check",
        "npm test",
        "build failed",
        "compiling ",
    ]
    .iter()
    .any(|needle| lowered.contains(needle))
    {
        return true;
    }

    contains_path_like_token(text)
}

pub(crate) fn contains_path_like_token(text: &str) -> bool {
    const CODE_EXTENSIONS: &[&str] = &[
        ".rs", ".toml", ".ts", ".tsx", ".js", ".jsx", ".py", ".json", ".md", ".yaml", ".yml",
        ".cjs", ".mjs",
    ];

    text.split_whitespace().any(|token| {
        let token = token.trim_matches(|ch: char| {
            matches!(
                ch,
                '`' | '"' | '\'' | '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';' | ':'
            )
        });
        if token.starts_with('/') && token.len() > 1 {
            return true;
        }
        token.contains('/')
            && CODE_EXTENSIONS
                .iter()
                .any(|extension| token.contains(extension))
    })
}

pub(crate) fn coding_execution_map(messages: &[AgentMessage]) -> String {
    let active_directory = checkpoint_active_directory(messages)
        .unwrap_or_else(|| COMPACTION_UNKNOWN_DIRECTORY.to_string());
    format!(
        "- Completed: {}\n- Current: {}\n- Pending: {}\n- Active directory: `{}`",
        checkpoint_completed_phase(messages),
        checkpoint_current_phase(messages),
        checkpoint_pending_phases(messages),
        active_directory,
    )
}

pub(crate) fn collect_message_structural_refs(messages: &[AgentMessage]) -> Vec<String> {
    let mut refs = Vec::new();
    for message in messages {
        for structural_ref in &message.structural_refs {
            if !refs.iter().any(|existing| existing == structural_ref) {
                refs.push(structural_ref.clone());
            }
        }
    }
    refs
}

pub(crate) fn render_structural_context(entries: &[StructuralContextEntry]) -> String {
    if entries.is_empty() {
        return "- none (No structural nodes were available for this compacted slice.)\n"
            .to_string();
    }

    entries
        .iter()
        .map(|entry| {
            format!(
                "- `{}` - {}\n",
                entry.node_id,
                crate::agent::goal_parsing::summarize_text(entry.summary.as_str(), 220)
            )
        })
        .collect()
}

pub(crate) async fn load_memory_graph_neighbors(
    history: &crate::history::HistoryStore,
    structural_refs: &[String],
    limit: usize,
) -> Result<Vec<MemoryGraphNeighborRow>> {
    let mut neighbors = Vec::new();
    for node_id in structural_refs.iter().take(limit) {
        let remaining = limit.saturating_sub(neighbors.len());
        if remaining == 0 {
            break;
        }
        let rows = history
            .list_memory_graph_neighbors(node_id, remaining)
            .await?;
        for row in rows {
            if neighbors.iter().any(|existing: &MemoryGraphNeighborRow| {
                existing.node.id == row.node.id
                    && existing.via_edge.source_node_id == row.via_edge.source_node_id
                    && existing.via_edge.target_node_id == row.via_edge.target_node_id
                    && existing.via_edge.relation_type == row.via_edge.relation_type
            }) {
                continue;
            }
            neighbors.push(row);
            if neighbors.len() >= limit {
                break;
            }
        }
    }
    Ok(neighbors)
}

pub(crate) fn graph_neighbor_summary(row: &MemoryGraphNeighborRow) -> String {
    let relation = row.via_edge.relation_type.replace('_', " ");
    let anchor = if row.via_edge.source_node_id == row.node.id {
        row.via_edge.target_node_id.as_str()
    } else {
        row.via_edge.source_node_id.as_str()
    };
    let summary = row
        .node
        .summary_text
        .as_deref()
        .filter(|value: &&str| !value.trim().is_empty())
        .map(|value| {
            format!(
                "; {}",
                crate::agent::goal_parsing::summarize_text(value, 140)
            )
        })
        .unwrap_or_default();
    format!(
        "graph neighbor `{}` ({}) via {} from `{}`{}",
        row.node.label, row.node.node_type, relation, anchor, summary
    )
}

pub(crate) fn merge_structural_context_entries(
    structural_entries: &[StructuralContextEntry],
    graph_neighbors: &[MemoryGraphNeighborRow],
    limit: usize,
) -> Vec<StructuralContextEntry> {
    let mut entries = structural_entries.to_vec();
    for neighbor in graph_neighbors {
        if entries.len() >= limit {
            break;
        }
        if entries
            .iter()
            .any(|entry| entry.node_id == neighbor.node.id)
        {
            continue;
        }
        entries.push(StructuralContextEntry {
            node_id: neighbor.node.id.clone(),
            summary: graph_neighbor_summary(neighbor),
        });
    }
    entries
}

pub(crate) fn collect_referenced_offloaded_payload_ids(messages: &[AgentMessage]) -> Vec<String> {
    let mut payload_ids = Vec::new();
    for message in messages {
        let Some(payload_id) = message.offloaded_payload_id.as_deref() else {
            continue;
        };
        if !payload_ids.iter().any(|existing| existing == payload_id) {
            payload_ids.push(payload_id.to_string());
        }
    }
    payload_ids
}

pub(crate) fn summarize_offloaded_metadata_summary(summary: &str) -> String {
    let normalized = summary.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        "summary unavailable".to_string()
    } else {
        crate::agent::goal_parsing::summarize_text(normalized.as_str(), 220)
    }
}

pub(crate) fn render_offloaded_payload_references(
    metadata_rows: &[crate::history::OffloadedPayloadMetadataRow],
) -> String {
    let mut rendered = String::new();

    for metadata in metadata_rows {
        rendered.push_str(&format!(
            "- `{}` (`{}`, {} bytes) - {}\n",
            metadata.payload_id,
            metadata.tool_name,
            metadata.byte_size,
            summarize_offloaded_metadata_summary(metadata.summary.as_str())
        ));
    }

    if rendered.is_empty() {
        "- none (No referenced offloaded payload metadata was available for this compacted slice.)\n"
            .to_string()
    } else {
        rendered
    }
}

pub(crate) async fn load_referenced_offloaded_payload_metadata(
    history: &crate::history::HistoryStore,
    thread_id: &str,
    messages: &[AgentMessage],
) -> Result<Vec<crate::history::OffloadedPayloadMetadataRow>> {
    let mut rows = Vec::new();

    for payload_id in collect_referenced_offloaded_payload_ids(messages)
        .into_iter()
        .take(CODING_COMPACTION_OFFLOAD_REFERENCE_LIMIT)
    {
        let Some(metadata) = history
            .get_offloaded_payload_metadata(payload_id.as_str())
            .await?
        else {
            continue;
        };
        if metadata.thread_id == thread_id {
            rows.push(metadata);
        }
    }

    Ok(rows)
}

pub(crate) fn compaction_payload_max_chars(target_tokens: usize) -> usize {
    (target_tokens / 4)
        .saturating_mul(APPROX_CHARS_PER_TOKEN)
        .clamp(4096, 8192)
}

pub(crate) fn fit_compaction_payload_to_budget(
    payload: String,
    target_tokens: usize,
) -> (String, bool) {
    let max_chars = compaction_payload_max_chars(target_tokens);
    if payload.chars().count() <= max_chars {
        return (payload, false);
    }

    let notice_chars = COMPACTION_PAYLOAD_TRUNCATION_NOTICE.chars().count();
    let retained_chars = max_chars.saturating_sub(notice_chars).max(1024);
    let retained = payload
        .chars()
        .take(retained_chars)
        .collect::<String>()
        .trim_end()
        .to_string();

    (
        format!("{retained}{COMPACTION_PAYLOAD_TRUNCATION_NOTICE}"),
        true,
    )
}

pub(crate) fn coding_compaction_payload_max_chars(target_tokens: usize) -> usize {
    compaction_payload_max_chars(target_tokens)
}

pub(crate) fn merge_compaction_fallback_notice(
    primary: Option<String>,
    secondary: Option<String>,
) -> Option<String> {
    match (primary, secondary) {
        (Some(primary), Some(secondary)) if primary == secondary => Some(primary),
        (Some(primary), Some(secondary)) => Some(format!("{primary} {secondary}")),
        (Some(primary), None) => Some(primary),
        (None, Some(secondary)) => Some(secondary),
        (None, None) => None,
    }
}
