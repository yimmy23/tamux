//! Context item model — typed, scored items that make up the agent's context window.

use serde::{Deserialize, Serialize};

use crate::agent::APPROX_CHARS_PER_TOKEN;

/// What kind of context this item represents.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContextType {
    /// A user or assistant conversational message.
    Conversation,
    /// Result from a tool execution.
    ToolResult,
    /// File content read into context.
    FileContent,
    /// Agent's internal reasoning or planning.
    AgentThought,
    /// Generated artifact (code, document, etc.).
    Artifact,
    /// System prompt or injected instructions.
    SystemPrompt,
}

/// Relevance category for context audit.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum RelevanceCategory {
    /// Must keep — recent user messages, active tool results.
    Critical,
    /// Recently accessed or high relevance score.
    Active,
    /// Old but has been accessed at some point.
    Dormant,
    /// Old, never re-accessed — safe to archive.
    Archivable,
}

/// A scored item in the agent's context window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextItem {
    /// Unique identifier.
    pub id: String,
    /// What kind of content this is.
    pub item_type: ContextType,
    /// The actual content text.
    pub content: String,
    /// When this item was created.
    pub timestamp: u64,
    /// Computed relevance score (0.0 = irrelevant, 1.0 = critical).
    pub relevance_score: f64,
    /// How many times this item was accessed/referenced.
    pub access_count: u32,
    /// Where this item came from (e.g., "user", "tool:bash_command", "system").
    pub source: String,
    /// Estimated token count for this item.
    pub estimated_tokens: u32,
}

impl ContextItem {
    /// Estimate the token count for this item's content.
    pub fn estimate_tokens(content: &str) -> u32 {
        (content.chars().count().div_ceil(APPROX_CHARS_PER_TOKEN) + 4) as u32
    }

    /// Compute a relevance score based on recency, access frequency, and type.
    ///
    /// - `now`: current timestamp in milliseconds
    /// - `max_age_ms`: age after which recency weight is 0 (default: 30 min)
    pub fn compute_relevance(&self, now: u64, max_age_ms: u64) -> f64 {
        let age_ms = now.saturating_sub(self.timestamp);
        let recency_weight = if max_age_ms == 0 {
            0.0
        } else {
            1.0 - (age_ms as f64 / max_age_ms as f64).min(1.0)
        };

        let access_weight = (self.access_count as f64 / 10.0).min(1.0);

        let type_weight = match self.item_type {
            ContextType::SystemPrompt => 1.0,
            ContextType::Conversation => 0.8,
            ContextType::AgentThought => 0.6,
            ContextType::ToolResult => 0.5,
            ContextType::Artifact => 0.4,
            ContextType::FileContent => 0.3,
        };

        // Weighted combination: recency 50%, access 20%, type 30%
        recency_weight * 0.5 + access_weight * 0.2 + type_weight * 0.3
    }

    /// Categorize this item based on its relevance score and age.
    pub fn categorize(&self, now: u64, recent_threshold_ms: u64) -> RelevanceCategory {
        let age_ms = now.saturating_sub(self.timestamp);
        let is_recent = age_ms < recent_threshold_ms;

        if is_recent || self.item_type == ContextType::SystemPrompt {
            RelevanceCategory::Critical
        } else if self.relevance_score > 0.5 {
            RelevanceCategory::Active
        } else if self.access_count > 0 {
            RelevanceCategory::Dormant
        } else {
            RelevanceCategory::Archivable
        }
    }
}

/// Report from a context audit.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextAuditReport {
    /// Total items audited.
    pub total_items: usize,
    /// Total estimated tokens.
    pub total_tokens: u32,
    /// Items by category.
    pub critical_count: usize,
    pub active_count: usize,
    pub dormant_count: usize,
    pub archivable_count: usize,
    /// Tokens by category.
    pub critical_tokens: u32,
    pub active_tokens: u32,
    pub dormant_tokens: u32,
    pub archivable_tokens: u32,
    /// Items sorted by relevance (highest first), with their categories.
    pub ranked_items: Vec<(String, RelevanceCategory, f64)>,
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn estimate_tokens_basic() {
        assert_eq!(ContextItem::estimate_tokens("hello world"), 7); // 11 chars / 4 + 4
    }

    #[test]
    fn estimate_tokens_empty() {
        assert_eq!(ContextItem::estimate_tokens(""), 4); // 0/4 + 4
    }

    #[test]
    fn relevance_recent_item_scores_high() {
        let mut item = make_item("a", ContextType::Conversation, 1000, 0);
        item.relevance_score = item.compute_relevance(1000, 60_000);
        assert!(item.relevance_score > 0.7);
    }

    #[test]
    fn relevance_old_item_scores_low() {
        let mut item = make_item("b", ContextType::FileContent, 0, 0);
        item.relevance_score = item.compute_relevance(120_000, 60_000);
        assert!(item.relevance_score < 0.3);
    }

    #[test]
    fn relevance_frequently_accessed_scores_higher() {
        let item_low = make_item("c1", ContextType::ToolResult, 0, 0);
        let item_high = make_item("c2", ContextType::ToolResult, 0, 10);
        let score_low = item_low.compute_relevance(120_000, 60_000);
        let score_high = item_high.compute_relevance(120_000, 60_000);
        assert!(score_high > score_low);
    }

    #[test]
    fn categorize_recent_as_critical() {
        let item = make_item("d", ContextType::Conversation, 950, 0);
        assert_eq!(item.categorize(1000, 60_000), RelevanceCategory::Critical);
    }

    #[test]
    fn categorize_system_prompt_always_critical() {
        let item = make_item("e", ContextType::SystemPrompt, 0, 0);
        assert_eq!(
            item.categorize(999_999, 60_000),
            RelevanceCategory::Critical
        );
    }

    #[test]
    fn categorize_old_accessed_as_dormant() {
        let mut item = make_item("f", ContextType::ToolResult, 0, 2);
        item.relevance_score = 0.2;
        assert_eq!(item.categorize(999_999, 60_000), RelevanceCategory::Dormant);
    }

    #[test]
    fn categorize_old_never_accessed_as_archivable() {
        let mut item = make_item("g", ContextType::FileContent, 0, 0);
        item.relevance_score = 0.1;
        assert_eq!(
            item.categorize(999_999, 60_000),
            RelevanceCategory::Archivable
        );
    }
}
