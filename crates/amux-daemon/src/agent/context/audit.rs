//! Context audit — categorize and rank context items by relevance.

use super::context_item::*;

/// Audit all context items: compute relevance scores, categorize each item,
/// tally counts and tokens per category, and return a ranked report.
pub fn context_audit(
    items: &mut [ContextItem],
    now: u64,
    recent_threshold_ms: u64,
    max_age_ms: u64,
) -> ContextAuditReport {
    let mut critical_count: usize = 0;
    let mut active_count: usize = 0;
    let mut dormant_count: usize = 0;
    let mut archivable_count: usize = 0;

    let mut critical_tokens: u32 = 0;
    let mut active_tokens: u32 = 0;
    let mut dormant_tokens: u32 = 0;
    let mut archivable_tokens: u32 = 0;

    let mut total_tokens: u32 = 0;

    let mut ranked_items: Vec<(String, RelevanceCategory, f64)> = Vec::with_capacity(items.len());

    for item in items.iter_mut() {
        // Compute and store the relevance score.
        item.relevance_score = item.compute_relevance(now, max_age_ms);

        // Categorize based on the freshly computed score.
        let category = item.categorize(now, recent_threshold_ms);

        let tokens = item.estimated_tokens;
        total_tokens += tokens;

        match category {
            RelevanceCategory::Critical => {
                critical_count += 1;
                critical_tokens += tokens;
            }
            RelevanceCategory::Active => {
                active_count += 1;
                active_tokens += tokens;
            }
            RelevanceCategory::Dormant => {
                dormant_count += 1;
                dormant_tokens += tokens;
            }
            RelevanceCategory::Archivable => {
                archivable_count += 1;
                archivable_tokens += tokens;
            }
        }

        ranked_items.push((item.id.clone(), category, item.relevance_score));
    }

    // Sort by relevance descending (highest first).
    ranked_items.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

    ContextAuditReport {
        total_items: items.len(),
        total_tokens,
        critical_count,
        active_count,
        dormant_count,
        archivable_count,
        critical_tokens,
        active_tokens,
        dormant_tokens,
        archivable_tokens,
        ranked_items,
    }
}

/// Format a [`ContextAuditReport`] into a human-readable summary suitable for
/// injection into an LLM prompt.
pub fn format_audit_report(report: &ContextAuditReport) -> String {
    use std::fmt::Write;

    let mut out = String::with_capacity(512);

    writeln!(out, "Context Audit:").unwrap();
    writeln!(
        out,
        "- Total: {} items, {} tokens",
        report.total_items,
        format_number(report.total_tokens),
    )
    .unwrap();
    writeln!(
        out,
        "- Critical: {} items ({} tokens)",
        report.critical_count,
        format_number(report.critical_tokens),
    )
    .unwrap();
    writeln!(
        out,
        "- Active: {} items ({} tokens)",
        report.active_count,
        format_number(report.active_tokens),
    )
    .unwrap();
    writeln!(
        out,
        "- Dormant: {} items ({} tokens)",
        report.dormant_count,
        format_number(report.dormant_tokens),
    )
    .unwrap();
    writeln!(
        out,
        "- Archivable: {} items ({} tokens)",
        report.archivable_count,
        format_number(report.archivable_tokens),
    )
    .unwrap();

    // Top items — show up to 5.
    let top: Vec<String> = report
        .ranked_items
        .iter()
        .take(5)
        .map(|(id, _cat, score)| format!("{id} ({score:.2})"))
        .collect();

    if !top.is_empty() {
        write!(out, "Top items: [{}]", top.join(", ")).unwrap();
    }

    out
}

/// Format a `u32` with thousands separators (comma-delimited).
fn format_number(n: u32) -> String {
    let s = n.to_string();
    let mut result = String::with_capacity(s.len() + s.len() / 3);
    for (i, ch) in s.chars().rev().enumerate() {
        if i > 0 && i % 3 == 0 {
            result.push(',');
        }
        result.push(ch);
    }
    result.chars().rev().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const RECENT_THRESHOLD: u64 = 300_000; // 5 min
    const MAX_AGE: u64 = 1_800_000; // 30 min

    fn make_item(
        id: &str,
        item_type: ContextType,
        timestamp: u64,
        access_count: u32,
    ) -> ContextItem {
        let content = format!("Content for {id}");
        ContextItem {
            id: id.into(),
            item_type,
            estimated_tokens: ContextItem::estimate_tokens(&content),
            content,
            timestamp,
            relevance_score: 0.0,
            access_count,
            source: "test".into(),
        }
    }

    // -----------------------------------------------------------------------
    // 1. Empty items returns zeroes
    // -----------------------------------------------------------------------
    #[test]
    fn empty_items_returns_zeroes() {
        let mut items: Vec<ContextItem> = vec![];
        let report = context_audit(&mut items, 1_000_000, RECENT_THRESHOLD, MAX_AGE);

        assert_eq!(report.total_items, 0);
        assert_eq!(report.total_tokens, 0);
        assert_eq!(report.critical_count, 0);
        assert_eq!(report.active_count, 0);
        assert_eq!(report.dormant_count, 0);
        assert_eq!(report.archivable_count, 0);
        assert_eq!(report.critical_tokens, 0);
        assert_eq!(report.active_tokens, 0);
        assert_eq!(report.dormant_tokens, 0);
        assert_eq!(report.archivable_tokens, 0);
        assert!(report.ranked_items.is_empty());
    }

    // -----------------------------------------------------------------------
    // 2. All items recent -> all Critical
    // -----------------------------------------------------------------------
    #[test]
    fn all_recent_items_are_critical() {
        let now = 1_000_000;
        let mut items = vec![
            make_item("a", ContextType::Conversation, now - 1_000, 0),
            make_item("b", ContextType::ToolResult, now - 2_000, 0),
            make_item("c", ContextType::FileContent, now - 100, 0),
        ];
        let report = context_audit(&mut items, now, RECENT_THRESHOLD, MAX_AGE);

        assert_eq!(report.critical_count, 3);
        assert_eq!(report.active_count, 0);
        assert_eq!(report.dormant_count, 0);
        assert_eq!(report.archivable_count, 0);
    }

    // -----------------------------------------------------------------------
    // 3. Mix of categories counted correctly
    // -----------------------------------------------------------------------
    #[test]
    fn mix_of_categories_counted_correctly() {
        let now = 2_000_000;
        let mut items = vec![
            // Recent -> Critical
            make_item("recent", ContextType::Conversation, now - 1_000, 0),
            // SystemPrompt -> always Critical
            make_item("sys", ContextType::SystemPrompt, 0, 0),
            // Old, high access -> will get Active (relevance > 0.5 due to access)
            make_item("active", ContextType::Conversation, now - 500_000, 10),
            // Old, some access but low relevance -> Dormant
            make_item("dormant", ContextType::FileContent, 0, 1),
            // Old, zero access -> Archivable
            make_item("archive", ContextType::FileContent, 0, 0),
        ];
        let report = context_audit(&mut items, now, RECENT_THRESHOLD, MAX_AGE);

        assert_eq!(report.critical_count, 2); // recent + sys
        assert_eq!(report.active_count, 1); // active
        assert_eq!(report.dormant_count, 1); // dormant
        assert_eq!(report.archivable_count, 1); // archive
        assert_eq!(report.total_items, 5);
    }

    // -----------------------------------------------------------------------
    // 4. Tokens summed per category
    // -----------------------------------------------------------------------
    #[test]
    fn tokens_summed_per_category() {
        let now = 2_000_000;
        let mut items = vec![
            // Recent -> Critical
            make_item("crit1", ContextType::Conversation, now - 1_000, 0),
            make_item("crit2", ContextType::Conversation, now - 2_000, 0),
            // Old, zero access -> Archivable
            make_item("arch1", ContextType::FileContent, 0, 0),
        ];

        let expected_crit_tokens: u32 = items[0].estimated_tokens + items[1].estimated_tokens;
        let expected_arch_tokens: u32 = items[2].estimated_tokens;

        let report = context_audit(&mut items, now, RECENT_THRESHOLD, MAX_AGE);

        assert_eq!(report.critical_tokens, expected_crit_tokens);
        assert_eq!(report.archivable_tokens, expected_arch_tokens);
        assert_eq!(
            report.total_tokens,
            expected_crit_tokens + expected_arch_tokens
        );
    }

    // -----------------------------------------------------------------------
    // 5. Ranked items sorted by relevance descending
    // -----------------------------------------------------------------------
    #[test]
    fn ranked_items_sorted_descending() {
        let now = 2_000_000;
        let mut items = vec![
            // Old file -> low relevance
            make_item("low", ContextType::FileContent, 0, 0),
            // Recent conversation -> high relevance
            make_item("high", ContextType::Conversation, now - 1_000, 5),
            // Mid-age tool result with some access
            make_item("mid", ContextType::ToolResult, now - 600_000, 3),
        ];
        let report = context_audit(&mut items, now, RECENT_THRESHOLD, MAX_AGE);

        // Verify descending order.
        for window in report.ranked_items.windows(2) {
            assert!(
                window[0].2 >= window[1].2,
                "Expected {:?} >= {:?}",
                window[0],
                window[1]
            );
        }
        // Highest should be the recent conversation.
        assert_eq!(report.ranked_items[0].0, "high");
    }

    // -----------------------------------------------------------------------
    // 6. System prompts always Critical (even when very old)
    // -----------------------------------------------------------------------
    #[test]
    fn system_prompts_always_critical() {
        let now = 10_000_000;
        let mut items = vec![
            make_item("sys1", ContextType::SystemPrompt, 0, 0),
            make_item("sys2", ContextType::SystemPrompt, 100, 0),
        ];
        let report = context_audit(&mut items, now, RECENT_THRESHOLD, MAX_AGE);

        assert_eq!(report.critical_count, 2);
        for (_id, cat, _score) in &report.ranked_items {
            assert_eq!(*cat, RelevanceCategory::Critical);
        }
    }

    // -----------------------------------------------------------------------
    // 7. Old unaccessed items are Archivable
    // -----------------------------------------------------------------------
    #[test]
    fn old_unaccessed_items_are_archivable() {
        let now = 10_000_000;
        let mut items = vec![
            make_item("old1", ContextType::FileContent, 0, 0),
            make_item("old2", ContextType::ToolResult, 0, 0),
            make_item("old3", ContextType::Artifact, 0, 0),
        ];
        let report = context_audit(&mut items, now, RECENT_THRESHOLD, MAX_AGE);

        assert_eq!(report.archivable_count, 3);
        assert_eq!(report.critical_count, 0);
        assert_eq!(report.active_count, 0);
        assert_eq!(report.dormant_count, 0);
    }

    // -----------------------------------------------------------------------
    // 8. Format report includes key metrics
    // -----------------------------------------------------------------------
    #[test]
    fn format_report_includes_key_metrics() {
        let now = 2_000_000;
        let mut items = vec![
            make_item("r1", ContextType::Conversation, now - 1_000, 0),
            make_item("a1", ContextType::FileContent, 0, 0),
        ];
        let report = context_audit(&mut items, now, RECENT_THRESHOLD, MAX_AGE);
        let text = format_audit_report(&report);

        assert!(text.contains("Context Audit:"), "missing header");
        assert!(text.contains("Total: 2 items"), "missing total items");
        assert!(text.contains("Critical: 1 items"), "missing critical");
        assert!(text.contains("Archivable: 1 items"), "missing archivable");
        assert!(text.contains("Top items:"), "missing top items");
        // Verify the highest-ranked item's id appears first in the list.
        assert!(text.contains("r1"), "top item id missing from report");
    }

    // -----------------------------------------------------------------------
    // Bonus: format_number helper
    // -----------------------------------------------------------------------
    #[test]
    fn format_number_with_commas() {
        assert_eq!(format_number(0), "0");
        assert_eq!(format_number(999), "999");
        assert_eq!(format_number(1_000), "1,000");
        assert_eq!(format_number(12_340), "12,340");
        assert_eq!(format_number(1_000_000), "1,000,000");
    }
}
