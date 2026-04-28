//! Context compression strategies — summarize, extract key points, or semantically compress.

use std::collections::{BTreeMap, HashSet};

use super::context_item::*;
use crate::agent::APPROX_CHARS_PER_TOKEN;

/// Strategy for compressing context items.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionStrategy {
    /// Template-based summary: "The assistant executed N tool calls: [list]. Key findings: [list]"
    Summarize,
    /// Preserve only decisions, errors, and key facts.
    ExtractKeyPoints,
    /// Merge duplicate references, remove redundant tool results.
    SemanticCompress,
}

/// Result of a context compression operation.
#[derive(Debug, Clone)]
pub struct CompressionResult {
    /// The compressed content text.
    pub compressed_content: String,
    /// Estimated token count of the original items.
    pub original_tokens: u32,
    /// Estimated token count of the compressed output.
    pub compressed_tokens: u32,
    /// Number of input items processed.
    pub items_processed: usize,
    /// Which strategy was used.
    pub strategy_used: CompressionStrategy,
}

/// Compress a slice of context items using the given strategy, respecting a
/// maximum output token budget.
pub fn compress(
    items: &[ContextItem],
    strategy: CompressionStrategy,
    max_output_tokens: u32,
) -> CompressionResult {
    let original_tokens: u32 = items.iter().map(|i| i.estimated_tokens).sum();

    if items.is_empty() {
        return CompressionResult {
            compressed_content: String::new(),
            original_tokens: 0,
            compressed_tokens: 0,
            items_processed: 0,
            strategy_used: strategy,
        };
    }

    let raw = match strategy {
        CompressionStrategy::Summarize => compress_summarize(items),
        CompressionStrategy::ExtractKeyPoints => compress_extract_key_points(items),
        CompressionStrategy::SemanticCompress => compress_semantic(items),
    };

    let compressed_content = truncate_to_tokens(&raw, max_output_tokens);
    let compressed_tokens = ContextItem::estimate_tokens(&compressed_content);

    CompressionResult {
        compressed_content,
        original_tokens,
        compressed_tokens,
        items_processed: items.len(),
        strategy_used: strategy,
    }
}

/// Select the most appropriate compression strategy based on the reduction
/// ratio required.
///
/// - ratio > 3x  => [`CompressionStrategy::Summarize`] (most aggressive)
/// - ratio > 1.5x => [`CompressionStrategy::ExtractKeyPoints`]
/// - otherwise    => [`CompressionStrategy::SemanticCompress`] (least aggressive)
pub fn select_strategy(
    total_tokens: u32,
    target_tokens: u32,
    _item_count: usize,
) -> CompressionStrategy {
    if target_tokens == 0 {
        return CompressionStrategy::Summarize;
    }

    let ratio = total_tokens as f64 / target_tokens as f64;

    if ratio > 3.0 {
        CompressionStrategy::Summarize
    } else if ratio > 1.5 {
        CompressionStrategy::ExtractKeyPoints
    } else {
        CompressionStrategy::SemanticCompress
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Truncate `text` so its estimated token count is at most `max_tokens`.
fn truncate_to_tokens(text: &str, max_tokens: u32) -> String {
    let current = ContextItem::estimate_tokens(text);
    if current <= max_tokens {
        return text.to_string();
    }

    // Target char budget: subtract the fixed overhead that estimate_tokens adds
    // (4 tokens), then multiply by chars-per-token.
    let max_chars = (max_tokens.saturating_sub(4) as usize) * APPROX_CHARS_PER_TOKEN;
    let truncated: String = text.chars().take(max_chars).collect();
    truncated
}

/// Summarize strategy: count tool calls, unique tool names, message counts.
fn compress_summarize(items: &[ContextItem]) -> String {
    let mut tool_call_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut user_messages = 0usize;
    let mut assistant_messages = 0usize;
    let mut thought_count = 0usize;
    let mut artifact_count = 0usize;
    let mut file_count = 0usize;

    for item in items {
        match item.item_type {
            ContextType::ToolResult => {
                let tool_name = extract_tool_name(&item.source);
                *tool_call_counts.entry(tool_name).or_insert(0) += 1;
            }
            ContextType::Conversation => {
                if item.source == "user" {
                    user_messages += 1;
                } else {
                    assistant_messages += 1;
                }
            }
            ContextType::AgentThought => {
                thought_count += 1;
            }
            ContextType::Artifact => {
                artifact_count += 1;
            }
            ContextType::FileContent => {
                file_count += 1;
            }
            ContextType::SystemPrompt => {}
        }
    }

    let mut parts: Vec<String> = Vec::new();

    // Tool calls summary
    let total_tool_calls: usize = tool_call_counts.values().sum();
    if total_tool_calls > 0 {
        let tool_list: Vec<String> = tool_call_counts
            .iter()
            .map(|(name, count)| format!("{name} ({count})"))
            .collect();
        parts.push(format!(
            "The assistant executed {total_tool_calls} tool calls: {}.",
            tool_list.join(", ")
        ));
    }

    // Conversation summary
    if user_messages > 0 || assistant_messages > 0 {
        parts.push(format!(
            "Conversation: {user_messages} user messages, {assistant_messages} assistant messages."
        ));
    }

    // Thoughts, artifacts, files
    if thought_count > 0 {
        parts.push(format!("{thought_count} agent thoughts recorded."));
    }
    if artifact_count > 0 {
        parts.push(format!("{artifact_count} artifacts generated."));
    }
    if file_count > 0 {
        parts.push(format!("{file_count} files read into context."));
    }

    // Extract key findings from tool results (first line of each, deduped)
    let mut findings: Vec<String> = Vec::new();
    let mut seen_findings: HashSet<String> = HashSet::new();
    for item in items {
        if item.item_type == ContextType::ToolResult {
            let first_line = item.content.lines().next().unwrap_or("").trim().to_string();
            if !first_line.is_empty() && seen_findings.insert(first_line.clone()) {
                findings.push(first_line);
            }
        }
    }
    if !findings.is_empty() {
        let capped: Vec<&String> = findings.iter().take(5).collect();
        let listing: Vec<String> = capped.iter().map(|f| format!("  - {f}")).collect();
        parts.push(format!("Key findings:\n{}", listing.join("\n")));
    }

    parts.join(" ")
}

/// Extract the tool name from a source string like "tool:bash_command".
fn extract_tool_name(source: &str) -> String {
    if let Some(stripped) = source.strip_prefix("tool:") {
        stripped.to_string()
    } else {
        source.to_string()
    }
}

/// Key-terms used by the ExtractKeyPoints strategy.
const KEY_TERMS: &[&str] = &["error", "fail", "decide", "chose", "result"];

/// ExtractKeyPoints strategy: keep only items that contain key terms or are
/// agent thoughts.
fn compress_extract_key_points(items: &[ContextItem]) -> String {
    let mut key_points: Vec<String> = Vec::new();

    for item in items {
        let dominated_by_type = item.item_type == ContextType::AgentThought;
        let content_lower = item.content.to_lowercase();
        let has_key_term = KEY_TERMS.iter().any(|term| content_lower.contains(term));

        if dominated_by_type || has_key_term {
            let first_meaningful = item
                .content
                .lines()
                .find(|line| !line.trim().is_empty())
                .unwrap_or("")
                .trim();
            key_points.push(format!("Key point: {first_meaningful}"));
        }
    }

    key_points.join("\n")
}

/// SemanticCompress strategy: group by source, deduplicate exact content,
/// keep only the most recent of each duplicate group.
fn compress_semantic(items: &[ContextItem]) -> String {
    // Group items by source.
    let mut groups: BTreeMap<String, Vec<&ContextItem>> = BTreeMap::new();
    for item in items {
        groups.entry(item.source.clone()).or_default().push(item);
    }

    let mut output_parts: Vec<String> = Vec::new();

    for (source, group_items) in &groups {
        // Deduplicate: for items with identical content keep only the most
        // recent (highest timestamp).
        let mut seen_content: BTreeMap<&str, &ContextItem> = BTreeMap::new();
        for item in group_items {
            let entry = seen_content.entry(item.content.as_str()).or_insert(item);
            if item.timestamp > entry.timestamp {
                *entry = item;
            }
        }

        // Collect deduplicated items sorted by timestamp.
        let mut deduped: Vec<&&ContextItem> = seen_content.values().collect();
        deduped.sort_by_key(|i| i.timestamp);

        let lines: Vec<&str> = deduped.iter().map(|i| i.content.as_str()).collect();
        if !lines.is_empty() {
            output_parts.push(format!("[{source}]\n{}", lines.join("\n")));
        }
    }

    output_parts.join("\n\n")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "compression/tests.rs"]
mod tests;
