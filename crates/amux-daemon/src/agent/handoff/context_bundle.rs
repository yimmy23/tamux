//! Context bundle assembly with token ceiling enforcement.
//!
//! Assembles context from episodic refs, negative constraints, partial outputs,
//! and parent context into a bundle that respects a strict token ceiling.
//! Token estimation uses a chars/4 approximation consistent with the codebase.

use super::{ContextBundle, EpisodeRef, PartialOutput};

impl ContextBundle {
    /// Create a new context bundle for a handoff.
    pub fn new(task_spec: String, acceptance_criteria: String) -> Self {
        let mut bundle = Self {
            task_spec,
            acceptance_criteria,
            episodic_refs: Vec::new(),
            negative_constraints: Vec::new(),
            partial_outputs: Vec::new(),
            parent_context_summary: String::new(),
            handoff_depth: 0,
            estimated_tokens: 0,
        };
        bundle.recompute_estimated_tokens();
        bundle
    }

    /// Estimate tokens for a text string using chars/4 approximation.
    pub fn estimate_tokens(text: &str) -> u32 {
        (text.len() as u32) / 4
    }

    /// Recompute estimated_tokens by summing across all fields.
    pub fn recompute_estimated_tokens(&mut self) {
        let mut total = 0u32;
        total += Self::estimate_tokens(&self.task_spec);
        total += Self::estimate_tokens(&self.acceptance_criteria);
        total += Self::estimate_tokens(&self.parent_context_summary);
        for er in &self.episodic_refs {
            total += Self::estimate_tokens(&er.episode_id);
            total += Self::estimate_tokens(&er.summary);
            total += Self::estimate_tokens(&er.outcome);
        }
        for nc in &self.negative_constraints {
            total += Self::estimate_tokens(nc);
        }
        for po in &self.partial_outputs {
            total += Self::estimate_tokens(&po.content);
            total += Self::estimate_tokens(&po.status);
        }
        self.estimated_tokens = total;
    }

    /// Enforce a token ceiling by summarizing/trimming fields.
    ///
    /// Order: first summarize parent_context_summary, then trim partial_outputs
    /// (oldest first), then truncate negative_constraints.
    pub fn enforce_token_ceiling(&mut self, max_tokens: u32) {
        self.recompute_estimated_tokens();
        if self.estimated_tokens <= max_tokens {
            return;
        }

        // Step 1: Summarize parent_context_summary with progressively smaller limits
        if !self.parent_context_summary.is_empty() {
            let mut char_limit = self.parent_context_summary.len();
            while self.estimated_tokens > max_tokens && char_limit > 32 {
                char_limit /= 2;
                self.parent_context_summary = crate::agent::goal_parsing::summarize_text(
                    &self.parent_context_summary,
                    char_limit,
                );
                self.recompute_estimated_tokens();
            }
            if self.estimated_tokens > max_tokens && char_limit <= 32 {
                self.parent_context_summary.clear();
                self.recompute_estimated_tokens();
            }
        }

        // Step 2: Trim partial outputs (oldest first)
        while self.estimated_tokens > max_tokens && !self.partial_outputs.is_empty() {
            self.partial_outputs.remove(0);
            self.recompute_estimated_tokens();
        }

        // Step 3: Truncate negative constraints (oldest first)
        while self.estimated_tokens > max_tokens && !self.negative_constraints.is_empty() {
            self.negative_constraints.remove(0);
            self.recompute_estimated_tokens();
        }
    }

    /// Returns true when handoff depth has reached the limit (>= 3).
    pub fn is_at_depth_limit(&self) -> bool {
        self.handoff_depth >= 3
    }

    /// Increment the handoff depth by 1.
    pub fn increment_depth(&mut self) {
        self.handoff_depth += 1;
    }
}

impl Default for ContextBundle {
    fn default() -> Self {
        Self {
            task_spec: String::new(),
            acceptance_criteria: String::new(),
            episodic_refs: Vec::new(),
            negative_constraints: Vec::new(),
            partial_outputs: Vec::new(),
            parent_context_summary: String::new(),
            handoff_depth: 0,
            estimated_tokens: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_bundle_small() -> ContextBundle {
        ContextBundle::new("Write a function".to_string(), "Must compile".to_string())
    }

    #[allow(dead_code)]
    fn make_episode_ref(id: &str) -> EpisodeRef {
        EpisodeRef {
            episode_id: id.to_string(),
            summary: format!("Episode {id} summary"),
            outcome: "success".to_string(),
        }
    }

    fn make_partial_output(step: usize, content: &str) -> PartialOutput {
        PartialOutput {
            step_index: step,
            content: content.to_string(),
            status: "partial".to_string(),
        }
    }

    #[test]
    fn test_new_bundle_under_ceiling() {
        let bundle = make_bundle_small();
        assert!(bundle.estimated_tokens < 2000);
        assert_eq!(bundle.handoff_depth, 0);
    }

    #[test]
    fn test_estimate_tokens_chars_div_4() {
        // 100 chars -> 25 tokens
        let text = "a".repeat(100);
        assert_eq!(ContextBundle::estimate_tokens(&text), 25);
    }

    #[test]
    fn test_estimate_tokens_empty() {
        assert_eq!(ContextBundle::estimate_tokens(""), 0);
    }

    #[test]
    fn test_recompute_estimated_tokens() {
        let mut bundle = make_bundle_small();
        bundle.parent_context_summary = "x".repeat(400); // 100 tokens
        bundle.recompute_estimated_tokens();
        // task_spec + acceptance_criteria + parent_context_summary
        let expected = ContextBundle::estimate_tokens("Write a function")
            + ContextBundle::estimate_tokens("Must compile")
            + 100;
        assert_eq!(bundle.estimated_tokens, expected);
    }

    #[test]
    fn test_enforce_ceiling_summarizes_parent_context() {
        let mut bundle = make_bundle_small();
        // Make parent_context_summary very large (10000 chars = 2500 tokens)
        bundle.parent_context_summary = "word ".repeat(2000);
        bundle.recompute_estimated_tokens();
        assert!(bundle.estimated_tokens > 2000);

        bundle.enforce_token_ceiling(2000);
        assert!(
            bundle.estimated_tokens <= 2000,
            "estimated_tokens={} should be <= 2000",
            bundle.estimated_tokens
        );
    }

    #[test]
    fn test_enforce_ceiling_trims_partial_outputs() {
        let mut bundle = make_bundle_small();
        // Add large partial outputs
        for i in 0..10 {
            bundle
                .partial_outputs
                .push(make_partial_output(i, &"x".repeat(1000)));
        }
        bundle.recompute_estimated_tokens();
        assert!(bundle.estimated_tokens > 2000);

        bundle.enforce_token_ceiling(2000);
        assert!(
            bundle.estimated_tokens <= 2000,
            "estimated_tokens={} should be <= 2000",
            bundle.estimated_tokens
        );
        // Some partial outputs should have been removed
        assert!(bundle.partial_outputs.len() < 10);
    }

    #[test]
    fn test_enforce_ceiling_already_under() {
        let mut bundle = make_bundle_small();
        let tokens_before = bundle.estimated_tokens;
        bundle.enforce_token_ceiling(2000);
        assert_eq!(bundle.estimated_tokens, tokens_before);
    }

    #[test]
    fn test_handoff_depth_starts_at_zero() {
        let bundle = ContextBundle::new("spec".to_string(), "criteria".to_string());
        assert_eq!(bundle.handoff_depth, 0);
    }

    #[test]
    fn test_is_at_depth_limit_false_at_zero() {
        let bundle = ContextBundle::default();
        assert!(!bundle.is_at_depth_limit());
    }

    #[test]
    fn test_is_at_depth_limit_true_at_three() {
        let mut bundle = ContextBundle::default();
        bundle.handoff_depth = 3;
        assert!(bundle.is_at_depth_limit());
    }

    #[test]
    fn test_is_at_depth_limit_true_above_three() {
        let mut bundle = ContextBundle::default();
        bundle.handoff_depth = 5;
        assert!(bundle.is_at_depth_limit());
    }

    #[test]
    fn test_increment_depth() {
        let mut bundle = ContextBundle::default();
        assert_eq!(bundle.handoff_depth, 0);
        bundle.increment_depth();
        assert_eq!(bundle.handoff_depth, 1);
        bundle.increment_depth();
        assert_eq!(bundle.handoff_depth, 2);
    }

    #[test]
    fn test_default_bundle() {
        let bundle = ContextBundle::default();
        assert!(bundle.task_spec.is_empty());
        assert!(bundle.acceptance_criteria.is_empty());
        assert!(bundle.episodic_refs.is_empty());
        assert!(bundle.negative_constraints.is_empty());
        assert!(bundle.partial_outputs.is_empty());
        assert!(bundle.parent_context_summary.is_empty());
        assert_eq!(bundle.handoff_depth, 0);
        assert_eq!(bundle.estimated_tokens, 0);
    }
}
