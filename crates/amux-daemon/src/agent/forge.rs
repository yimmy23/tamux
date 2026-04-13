#![allow(dead_code)]

//! Trajectory-Informed Self-Reflection Loop (Spec 02) — Svarog "Forge"
//!
//! Analyzes execution traces to detect recurring patterns (fallback loops,
//! revision triggers, timeout patterns, approval friction) and generates
//! strategy hints. High-priority hints are appended to MEMORY.md with a
//! timestamped `[forge]` provenance prefix.

use super::*;
use chrono::{SecondsFormat, Utc};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

const TRACE_SCAN_LIMIT: usize = 200;

/// An execution pattern detected by the forge pass.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionPattern {
    pub pattern_type: PatternType,
    pub frequency: usize,
    pub affected_tools: Vec<String>,
    pub operator_impact: String, // e.g., "caused 3 revisions"
    pub confidence: f64,
}

/// Types of execution patterns the forge detects.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PatternType {
    /// bash_command → read_file happens repeatedly
    ToolFallbackLoop,
    /// Agent output consistently revised by operator
    RevisionTrigger,
    /// Certain commands consistently time out
    TimeoutProne,
    /// Tools that trigger approval but are always denied
    ApprovalFriction,
    /// Tasks spawned but never completed
    StaleTaskAccumulation,
}

impl std::fmt::Display for PatternType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ToolFallbackLoop => write!(f, "tool_fallback_loop"),
            Self::RevisionTrigger => write!(f, "revision_trigger"),
            Self::TimeoutProne => write!(f, "timeout_prone"),
            Self::ApprovalFriction => write!(f, "approval_friction"),
            Self::StaleTaskAccumulation => write!(f, "stale_task_accumulation"),
        }
    }
}

/// A strategy hint generated from detected patterns.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyHint {
    pub for_agent: String,
    pub target: String,         // tool name, behavior, or workflow
    pub hint: String,           // actionable suggestion
    pub priority: u8,           // 1-5, 5 being highest
    pub source_pattern: String, // pattern_type that generated this
}

/// Configuration for the forge pass.
#[derive(Debug, Clone)]
pub struct ForgeConfig {
    pub enabled: bool,
    pub interval_hours: u64,
    pub lookback_hours: u64,
    pub min_pattern_frequency: usize, // patterns must occur ≥N times
    pub min_auto_apply_priority: u8,  // hints with priority ≥ N auto-applied
    pub agent_id: String,
    pub max_forge_entries_per_pass: usize,
}

impl Default for ForgeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_hours: 6,
            lookback_hours: 48,
            min_pattern_frequency: 3,
            min_auto_apply_priority: 3,
            agent_id: "svarog".into(),
            max_forge_entries_per_pass: 10,
        }
    }
}

/// Result of a forge analysis pass.
#[derive(Debug)]
pub struct ForgeAnalysis {
    pub agent_id: String,
    pub period_start_ms: u64,
    pub period_end_ms: u64,
    pub traces_analyzed: usize,
    pub patterns: Vec<ExecutionPattern>,
    pub strategy_hints: Vec<StrategyHint>,
}

/// Result summary returned after applying forge hints.
#[derive(Debug, Default)]
pub struct ForgeResult {
    pub traces_analyzed: usize,
    pub patterns_detected: usize,
    pub hints_generated: usize,
    pub hints_auto_applied: usize,
    pub hints_logged_only: usize,
}

#[derive(Debug, Clone)]
struct ForgeTraceRow {
    tool_sequence_json: Option<String>,
    metrics_json: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct TraceMetrics {
    #[serde(default)]
    total_duration_ms: u64,
    #[serde(default)]
    total_tokens_used: u64,
    #[serde(default)]
    step_count: usize,
    #[serde(default)]
    success_rate: f64,
    #[serde(default)]
    tool_fallbacks: u64,
    #[serde(default)]
    operator_revisions: u64,
    #[serde(default)]
    fast_denials: u64,
    #[serde(default)]
    exit_code: Option<i64>,
}

/// Run a forge pass over execution traces for the given agent.
pub async fn run_forge_pass(
    db: &crate::history::HistoryStore,
    config: &ForgeConfig,
    agent_data_dir: &std::path::Path,
) -> anyhow::Result<ForgeResult> {
    if !config.enabled {
        return Ok(ForgeResult::default());
    }

    let period_end_ms = now_millis();
    let period_start_ms = period_end_ms.saturating_sub(config.lookback_hours * 60 * 60 * 1000);
    let traces =
        list_recent_trace_rows(db, &config.agent_id, period_start_ms, TRACE_SCAN_LIMIT).await?;
    let traces_analyzed = traces.len();
    let patterns = detect_patterns(&traces, config.min_pattern_frequency);
    let mut hints = generate_hints(&patterns, &config.agent_id);
    hints.truncate(config.max_forge_entries_per_pass);
    let applied = apply_forge_hints(
        &hints,
        config.min_auto_apply_priority,
        agent_data_dir,
        config.max_forge_entries_per_pass,
    )
    .await?;

    let result = ForgeResult {
        traces_analyzed,
        patterns_detected: patterns.len(),
        hints_generated: hints.len(),
        hints_auto_applied: applied.len(),
        hints_logged_only: hints.len().saturating_sub(applied.len()),
    };

    log_forge_pass(
        db,
        &config.agent_id,
        period_start_ms,
        period_end_ms,
        &result,
    )
    .await?;

    Ok(result)
}

/// Apply forge hints to MEMORY.md (append with timestamped [forge] prefix).
pub async fn apply_forge_hints(
    hints: &[StrategyHint],
    min_priority: u8,
    agent_data_dir: &std::path::Path,
    max_entries: usize,
) -> anyhow::Result<Vec<String>> {
    let scope_id = current_agent_scope_id();
    ensure_memory_files_for_scope(agent_data_dir, &scope_id).await?;
    let memory_path = memory_paths_for_scope(agent_data_dir, &scope_id).memory_path;
    let mut existing = tokio::fs::read_to_string(&memory_path)
        .await
        .unwrap_or_default();
    let mut applied = Vec::new();

    for hint in hints
        .iter()
        .filter(|hint| hint.priority >= min_priority)
        .take(max_entries)
    {
        if content_contains_equivalent_hint(&existing, &hint.hint) {
            continue;
        }

        let line = format_forge_note(&hint.hint, now_millis());
        let next = if existing.trim().is_empty() {
            line.clone()
        } else {
            format!("{}\n\n{}", existing.trim_end(), line)
        };

        if next.chars().count() > 2_200 {
            break;
        }

        existing = next;
        applied.push(line);
    }

    if !applied.is_empty() {
        tokio::fs::write(&memory_path, existing).await?;
    }

    Ok(applied)
}

fn format_forge_note(hint: &str, timestamp_ms: u64) -> String {
    let timestamp = chrono::DateTime::<Utc>::from_timestamp_millis(timestamp_ms as i64)
        .map(|dt| dt.to_rfc3339_opts(SecondsFormat::Secs, true))
        .unwrap_or_else(|| timestamp_ms.to_string());
    format!("- [forge][{timestamp}] {}", hint.trim())
}

fn content_contains_equivalent_hint(content: &str, hint: &str) -> bool {
    let normalized_hint = normalize_forge_hint(hint);
    if normalized_hint.is_empty() {
        return false;
    }

    content.lines().any(|line| {
        normalized_line_hint(line)
            .as_ref()
            .is_some_and(|existing| existing == &normalized_hint)
    })
}

fn normalized_line_hint(line: &str) -> Option<String> {
    let cleaned = strip_forge_markup(line);
    if cleaned.is_empty() || cleaned.starts_with('#') {
        return None;
    }

    let normalized = normalize_forge_hint(&cleaned);
    (!normalized.is_empty()).then_some(normalized)
}

fn strip_forge_markup(line: &str) -> String {
    let mut cleaned = line.trim();
    while let Some(rest) = cleaned.strip_prefix(['-', '*', '>', ' ']) {
        cleaned = rest.trim_start();
    }

    while let Some(rest) = cleaned.strip_prefix('[') {
        if let Some(end_idx) = rest.find(']') {
            cleaned = rest[end_idx + 1..].trim_start();
        } else {
            break;
        }
    }

    cleaned
        .trim_matches('`')
        .trim_matches('*')
        .trim_matches('_')
        .trim()
        .to_string()
}

fn normalize_forge_hint(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '/' || ch == '.' || ch == '-' {
                ch.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

async fn list_recent_trace_rows(
    db: &crate::history::HistoryStore,
    agent_id: &str,
    period_start_ms: u64,
    limit: usize,
) -> anyhow::Result<Vec<ForgeTraceRow>> {
    let agent_id = agent_id.to_string();
    db.conn
        .call(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT tool_sequence_json, metrics_json
                 FROM execution_traces
                 WHERE agent_id = ?1 AND started_at_ms >= ?2
                 ORDER BY started_at_ms DESC
                 LIMIT ?3",
            )?;
            let rows = stmt.query_map(
                params![agent_id, period_start_ms as i64, limit as i64],
                |row| {
                    Ok(ForgeTraceRow {
                        tool_sequence_json: row.get(0)?,
                        metrics_json: row.get(1)?,
                    })
                },
            )?;
            Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))
}

fn detect_patterns(traces: &[ForgeTraceRow], min_frequency: usize) -> Vec<ExecutionPattern> {
    let mut fallback_pairs: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut timeout_by_tool: BTreeMap<String, usize> = BTreeMap::new();
    let mut revision_count = 0usize;
    let mut approval_friction_count = 0usize;
    let mut stale_task_count = 0usize;

    for trace in traces {
        let tool_sequence: Vec<String> = trace
            .tool_sequence_json
            .as_deref()
            .and_then(|json| serde_json::from_str(json).ok())
            .unwrap_or_default();
        let metrics: TraceMetrics = trace
            .metrics_json
            .as_deref()
            .and_then(|json| serde_json::from_str(json).ok())
            .unwrap_or_default();

        for pair in tool_sequence.windows(2) {
            if let [from, to] = pair {
                *fallback_pairs
                    .entry((from.clone(), to.clone()))
                    .or_insert(0) += 1;
            }
        }

        if metrics.operator_revisions > 0 {
            revision_count += 1;
        }
        if metrics.fast_denials > 0 {
            approval_friction_count += 1;
        }
        if metrics.exit_code.unwrap_or(0) != 0 || metrics.total_duration_ms >= 30_000 {
            for tool in &tool_sequence {
                *timeout_by_tool.entry(tool.clone()).or_insert(0) += 1;
            }
        }
        if metrics.success_rate == 0.0
            && metrics.step_count > 0
            && metrics.total_duration_ms >= 60_000
        {
            stale_task_count += 1;
        }
    }

    let mut patterns = Vec::new();

    for ((from, to), frequency) in fallback_pairs {
        if frequency >= min_frequency {
            patterns.push(ExecutionPattern {
                pattern_type: PatternType::ToolFallbackLoop,
                frequency,
                affected_tools: vec![from, to],
                operator_impact: format!("observed {frequency} consecutive fallback transitions"),
                confidence: 0.72,
            });
        }
    }

    for (tool, frequency) in timeout_by_tool {
        if frequency >= min_frequency {
            patterns.push(ExecutionPattern {
                pattern_type: PatternType::TimeoutProne,
                frequency,
                affected_tools: vec![tool],
                operator_impact: format!("involved in {frequency} slow or non-zero-exit traces"),
                confidence: 0.67,
            });
        }
    }

    if revision_count >= min_frequency {
        patterns.push(ExecutionPattern {
            pattern_type: PatternType::RevisionTrigger,
            frequency: revision_count,
            affected_tools: Vec::new(),
            operator_impact: format!("{revision_count} traces required operator revision"),
            confidence: 0.75,
        });
    }

    if approval_friction_count >= min_frequency {
        patterns.push(ExecutionPattern {
            pattern_type: PatternType::ApprovalFriction,
            frequency: approval_friction_count,
            affected_tools: Vec::new(),
            operator_impact: format!("{approval_friction_count} traces hit fast denials"),
            confidence: 0.7,
        });
    }

    if stale_task_count >= min_frequency {
        patterns.push(ExecutionPattern {
            pattern_type: PatternType::StaleTaskAccumulation,
            frequency: stale_task_count,
            affected_tools: Vec::new(),
            operator_impact: format!("{stale_task_count} traces look stalled or abandoned"),
            confidence: 0.64,
        });
    }

    patterns
}

fn generate_hints(patterns: &[ExecutionPattern], agent_id: &str) -> Vec<StrategyHint> {
    let mut hints = Vec::new();
    let mut seen = BTreeSet::new();

    for pattern in patterns {
        let hint = match pattern.pattern_type {
            PatternType::ToolFallbackLoop => {
                let target = if pattern.affected_tools.is_empty() {
                    "workflow".to_string()
                } else {
                    pattern.affected_tools.join(" -> ")
                };
                StrategyHint {
                    for_agent: agent_id.to_string(),
                    target,
                    hint: format!(
                        "When the same fallback chain appears repeatedly, prefer the later successful tool earlier in the plan and justify the switch explicitly."
                    ),
                    priority: 4,
                    source_pattern: pattern.pattern_type.to_string(),
                }
            }
            PatternType::RevisionTrigger => StrategyHint {
                for_agent: agent_id.to_string(),
                target: "response_style".to_string(),
                hint: "Operator revisions are recurring; lead with a compact answer, then the hard details, and avoid vague framing.".to_string(),
                priority: 4,
                source_pattern: pattern.pattern_type.to_string(),
            },
            PatternType::TimeoutProne => StrategyHint {
                for_agent: agent_id.to_string(),
                target: pattern
                    .affected_tools
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "tooling".to_string()),
                hint: "Timeout-prone traces are recurring; prefer bounded reads, shorter commands, and non-blocking execution for long-running work.".to_string(),
                priority: 5,
                source_pattern: pattern.pattern_type.to_string(),
            },
            PatternType::ApprovalFriction => StrategyHint {
                for_agent: agent_id.to_string(),
                target: "approval_risk".to_string(),
                hint: "Approval friction is recurring; choose lower-blast-radius tools first and avoid unnecessary approval-triggering actions.".to_string(),
                priority: 3,
                source_pattern: pattern.pattern_type.to_string(),
            },
            PatternType::StaleTaskAccumulation => StrategyHint {
                for_agent: agent_id.to_string(),
                target: "task_hygiene".to_string(),
                hint: "Stale task accumulation is recurring; keep todo/task state current and prefer small, finishable task slices over lingering background work.".to_string(),
                priority: 3,
                source_pattern: pattern.pattern_type.to_string(),
            },
        };

        let key = format!("{}::{}", hint.target, hint.hint);
        if seen.insert(key) {
            hints.push(hint);
        }
    }

    hints
}

async fn log_forge_pass(
    db: &crate::history::HistoryStore,
    agent_id: &str,
    period_start_ms: u64,
    period_end_ms: u64,
    result: &ForgeResult,
) -> anyhow::Result<()> {
    let agent_id = agent_id.to_string();
    let traces_analyzed = result.traces_analyzed as i64;
    let patterns_found = result.patterns_detected as i64;
    let hints_applied = result.hints_auto_applied as i64;
    let hints_logged = result.hints_logged_only as i64;
    let completed_at_ms = now_millis() as i64;

    db.conn
        .call(move |conn| {
            conn.execute(
                "INSERT INTO forge_pass_log \
                 (agent_id, period_start_ms, period_end_ms, traces_analyzed, patterns_found, hints_applied, hints_logged, completed_at_ms) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    agent_id,
                    period_start_ms as i64,
                    period_end_ms as i64,
                    traces_analyzed,
                    patterns_found,
                    hints_applied,
                    hints_logged,
                    completed_at_ms,
                ],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use uuid::Uuid;

    #[test]
    fn pattern_type_display() {
        assert_eq!(
            PatternType::ToolFallbackLoop.to_string(),
            "tool_fallback_loop"
        );
        assert_eq!(PatternType::TimeoutProne.to_string(), "timeout_prone");
    }

    #[test]
    fn default_config_sane() {
        let cfg = ForgeConfig::default();
        assert!(cfg.enabled);
        assert_eq!(cfg.interval_hours, 6);
        assert_eq!(cfg.min_pattern_frequency, 3);
        assert_eq!(cfg.agent_id, "svarog");
    }

    #[test]
    fn strategy_hint_serialization() {
        let hint = StrategyHint {
            for_agent: "svarog".into(),
            target: "bash_command".into(),
            hint: "prefer read_file over bash_command for file reads".into(),
            priority: 4,
            source_pattern: "tool_fallback_loop".into(),
        };
        let json = serde_json::to_string(&hint).unwrap();
        assert!(json.contains("tool_fallback_loop"));
        assert!(json.contains("prefer read_file"));
    }

    #[test]
    fn detects_fallback_pattern() {
        let traces = vec![
            ForgeTraceRow {
                tool_sequence_json: Some("[\"bash_command\",\"read_file\"]".into()),
                metrics_json: Some("{}".into()),
            },
            ForgeTraceRow {
                tool_sequence_json: Some("[\"bash_command\",\"read_file\"]".into()),
                metrics_json: Some("{}".into()),
            },
            ForgeTraceRow {
                tool_sequence_json: Some("[\"bash_command\",\"read_file\"]".into()),
                metrics_json: Some("{}".into()),
            },
        ];
        let patterns = detect_patterns(&traces, 3);
        assert!(patterns
            .iter()
            .any(|pattern| pattern.pattern_type == PatternType::ToolFallbackLoop));
    }

    #[test]
    fn formatted_forge_note_includes_rfc3339_timestamp() {
        let note = format_forge_note(
            "Prefer bounded reads before shelling out for large files.",
            0,
        );
        assert_eq!(
            note,
            "- [forge][1970-01-01T00:00:00Z] Prefer bounded reads before shelling out for large files."
        );
    }

    #[test]
    fn equivalent_hint_detection_ignores_forge_provenance_tags() {
        let content = concat!(
            "# Memory\n",
            "- [forge] Timeout-prone traces are recurring; prefer bounded reads, shorter commands, and non-blocking execution for long-running work.\n",
            "- [forge][2026-04-12T09:10:11Z] When the same fallback chain appears repeatedly, prefer the later successful tool earlier in the plan and justify the switch explicitly.\n"
        );

        assert!(content_contains_equivalent_hint(
            content,
            "Timeout-prone traces are recurring; prefer bounded reads, shorter commands, and non-blocking execution for long-running work."
        ));
        assert!(content_contains_equivalent_hint(
            content,
            "When the same fallback chain appears repeatedly, prefer the later successful tool earlier in the plan and justify the switch explicitly."
        ));
        assert!(!content_contains_equivalent_hint(
            content,
            "Approval friction is recurring; choose lower-blast-radius tools first and avoid unnecessary approval-triggering actions."
        ));
    }

    #[tokio::test]
    async fn apply_forge_hints_adds_timestamp_and_skips_equivalent_duplicates() -> anyhow::Result<()>
    {
        crate::agent::agent_identity::run_with_agent_scope(
            crate::agent::agent_identity::RADOGOST_AGENT_ID.to_string(),
            async {
                let root = std::env::temp_dir().join(format!("tamux-forge-test-{}", Uuid::new_v4()));
                let scope_id = current_agent_scope_id();
                ensure_memory_files_for_scope(&root, &scope_id).await?;
                let memory_path = memory_paths_for_scope(&root, &scope_id).memory_path;
                tokio::fs::write(&memory_path, "# Memory\n").await?;

                let hints = vec![
                    StrategyHint {
                        for_agent: "svarog".into(),
                        target: "bash_command -> read_file".into(),
                        hint: "When the same fallback chain appears repeatedly, prefer the later successful tool earlier in the plan and justify the switch explicitly.".into(),
                        priority: 4,
                        source_pattern: "tool_fallback_loop".into(),
                    },
                    StrategyHint {
                        for_agent: "svarog".into(),
                        target: "approval_risk".into(),
                        hint: "Approval friction is recurring; choose lower-blast-radius tools first and avoid unnecessary approval-triggering actions.".into(),
                        priority: 2,
                        source_pattern: "approval_friction".into(),
                    },
                ];

                let applied = apply_forge_hints(&hints, 3, &root, 10).await?;
                assert_eq!(applied.len(), 1);
                assert!(applied[0].starts_with("- [forge]["));

                let initial = tokio::fs::read_to_string(&memory_path).await?;
                assert!(initial.contains("- [forge]["));
                assert!(initial.contains("prefer the later successful tool earlier in the plan"));
                assert!(!initial.contains("Approval friction is recurring"));

                let reapplied = apply_forge_hints(&hints, 3, &root, 10).await?;
                assert!(reapplied.is_empty());

                let final_content = tokio::fs::read_to_string(&memory_path).await?;
                assert_eq!(
                    final_content
                        .matches("prefer the later successful tool earlier in the plan")
                        .count(),
                    1
                );

                fs::remove_dir_all(root)?;
                Ok(())
            },
        )
        .await
    }
}
