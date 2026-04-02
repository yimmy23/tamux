//! Provider rate cards for token-to-USD cost estimation.
//!
//! Each `RateCard` stores the per-million-token price for input (prompt) and
//! output (completion) tokens. `default_rate_cards` returns a baseline table
//! covering the most popular models; operators can override via `CostConfig`.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Per-model pricing: input and output cost per 1 million tokens (USD).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateCard {
    pub input_per_million: f64,
    pub output_per_million: f64,
}

/// Returns default rate cards for popular models. Prices are per 1M tokens (USD).
pub fn default_rate_cards() -> HashMap<String, RateCard> {
    let mut cards = HashMap::new();
    cards.insert(
        "gpt-4o".to_string(),
        RateCard {
            input_per_million: 2.50,
            output_per_million: 10.00,
        },
    );
    cards.insert(
        "gpt-4o-mini".to_string(),
        RateCard {
            input_per_million: 0.15,
            output_per_million: 0.60,
        },
    );
    cards.insert(
        "claude-sonnet-4-20250514".to_string(),
        RateCard {
            input_per_million: 3.00,
            output_per_million: 15.00,
        },
    );
    cards.insert(
        "claude-3-5-sonnet-20241022".to_string(),
        RateCard {
            input_per_million: 3.00,
            output_per_million: 15.00,
        },
    );
    cards.insert(
        "claude-3-haiku-20240307".to_string(),
        RateCard {
            input_per_million: 0.25,
            output_per_million: 1.25,
        },
    );
    cards.insert(
        "claude-3-opus-20240229".to_string(),
        RateCard {
            input_per_million: 15.00,
            output_per_million: 75.00,
        },
    );
    cards.insert(
        "o1-mini".to_string(),
        RateCard {
            input_per_million: 3.00,
            output_per_million: 12.00,
        },
    );
    cards
}

/// Look up a rate card for the given provider/model combination.
///
/// Tries in order:
/// 1. Exact model match (e.g. `"claude-3-5-sonnet-20241022"`)
/// 2. Model with date suffix stripped (e.g. `"claude-3-5-sonnet"`)
/// 3. `"provider/model"` composite key
pub fn lookup_rate<'a>(
    rate_cards: &'a HashMap<String, RateCard>,
    provider: &str,
    model: &str,
) -> Option<&'a RateCard> {
    // 1. Exact match
    if let Some(card) = rate_cards.get(model) {
        return Some(card);
    }

    // 2. Strip trailing date suffix (8+ digit suffix after a hyphen)
    let stripped = strip_date_suffix(model);
    if stripped != model {
        if let Some(card) = rate_cards.get(stripped) {
            return Some(card);
        }
    }

    // 3. "provider/model" composite
    let composite = format!("{provider}/{model}");
    rate_cards.get(&composite)
}

/// Strips a trailing date suffix like `-20241022` from a model name.
/// Returns the original string if no date suffix is found.
fn strip_date_suffix(model: &str) -> &str {
    if let Some(pos) = model.rfind('-') {
        let suffix = &model[pos + 1..];
        // A date suffix is typically 8 digits (YYYYMMDD)
        if suffix.len() >= 8 && suffix.chars().all(|c| c.is_ascii_digit()) {
            return &model[..pos];
        }
    }
    model
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cost_default_rate_cards_includes_expected_models() {
        let cards = default_rate_cards();
        assert!(cards.contains_key("gpt-4o"), "missing gpt-4o");
        assert!(cards.contains_key("gpt-4o-mini"), "missing gpt-4o-mini");
        assert!(
            cards.contains_key("claude-sonnet-4-20250514"),
            "missing claude-sonnet-4-20250514"
        );
        assert!(
            cards.contains_key("claude-3-5-sonnet-20241022"),
            "missing claude-3-5-sonnet"
        );
        assert!(
            cards.contains_key("claude-3-haiku-20240307"),
            "missing claude-3-haiku"
        );
        assert!(
            cards.contains_key("claude-3-opus-20240229"),
            "missing claude-3-opus"
        );
        assert!(cards.contains_key("o1-mini"), "missing o1-mini");
    }

    #[test]
    fn cost_lookup_rate_exact_match() {
        let cards = default_rate_cards();
        let rate = lookup_rate(&cards, "openai", "gpt-4o");
        assert!(rate.is_some());
        let r = rate.unwrap();
        assert!((r.input_per_million - 2.50).abs() < f64::EPSILON);
        assert!((r.output_per_million - 10.00).abs() < f64::EPSILON);
    }

    #[test]
    fn cost_lookup_rate_strips_date_suffix() {
        let mut cards = HashMap::new();
        cards.insert(
            "claude-3-5-sonnet".to_string(),
            RateCard {
                input_per_million: 3.0,
                output_per_million: 15.0,
            },
        );
        let rate = lookup_rate(&cards, "anthropic", "claude-3-5-sonnet-20241022");
        assert!(rate.is_some(), "should match after stripping date suffix");
    }

    #[test]
    fn cost_lookup_rate_returns_none_for_unknown() {
        let cards = default_rate_cards();
        let rate = lookup_rate(&cards, "unknown", "totally-fake-model");
        assert!(rate.is_none());
    }

    #[test]
    fn cost_strip_date_suffix_works() {
        assert_eq!(
            strip_date_suffix("claude-3-5-sonnet-20241022"),
            "claude-3-5-sonnet"
        );
        assert_eq!(strip_date_suffix("gpt-4o"), "gpt-4o");
        assert_eq!(strip_date_suffix("o1-mini"), "o1-mini");
    }
}
