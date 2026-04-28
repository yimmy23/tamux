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
            bail!(
                "unknown condition '{other}'; expected one of: timeout, tool_success_count, error_count, tool_call_count"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "termination/tests.rs"]
mod tests;
