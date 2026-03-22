//! Termination condition DSL — parse and evaluate conditions for auto-stopping sub-agents.
//!
//! Supports a small expression language with atoms (`timeout(N)`,
//! `tool_success_count(N)`, `error_count(N)`, `tool_call_count(N)`) and
//! boolean combinators (`AND`, `OR`, `NOT`) with parenthetical grouping.
//!
//! # Grammar (informal)
//!
//! ```text
//! expr       = or_expr
//! or_expr    = and_expr ("OR" and_expr)*
//! and_expr   = not_expr ("AND" not_expr)*
//! not_expr   = "NOT" not_expr | atom
//! atom       = "(" expr ")" | condition
//! condition  = "timeout" "(" U64 ")"
//!            | "tool_success_count" "(" U32 ")"
//!            | "error_count" "(" U32 ")"
//!            | "tool_call_count" "(" U32 ")"
//! ```

use anyhow::{bail, Context, Result};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A parsed termination condition tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminationCondition {
    Timeout(u64),
    ToolSuccessCount(u32),
    ErrorCount(u32),
    ToolCallCount(u32),
    And(Box<TerminationCondition>, Box<TerminationCondition>),
    Or(Box<TerminationCondition>, Box<TerminationCondition>),
    Not(Box<TerminationCondition>),
}

/// Runtime metrics fed into the evaluator.
#[derive(Debug, Clone, Default)]
pub struct TerminationMetrics {
    pub elapsed_secs: u64,
    pub tool_calls_total: u32,
    pub tool_calls_succeeded: u32,
    pub consecutive_errors: u32,
    pub total_errors: u32,
}

/// Wraps a parsed [`TerminationCondition`] and evaluates it against live
/// metrics.
#[derive(Debug, Clone)]
pub struct TerminationEvaluator {
    condition: TerminationCondition,
}

impl TerminationEvaluator {
    /// Parse a DSL string into a `TerminationEvaluator`.
    ///
    /// Returns an error when the input is empty or syntactically invalid.
    pub fn parse(dsl: &str) -> Result<Self> {
        let tokens = tokenize(dsl)?;
        if tokens.is_empty() {
            bail!("empty termination DSL");
        }
        let mut pos = 0;
        let condition = parse_or_expr(&tokens, &mut pos)?;
        if pos < tokens.len() {
            bail!(
                "unexpected trailing token '{}' at position {pos}",
                tokens[pos]
            );
        }
        Ok(Self { condition })
    }

    /// Convenience constructor for a simple timeout condition.
    pub fn default_timeout(secs: u64) -> Self {
        Self {
            condition: TerminationCondition::Timeout(secs),
        }
    }

    /// Evaluate the condition against the given metrics.
    ///
    /// Returns `(should_stop, reason)` where `reason` is `Some` when the
    /// condition fires.
    pub fn should_terminate(&self, metrics: &TerminationMetrics) -> (bool, Option<String>) {
        evaluate(&self.condition, metrics)
    }

    /// Access the underlying parsed condition (useful for debugging /
    /// serialisation).
    pub fn condition(&self) -> &TerminationCondition {
        &self.condition
    }
}

// ---------------------------------------------------------------------------
// Evaluator
// ---------------------------------------------------------------------------

fn evaluate(cond: &TerminationCondition, m: &TerminationMetrics) -> (bool, Option<String>) {
    match cond {
        TerminationCondition::Timeout(secs) => {
            let fired = m.elapsed_secs >= *secs;
            if fired {
                (
                    true,
                    Some(format!("timeout: elapsed {}s >= {secs}s", m.elapsed_secs)),
                )
            } else {
                (false, None)
            }
        }
        TerminationCondition::ToolSuccessCount(n) => {
            let fired = m.tool_calls_succeeded >= *n;
            if fired {
                (
                    true,
                    Some(format!(
                        "tool_success_count: {} >= {n}",
                        m.tool_calls_succeeded
                    )),
                )
            } else {
                (false, None)
            }
        }
        TerminationCondition::ErrorCount(n) => {
            let fired = m.consecutive_errors >= *n;
            if fired {
                (
                    true,
                    Some(format!(
                        "error_count: {} consecutive errors >= {n}",
                        m.consecutive_errors
                    )),
                )
            } else {
                (false, None)
            }
        }
        TerminationCondition::ToolCallCount(n) => {
            let fired = m.tool_calls_total >= *n;
            if fired {
                (
                    true,
                    Some(format!("tool_call_count: {} >= {n}", m.tool_calls_total)),
                )
            } else {
                (false, None)
            }
        }
        TerminationCondition::And(lhs, rhs) => {
            let (l, lr) = evaluate(lhs, m);
            let (r, rr) = evaluate(rhs, m);
            if l && r {
                let reason = match (lr, rr) {
                    (Some(a), Some(b)) => Some(format!("({a}) AND ({b})")),
                    (Some(a), None) => Some(a),
                    (None, Some(b)) => Some(b),
                    (None, None) => None,
                };
                (true, reason)
            } else {
                (false, None)
            }
        }
        TerminationCondition::Or(lhs, rhs) => {
            let (l, lr) = evaluate(lhs, m);
            let (r, rr) = evaluate(rhs, m);
            if l || r {
                // Prefer the side(s) that actually fired.
                let reason = match (l, r) {
                    (true, true) => match (lr, rr) {
                        (Some(a), Some(b)) => Some(format!("({a}) OR ({b})")),
                        (Some(a), None) => Some(a),
                        (None, Some(b)) => Some(b),
                        (None, None) => None,
                    },
                    (true, false) => lr,
                    (false, true) => rr,
                    _ => None,
                };
                (true, reason)
            } else {
                (false, None)
            }
        }
        TerminationCondition::Not(inner) => {
            let (val, _reason) = evaluate(inner, m);
            if !val {
                (true, Some("NOT condition met".to_string()))
            } else {
                (false, None)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tokenizer
// ---------------------------------------------------------------------------

fn tokenize(input: &str) -> Result<Vec<String>> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();

    while let Some(&ch) = chars.peek() {
        if ch.is_whitespace() {
            chars.next();
            continue;
        }
        if ch == '(' || ch == ')' {
            tokens.push(ch.to_string());
            chars.next();
            continue;
        }
        // Accumulate an identifier or number.
        let mut buf = String::new();
        while let Some(&c) = chars.peek() {
            if c.is_whitespace() || c == '(' || c == ')' {
                break;
            }
            buf.push(c);
            chars.next();
        }
        if !buf.is_empty() {
            tokens.push(buf);
        }
    }

    Ok(tokens)
}

// ---------------------------------------------------------------------------
// Recursive-descent parser
// ---------------------------------------------------------------------------

/// or_expr = and_expr ("OR" and_expr)*
fn parse_or_expr(tokens: &[String], pos: &mut usize) -> Result<TerminationCondition> {
    let mut node = parse_and_expr(tokens, pos)?;
    while *pos < tokens.len() && tokens[*pos] == "OR" {
        *pos += 1; // consume OR
        let rhs = parse_and_expr(tokens, pos)?;
        node = TerminationCondition::Or(Box::new(node), Box::new(rhs));
    }
    Ok(node)
}

/// and_expr = not_expr ("AND" not_expr)*
fn parse_and_expr(tokens: &[String], pos: &mut usize) -> Result<TerminationCondition> {
    let mut node = parse_not_expr(tokens, pos)?;
    while *pos < tokens.len() && tokens[*pos] == "AND" {
        *pos += 1; // consume AND
        let rhs = parse_not_expr(tokens, pos)?;
        node = TerminationCondition::And(Box::new(node), Box::new(rhs));
    }
    Ok(node)
}

/// not_expr = "NOT" not_expr | atom
fn parse_not_expr(tokens: &[String], pos: &mut usize) -> Result<TerminationCondition> {
    if *pos < tokens.len() && tokens[*pos] == "NOT" {
        *pos += 1; // consume NOT
        let inner = parse_not_expr(tokens, pos)?;
        return Ok(TerminationCondition::Not(Box::new(inner)));
    }
    parse_atom(tokens, pos)
}

/// atom = "(" expr ")" | condition
fn parse_atom(tokens: &[String], pos: &mut usize) -> Result<TerminationCondition> {
    if *pos >= tokens.len() {
        bail!("unexpected end of input");
    }

    // Parenthesized sub-expression.
    if tokens[*pos] == "(" {
        *pos += 1; // consume (
        let node = parse_or_expr(tokens, pos)?;
        if *pos >= tokens.len() || tokens[*pos] != ")" {
            bail!(
                "expected ')' but found end of input or '{}'",
                tokens.get(*pos).unwrap_or(&String::new())
            );
        }
        *pos += 1; // consume )
        return Ok(node);
    }

    // Condition atoms: name ( number )
    let name = &tokens[*pos];
    *pos += 1;

    // Expect '('
    if *pos >= tokens.len() || tokens[*pos] != "(" {
        bail!(
            "expected '(' after condition name '{name}' but found '{}'",
            tokens.get(*pos).unwrap_or(&"end of input".to_string())
        );
    }
    *pos += 1; // consume (

    // Expect a numeric argument.
    if *pos >= tokens.len() {
        bail!("expected number after '(' in '{name}(...)'");
    }
    let num_str = &tokens[*pos];
    *pos += 1;

    // Expect ')'
    if *pos >= tokens.len() || tokens[*pos] != ")" {
        bail!(
            "expected ')' to close '{name}(...)' but found '{}'",
            tokens.get(*pos).unwrap_or(&"end of input".to_string())
        );
    }
    *pos += 1; // consume )

    match name.as_str() {
        "timeout" => {
            let n: u64 = num_str
                .parse()
                .with_context(|| format!("invalid number '{num_str}' in timeout()"))?;
            Ok(TerminationCondition::Timeout(n))
        }
        "tool_success_count" => {
            let n: u32 = num_str
                .parse()
                .with_context(|| format!("invalid number '{num_str}' in tool_success_count()"))?;
            Ok(TerminationCondition::ToolSuccessCount(n))
        }
        "error_count" => {
            let n: u32 = num_str
                .parse()
                .with_context(|| format!("invalid number '{num_str}' in error_count()"))?;
            Ok(TerminationCondition::ErrorCount(n))
        }
        "tool_call_count" => {
            let n: u32 = num_str
                .parse()
                .with_context(|| format!("invalid number '{num_str}' in tool_call_count()"))?;
            Ok(TerminationCondition::ToolCallCount(n))
        }
        other => {
            bail!("unknown condition '{other}'; expected one of: timeout, tool_success_count, error_count, tool_call_count");
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- helpers ---

    fn metrics(
        elapsed_secs: u64,
        total: u32,
        succeeded: u32,
        consecutive_err: u32,
        total_err: u32,
    ) -> TerminationMetrics {
        TerminationMetrics {
            elapsed_secs,
            tool_calls_total: total,
            tool_calls_succeeded: succeeded,
            consecutive_errors: consecutive_err,
            total_errors: total_err,
        }
    }

    // -----------------------------------------------------------------------
    // 1. Simple condition parsing
    // -----------------------------------------------------------------------

    #[test]
    fn parse_timeout() {
        let ev = TerminationEvaluator::parse("timeout(300)").unwrap();
        assert_eq!(ev.condition(), &TerminationCondition::Timeout(300));
    }

    #[test]
    fn parse_tool_success_count() {
        let ev = TerminationEvaluator::parse("tool_success_count(5)").unwrap();
        assert_eq!(ev.condition(), &TerminationCondition::ToolSuccessCount(5));
    }

    #[test]
    fn parse_error_count() {
        let ev = TerminationEvaluator::parse("error_count(3)").unwrap();
        assert_eq!(ev.condition(), &TerminationCondition::ErrorCount(3));
    }

    #[test]
    fn parse_tool_call_count() {
        let ev = TerminationEvaluator::parse("tool_call_count(50)").unwrap();
        assert_eq!(ev.condition(), &TerminationCondition::ToolCallCount(50));
    }

    // -----------------------------------------------------------------------
    // 2. Simple condition evaluation
    // -----------------------------------------------------------------------

    #[test]
    fn timeout_fires_when_elapsed() {
        let ev = TerminationEvaluator::default_timeout(60);
        let (stop, reason) = ev.should_terminate(&metrics(60, 0, 0, 0, 0));
        assert!(stop);
        assert!(reason.unwrap().contains("timeout"));
    }

    #[test]
    fn timeout_does_not_fire_before_elapsed() {
        let ev = TerminationEvaluator::default_timeout(60);
        let (stop, reason) = ev.should_terminate(&metrics(59, 0, 0, 0, 0));
        assert!(!stop);
        assert!(reason.is_none());
    }

    #[test]
    fn tool_success_count_fires() {
        let ev = TerminationEvaluator::parse("tool_success_count(3)").unwrap();
        let (stop, _) = ev.should_terminate(&metrics(0, 5, 3, 0, 0));
        assert!(stop);
    }

    #[test]
    fn tool_success_count_does_not_fire() {
        let ev = TerminationEvaluator::parse("tool_success_count(3)").unwrap();
        let (stop, _) = ev.should_terminate(&metrics(0, 5, 2, 0, 0));
        assert!(!stop);
    }

    #[test]
    fn error_count_fires() {
        let ev = TerminationEvaluator::parse("error_count(3)").unwrap();
        let (stop, reason) = ev.should_terminate(&metrics(0, 10, 7, 3, 5));
        assert!(stop);
        assert!(reason.unwrap().contains("consecutive errors"));
    }

    #[test]
    fn tool_call_count_fires() {
        let ev = TerminationEvaluator::parse("tool_call_count(10)").unwrap();
        let (stop, _) = ev.should_terminate(&metrics(0, 10, 8, 0, 2));
        assert!(stop);
    }

    // -----------------------------------------------------------------------
    // 3. Compound: OR
    // -----------------------------------------------------------------------

    #[test]
    fn or_fires_on_left() {
        let ev = TerminationEvaluator::parse("timeout(300) OR error_count(3)").unwrap();
        let (stop, reason) = ev.should_terminate(&metrics(300, 0, 0, 0, 0));
        assert!(stop);
        assert!(reason.unwrap().contains("timeout"));
    }

    #[test]
    fn or_fires_on_right() {
        let ev = TerminationEvaluator::parse("timeout(300) OR error_count(3)").unwrap();
        let (stop, reason) = ev.should_terminate(&metrics(10, 0, 0, 3, 3));
        assert!(stop);
        assert!(reason.unwrap().contains("error_count"));
    }

    #[test]
    fn or_does_not_fire_when_neither() {
        let ev = TerminationEvaluator::parse("timeout(300) OR error_count(3)").unwrap();
        let (stop, _) = ev.should_terminate(&metrics(100, 0, 0, 2, 2));
        assert!(!stop);
    }

    // -----------------------------------------------------------------------
    // 4. Compound: AND
    // -----------------------------------------------------------------------

    #[test]
    fn and_fires_when_both() {
        let ev = TerminationEvaluator::parse("timeout(60) AND tool_call_count(5)").unwrap();
        let (stop, _) = ev.should_terminate(&metrics(60, 5, 3, 0, 0));
        assert!(stop);
    }

    #[test]
    fn and_does_not_fire_when_only_one() {
        let ev = TerminationEvaluator::parse("timeout(60) AND tool_call_count(5)").unwrap();
        let (stop, _) = ev.should_terminate(&metrics(60, 4, 3, 0, 0));
        assert!(!stop);
    }

    // -----------------------------------------------------------------------
    // 5. NOT
    // -----------------------------------------------------------------------

    #[test]
    fn not_inverts_false_to_true() {
        let ev = TerminationEvaluator::parse("NOT timeout(300)").unwrap();
        let (stop, reason) = ev.should_terminate(&metrics(100, 0, 0, 0, 0));
        assert!(stop);
        assert!(reason.unwrap().contains("NOT"));
    }

    #[test]
    fn not_inverts_true_to_false() {
        let ev = TerminationEvaluator::parse("NOT timeout(300)").unwrap();
        let (stop, _) = ev.should_terminate(&metrics(300, 0, 0, 0, 0));
        assert!(!stop);
    }

    // -----------------------------------------------------------------------
    // 6. Nested / parenthesized
    // -----------------------------------------------------------------------

    #[test]
    fn nested_and_or() {
        // (timeout(300) AND tool_call_count(50)) OR error_count(3)
        let ev =
            TerminationEvaluator::parse("(timeout(300) AND tool_call_count(50)) OR error_count(3)")
                .unwrap();

        // error_count alone fires.
        let (stop, _) = ev.should_terminate(&metrics(10, 2, 2, 3, 3));
        assert!(stop);

        // AND branch fires.
        let (stop, _) = ev.should_terminate(&metrics(300, 50, 40, 0, 0));
        assert!(stop);

        // Neither fires.
        let (stop, _) = ev.should_terminate(&metrics(300, 49, 40, 2, 2));
        assert!(!stop);
    }

    #[test]
    fn double_not() {
        let ev = TerminationEvaluator::parse("NOT NOT timeout(60)").unwrap();
        // Double negation: fires when timeout fires.
        let (stop, _) = ev.should_terminate(&metrics(60, 0, 0, 0, 0));
        assert!(stop);
        let (stop, _) = ev.should_terminate(&metrics(59, 0, 0, 0, 0));
        assert!(!stop);
    }

    // -----------------------------------------------------------------------
    // 7. Parse errors
    // -----------------------------------------------------------------------

    #[test]
    fn error_empty_string() {
        let result = TerminationEvaluator::parse("");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("empty"));
    }

    #[test]
    fn error_whitespace_only() {
        let result = TerminationEvaluator::parse("   ");
        assert!(result.is_err());
    }

    #[test]
    fn error_unknown_condition() {
        let result = TerminationEvaluator::parse("foobar(42)");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("unknown condition"));
    }

    #[test]
    fn error_missing_paren() {
        let result = TerminationEvaluator::parse("timeout 300");
        assert!(result.is_err());
    }

    #[test]
    fn error_unclosed_paren() {
        let result = TerminationEvaluator::parse("(timeout(300)");
        assert!(result.is_err());
    }

    #[test]
    fn error_invalid_number() {
        let result = TerminationEvaluator::parse("timeout(abc)");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("invalid number"));
    }

    #[test]
    fn error_trailing_tokens() {
        let result = TerminationEvaluator::parse("timeout(60) timeout(120)");
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("trailing"));
    }

    // -----------------------------------------------------------------------
    // 8. Edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn zero_timeout_fires_immediately() {
        let ev = TerminationEvaluator::parse("timeout(0)").unwrap();
        let (stop, _) = ev.should_terminate(&metrics(0, 0, 0, 0, 0));
        assert!(stop);
    }

    #[test]
    fn zero_error_count_fires_immediately() {
        let ev = TerminationEvaluator::parse("error_count(0)").unwrap();
        let (stop, _) = ev.should_terminate(&metrics(0, 0, 0, 0, 0));
        assert!(stop);
    }

    #[test]
    fn default_timeout_constructor() {
        let ev = TerminationEvaluator::default_timeout(120);
        assert_eq!(ev.condition(), &TerminationCondition::Timeout(120));
        let (stop, _) = ev.should_terminate(&metrics(119, 0, 0, 0, 0));
        assert!(!stop);
        let (stop, _) = ev.should_terminate(&metrics(120, 0, 0, 0, 0));
        assert!(stop);
    }

    #[test]
    fn or_both_sides_fire_reports_both() {
        let ev = TerminationEvaluator::parse("timeout(60) OR error_count(3)").unwrap();
        let (stop, reason) = ev.should_terminate(&metrics(60, 0, 0, 3, 3));
        assert!(stop);
        let reason = reason.unwrap();
        assert!(reason.contains("timeout"));
        assert!(reason.contains("error_count"));
    }

    #[test]
    fn deeply_nested_parens() {
        let ev = TerminationEvaluator::parse(
            "((timeout(300) AND tool_call_count(50)) OR (error_count(3) AND tool_success_count(1)))",
        )
        .unwrap();
        // Second AND branch fires.
        let (stop, _) = ev.should_terminate(&metrics(10, 5, 1, 3, 3));
        assert!(stop);
        // Neither branch fires.
        let (stop, _) = ev.should_terminate(&metrics(10, 5, 0, 3, 3));
        assert!(!stop);
    }

    #[test]
    fn large_timeout_value() {
        let ev = TerminationEvaluator::parse("timeout(999999999)").unwrap();
        assert_eq!(ev.condition(), &TerminationCondition::Timeout(999_999_999));
    }

    #[test]
    fn chained_or_three_conditions() {
        let ev =
            TerminationEvaluator::parse("timeout(300) OR error_count(5) OR tool_call_count(100)")
                .unwrap();
        // Only third fires.
        let (stop, _) = ev.should_terminate(&metrics(10, 100, 80, 2, 4));
        assert!(stop);
    }

    #[test]
    fn chained_and_three_conditions() {
        let ev =
            TerminationEvaluator::parse("timeout(60) AND error_count(1) AND tool_call_count(10)")
                .unwrap();
        // All three met.
        let (stop, _) = ev.should_terminate(&metrics(60, 10, 5, 1, 1));
        assert!(stop);
        // One not met.
        let (stop, _) = ev.should_terminate(&metrics(60, 9, 5, 1, 1));
        assert!(!stop);
    }

    #[test]
    fn not_with_and() {
        // NOT (timeout(300) AND error_count(3))
        let ev = TerminationEvaluator::parse("NOT (timeout(300) AND error_count(3))").unwrap();
        // Inner is true → NOT yields false.
        let (stop, _) = ev.should_terminate(&metrics(300, 0, 0, 3, 3));
        assert!(!stop);
        // Inner is false → NOT yields true.
        let (stop, _) = ev.should_terminate(&metrics(100, 0, 0, 3, 3));
        assert!(stop);
    }
}
