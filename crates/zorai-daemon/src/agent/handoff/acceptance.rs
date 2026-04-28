//! Acceptance criteria validation with structural checks.
//!
//! Validates specialist output against acceptance criteria before accepting
//! handoff results. Structural checks are evaluated first; LLM validation
//! is deferred to the broker if structural checks pass and it is requested.

use super::AcceptanceCriteria;

/// Result of validating output against acceptance criteria.
#[derive(Debug, Clone)]
pub struct ValidationResult {
    /// Whether all checks passed.
    pub passed: bool,
    /// Descriptions of any failed checks.
    pub failures: Vec<String>,
    /// Whether LLM validation is still needed (only set when structural checks pass).
    pub needs_llm_validation: bool,
}

impl AcceptanceCriteria {
    /// Validate output against structural checks.
    ///
    /// Checks are evaluated in order: non_empty, min_length:N, contains:TEXT.
    /// All failures are collected (not short-circuited).
    /// `needs_llm_validation` is only set when structural checks pass and
    /// `require_llm_validation` is true.
    pub fn validate_structural(&self, output: &str) -> ValidationResult {
        let mut failures = Vec::new();

        // Always check that output is non-empty if there are no explicit checks
        // but only add failure if output is actually empty
        let is_empty = output.trim().is_empty();

        for check in &self.structural_checks {
            if check == "non_empty" {
                if is_empty {
                    failures.push("output is empty".to_string());
                }
            } else if let Some(rest) = check.strip_prefix("min_length:") {
                if let Ok(min) = rest.parse::<usize>() {
                    if output.len() < min {
                        failures.push(format!(
                            "output length {} is below minimum {}",
                            output.len(),
                            min
                        ));
                    }
                }
            } else if let Some(needle) = check.strip_prefix("contains:") {
                if !output.contains(needle) {
                    failures.push(format!("output does not contain '{}'", needle));
                }
            }
        }

        let passed = failures.is_empty();
        let needs_llm_validation = passed && self.require_llm_validation;

        ValidationResult {
            passed,
            failures,
            needs_llm_validation,
        }
    }

    /// Default acceptance criteria for code output.
    pub fn default_for_code() -> Self {
        Self {
            description: "Code output validation".to_string(),
            structural_checks: vec!["non_empty".to_string(), "min_length:50".to_string()],
            require_llm_validation: true,
        }
    }

    /// Default acceptance criteria for research output.
    pub fn default_for_research() -> Self {
        Self {
            description: "Research output validation".to_string(),
            structural_checks: vec!["non_empty".to_string(), "min_length:100".to_string()],
            require_llm_validation: true,
        }
    }

    /// Default acceptance criteria for review output.
    pub fn default_for_review() -> Self {
        Self {
            description: "Review output validation".to_string(),
            structural_checks: vec!["non_empty".to_string()],
            require_llm_validation: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_criteria(checks: Vec<&str>, require_llm: bool) -> AcceptanceCriteria {
        AcceptanceCriteria {
            description: "test criteria".to_string(),
            structural_checks: checks.into_iter().map(|s| s.to_string()).collect(),
            require_llm_validation: require_llm,
        }
    }

    #[test]
    fn test_empty_output_fails() {
        let criteria = make_criteria(vec!["non_empty"], false);
        let result = criteria.validate_structural("");
        assert!(!result.passed);
        assert!(!result.failures.is_empty());
        assert!(result.failures[0].contains("empty"));
    }

    #[test]
    fn test_non_empty_output_no_checks_passes() {
        let criteria = make_criteria(vec![], false);
        let result = criteria.validate_structural("some output");
        assert!(result.passed);
        assert!(result.failures.is_empty());
    }

    #[test]
    fn test_non_empty_check_passes() {
        let criteria = make_criteria(vec!["non_empty"], false);
        let result = criteria.validate_structural("some output");
        assert!(result.passed);
    }

    #[test]
    fn test_min_length_fails_when_short() {
        let criteria = make_criteria(vec!["min_length:100"], false);
        let result = criteria.validate_structural(&"x".repeat(50));
        assert!(!result.passed);
        assert!(result.failures[0].contains("100"));
    }

    #[test]
    fn test_min_length_passes_when_long_enough() {
        let criteria = make_criteria(vec!["min_length:100"], false);
        let result = criteria.validate_structural(&"x".repeat(150));
        assert!(result.passed);
    }

    #[test]
    fn test_contains_fails_when_missing() {
        let criteria = make_criteria(vec!["contains:error"], false);
        let result = criteria.validate_structural("everything is fine");
        assert!(!result.passed);
        assert!(result.failures[0].contains("error"));
    }

    #[test]
    fn test_contains_passes_when_present() {
        let criteria = make_criteria(vec!["contains:success"], false);
        let result = criteria.validate_structural("the operation was a success");
        assert!(result.passed);
    }

    #[test]
    fn test_multiple_checks_all_must_pass() {
        let criteria = make_criteria(vec!["non_empty", "min_length:10", "contains:ok"], false);
        // Short and missing "ok"
        let result = criteria.validate_structural("hi");
        assert!(!result.passed);
        // Should have 2 failures: min_length and contains
        assert_eq!(result.failures.len(), 2);
    }

    #[test]
    fn test_llm_validation_flag_when_structural_pass() {
        let criteria = make_criteria(vec!["non_empty"], true);
        let result = criteria.validate_structural("some output");
        assert!(result.passed);
        assert!(result.needs_llm_validation);
    }

    #[test]
    fn test_llm_validation_flag_not_set_when_structural_fail() {
        let criteria = make_criteria(vec!["non_empty"], true);
        let result = criteria.validate_structural("");
        assert!(!result.passed);
        assert!(!result.needs_llm_validation);
    }

    #[test]
    fn test_default_for_code() {
        let criteria = AcceptanceCriteria::default_for_code();
        assert!(criteria
            .structural_checks
            .contains(&"non_empty".to_string()));
        assert!(criteria
            .structural_checks
            .contains(&"min_length:50".to_string()));
        assert!(criteria.require_llm_validation);
    }

    #[test]
    fn test_default_for_research() {
        let criteria = AcceptanceCriteria::default_for_research();
        assert!(criteria
            .structural_checks
            .contains(&"non_empty".to_string()));
        assert!(criteria
            .structural_checks
            .contains(&"min_length:100".to_string()));
        assert!(criteria.require_llm_validation);
    }

    #[test]
    fn test_default_for_review() {
        let criteria = AcceptanceCriteria::default_for_review();
        assert!(criteria
            .structural_checks
            .contains(&"non_empty".to_string()));
        assert!(!criteria.require_llm_validation);
    }
}
