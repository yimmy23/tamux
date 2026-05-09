use super::*;

/// Helper to build a [`ContextItem`] with sensible defaults.
fn make_item(
    id: &str,
    item_type: ContextType,
    content: &str,
    source: &str,
    timestamp: u64,
) -> ContextItem {
    ContextItem {
        id: id.into(),
        item_type,
        content: content.into(),
        timestamp,
        relevance_score: 0.0,
        access_count: 0,
        source: source.into(),
        estimated_tokens: ContextItem::estimate_tokens(content),
    }
}

#[test]
fn summarize_produces_tool_call_summary() {
    let items = vec![
        make_item(
            "t1",
            ContextType::ToolResult,
            "File created",
            "tool:bash_command",
            1,
        ),
        make_item(
            "t2",
            ContextType::ToolResult,
            "Listing dirs",
            "tool:bash_command",
            2,
        ),
        make_item(
            "t3",
            ContextType::ToolResult,
            "Search result",
            "tool:grep",
            3,
        ),
    ];

    let result = compress(&items, CompressionStrategy::Summarize, 1000);

    assert!(result.compressed_content.contains("3 tool calls"));
    assert!(result.compressed_content.contains("bash_command (2)"));
    assert!(result.compressed_content.contains("grep (1)"));
    assert_eq!(result.strategy_used, CompressionStrategy::Summarize);
}

#[test]
fn extract_key_points_filters_for_key_terms() {
    let items = vec![
        make_item("a", ContextType::Conversation, "Hello world", "user", 1),
        make_item(
            "b",
            ContextType::ToolResult,
            "error: file not found",
            "tool:read",
            2,
        ),
        make_item(
            "c",
            ContextType::Conversation,
            "I decided to try again",
            "user",
            3,
        ),
        make_item("d", ContextType::Conversation, "How are you?", "user", 4),
    ];

    let result = compress(&items, CompressionStrategy::ExtractKeyPoints, 1000);

    assert!(result.compressed_content.contains("Key point:"));
    assert!(result.compressed_content.contains("error: file not found"));
    assert!(!result.compressed_content.contains("Hello world"));
    assert!(!result.compressed_content.contains("How are you?"));
}

#[test]
fn semantic_compress_deduplicates() {
    let items = vec![
        make_item("a", ContextType::ToolResult, "same content", "tool:bash", 1),
        make_item("b", ContextType::ToolResult, "same content", "tool:bash", 5),
        make_item(
            "c",
            ContextType::ToolResult,
            "different content",
            "tool:bash",
            3,
        ),
    ];

    let result = compress(&items, CompressionStrategy::SemanticCompress, 1000);

    assert_eq!(result.compressed_content.matches("same content").count(), 1);
    assert!(result.compressed_content.contains("different content"));
}

#[test]
fn compression_result_tracks_token_counts() {
    let items = vec![
        make_item(
            "a",
            ContextType::Conversation,
            "Hello world, this is a long message to test token counting",
            "user",
            1,
        ),
        make_item(
            "b",
            ContextType::Conversation,
            "Another message here",
            "assistant",
            2,
        ),
    ];

    let original_total: u32 = items.iter().map(|i| i.estimated_tokens).sum();
    let result = compress(&items, CompressionStrategy::Summarize, 1000);

    assert_eq!(result.original_tokens, original_total);
    assert!(result.compressed_tokens > 0);
    assert_eq!(result.items_processed, 2);
}

#[test]
fn select_strategy_chooses_based_on_ratios() {
    assert_eq!(
        select_strategy(4000, 1000, 10),
        CompressionStrategy::Summarize
    );
    assert_eq!(
        select_strategy(2000, 1000, 10),
        CompressionStrategy::ExtractKeyPoints
    );
    assert_eq!(
        select_strategy(1200, 1000, 10),
        CompressionStrategy::SemanticCompress
    );
}

#[test]
fn empty_items_produces_empty_result() {
    let result = compress(&[], CompressionStrategy::Summarize, 1000);

    assert!(result.compressed_content.is_empty());
    assert_eq!(result.original_tokens, 0);
    assert_eq!(result.compressed_tokens, 0);
    assert_eq!(result.items_processed, 0);
}

#[test]
fn compression_ratio_is_calculated_correctly() {
    let long_content = "x".repeat(200);
    let items = vec![
        make_item("a", ContextType::ToolResult, &long_content, "tool:bash", 1),
        make_item("b", ContextType::ToolResult, &long_content, "tool:bash", 2),
        make_item("c", ContextType::ToolResult, &long_content, "tool:grep", 3),
        make_item("d", ContextType::Conversation, &long_content, "user", 4),
        make_item(
            "e",
            ContextType::Conversation,
            &long_content,
            "assistant",
            5,
        ),
    ];

    let original_total: u32 = items.iter().map(|i| i.estimated_tokens).sum();
    let result = compress(&items, CompressionStrategy::Summarize, 1000);

    assert_eq!(result.original_tokens, original_total);
    assert_eq!(
        result.compressed_tokens,
        ContextItem::estimate_tokens(&result.compressed_content),
    );
    assert!(
        result.compressed_tokens < result.original_tokens,
        "compressed {} should be < original {}",
        result.compressed_tokens,
        result.original_tokens,
    );
}

#[test]
fn max_output_tokens_respected() {
    let items = vec![
        make_item("a", ContextType::Conversation, &"x".repeat(400), "user", 1),
        make_item(
            "b",
            ContextType::Conversation,
            &"y".repeat(400),
            "assistant",
            2,
        ),
    ];

    let result = compress(&items, CompressionStrategy::SemanticCompress, 10);

    assert!(
        result.compressed_tokens <= 10,
        "compressed_tokens {} should be <= 10",
        result.compressed_tokens,
    );
}

#[test]
fn different_item_types_are_handled() {
    let items = vec![
        make_item(
            "a",
            ContextType::SystemPrompt,
            "You are a helpful assistant",
            "system",
            1,
        ),
        make_item(
            "b",
            ContextType::AgentThought,
            "I should search for the file",
            "agent",
            2,
        ),
        make_item("c", ContextType::Artifact, "fn main() {}", "artifact", 3),
        make_item(
            "d",
            ContextType::FileContent,
            "line 1\nline 2",
            "file:/tmp/x.rs",
            4,
        ),
        make_item("e", ContextType::Conversation, "Hello", "user", 5),
        make_item("f", ContextType::ToolResult, "OK", "tool:bash", 6),
    ];

    let r1 = compress(&items, CompressionStrategy::Summarize, 1000);
    let r2 = compress(&items, CompressionStrategy::ExtractKeyPoints, 1000);
    let r3 = compress(&items, CompressionStrategy::SemanticCompress, 1000);

    assert_eq!(r1.items_processed, 6);
    assert_eq!(r2.items_processed, 6);
    assert_eq!(r3.items_processed, 6);

    assert!(r1.compressed_content.contains("1 tool calls"));
    assert!(r1.compressed_content.len() > 0);
    assert!(r2.compressed_content.contains("Key point:"));
    assert!(r3.compressed_content.contains("[system]"));
}

#[test]
fn mixed_strategy_selection_boundaries() {
    assert_eq!(
        select_strategy(3000, 1000, 5),
        CompressionStrategy::ExtractKeyPoints,
    );

    assert_eq!(
        select_strategy(1500, 1000, 5),
        CompressionStrategy::SemanticCompress,
    );

    assert_eq!(
        select_strategy(3001, 1000, 5),
        CompressionStrategy::Summarize,
    );

    assert_eq!(
        select_strategy(1501, 1000, 5),
        CompressionStrategy::ExtractKeyPoints,
    );

    assert_eq!(select_strategy(5000, 0, 10), CompressionStrategy::Summarize,);

    assert_eq!(
        select_strategy(1000, 1000, 10),
        CompressionStrategy::SemanticCompress,
    );
}

#[test]
fn extract_key_points_includes_agent_thoughts() {
    let items = vec![
        make_item(
            "a",
            ContextType::AgentThought,
            "Planning next steps",
            "agent",
            1,
        ),
        make_item("b", ContextType::Conversation, "Ordinary chat", "user", 2),
    ];

    let result = compress(&items, CompressionStrategy::ExtractKeyPoints, 1000);

    assert!(result.compressed_content.contains("Planning next steps"));
    assert!(!result.compressed_content.contains("Ordinary chat"));
}

#[test]
fn semantic_compress_keeps_most_recent_duplicate() {
    let items = vec![
        make_item("old", ContextType::ToolResult, "dup content", "tool:x", 10),
        make_item("new", ContextType::ToolResult, "dup content", "tool:x", 99),
    ];

    let result = compress(&items, CompressionStrategy::SemanticCompress, 1000);

    assert_eq!(result.compressed_content.matches("dup content").count(), 1);
}
