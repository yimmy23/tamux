//! Episodic recall over threads, transcripts, telemetry logs, and agent events.

use std::cmp::Reverse;
use std::path::PathBuf;

use serde_json::Value;

use super::*;

const MAX_THREAD_SCAN: usize = 40;
const MAX_MESSAGES_PER_THREAD: usize = 200;
const MAX_SNIPPETS_PER_GROUP: usize = 3;
const MAX_TELEMETRY_SCAN_LINES: usize = 400;
const MAX_EVENT_SCAN: usize = 200;

#[derive(Debug, Clone)]
struct RecallGroup {
    source: &'static str,
    id: String,
    title: String,
    timestamp: u64,
    score: usize,
    lineage: Option<String>,
    summary: String,
    snippets: Vec<String>,
}

pub(super) async fn execute_session_search(
    session_manager: &Arc<SessionManager>,
    query: &str,
    limit: usize,
) -> Result<String> {
    let tokens = query_tokens(query);
    if tokens.is_empty() {
        anyhow::bail!("query must include at least one searchable token");
    }

    let mut groups = Vec::new();
    groups.extend(recall_from_threads(session_manager, &tokens).await?);
    groups.extend(recall_from_transcripts(session_manager, &tokens).await?);
    groups.extend(recall_from_agent_events(session_manager, &tokens).await?);
    groups.extend(recall_from_telemetry("cognitive", &tokens)?);
    groups.extend(recall_from_telemetry("operational", &tokens)?);

    groups.sort_by_key(|group| (Reverse(group.score), Reverse(group.timestamp)));
    groups.truncate(limit.max(1));

    if groups.is_empty() {
        return Ok(format!(
            "No matching prior sessions, transcripts, cognitive traces, or operational logs for \"{query}\"."
        ));
    }

    let mut lines = Vec::new();
    lines.push(format!("Session search for \"{query}\""));
    lines.push(format!(
        "Found {} grouped recall result(s) across threads, transcripts, telemetry, and behavioral events.",
        groups.len()
    ));
    lines.push(String::new());
    for (index, group) in groups.iter().enumerate() {
        lines.push(format!(
            "{}. [{}] {} | id={} | ts={} | score={}",
            index + 1,
            group.source,
            group.title,
            group.id,
            group.timestamp,
            group.score
        ));
        if let Some(lineage) = group.lineage.as_deref().filter(|value| !value.is_empty()) {
            lines.push(format!("   Lineage: {lineage}"));
        }
        lines.push(format!("   Summary: {}", group.summary));
        for snippet in &group.snippets {
            lines.push(format!("   - {snippet}"));
        }
    }

    Ok(lines.join("\n"))
}

async fn recall_from_threads(
    session_manager: &SessionManager,
    tokens: &[String],
) -> Result<Vec<RecallGroup>> {
    let mut threads = session_manager.list_agent_threads().await?;
    threads.sort_by_key(|thread| Reverse(thread.updated_at));
    threads.truncate(MAX_THREAD_SCAN);

    let mut groups = Vec::new();
    for thread in threads {
        let messages =
            session_manager.list_agent_messages(&thread.id, Some(MAX_MESSAGES_PER_THREAD)).await?;
        let mut score = match_score(&thread.title, tokens);
        let mut snippets = Vec::new();
        let mut role_hits = Vec::new();
        for message in messages.iter().rev() {
            let content_score = match_score(&message.content, tokens);
            if content_score == 0 {
                continue;
            }
            score += content_score;
            role_hits.push(message.role.clone());
            if snippets.len() < MAX_SNIPPETS_PER_GROUP {
                snippets.push(build_snippet(&message.content, tokens));
            }
        }
        if score == 0 {
            continue;
        }
        let metadata = parse_thread_metadata(thread.metadata_json.as_deref());
        groups.push(RecallGroup {
            source: "thread",
            id: thread.id.clone(),
            title: thread.title.clone(),
            timestamp: thread.updated_at as u64,
            score,
            lineage: metadata.upstream_thread_id,
            summary: format!(
                "Thread with {} message(s); matched {} message(s) across roles: {}.",
                thread.message_count,
                role_hits.len(),
                summarize_roles(&role_hits)
            ),
            snippets,
        });
    }
    Ok(groups)
}

async fn recall_from_transcripts(
    session_manager: &SessionManager,
    tokens: &[String],
) -> Result<Vec<RecallGroup>> {
    let entries = session_manager.list_transcript_index(None).await?;
    let mut groups = Vec::new();
    for entry in entries.into_iter().take(MAX_THREAD_SCAN) {
        let mut combined = entry.preview.clone().unwrap_or_default();
        let transcript_path = PathBuf::from(&entry.filename);
        if transcript_path.exists() {
            if let Ok(content) = std::fs::read_to_string(&transcript_path) {
                if combined.is_empty() || match_score(&combined, tokens) == 0 {
                    combined = content;
                }
            }
        }
        let score = match_score(&combined, tokens);
        if score == 0 {
            continue;
        }
        groups.push(RecallGroup {
            source: "transcript",
            id: entry.id,
            title: entry
                .reason
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| entry.filename.clone()),
            timestamp: entry.captured_at as u64,
            score,
            lineage: None,
            summary: format!(
                "Transcript snapshot from {}{}.",
                entry.filename,
                entry
                    .pane_id
                    .as_deref()
                    .map(|pane| format!(" (pane {pane})"))
                    .unwrap_or_default()
            ),
            snippets: vec![build_snippet(&combined, tokens)],
        });
    }
    Ok(groups)
}

async fn recall_from_agent_events(
    session_manager: &SessionManager,
    tokens: &[String],
) -> Result<Vec<RecallGroup>> {
    let events =
        session_manager.list_agent_events(Some("behavioral"), None, Some(MAX_EVENT_SCAN)).await?;
    let mut groups = Vec::new();
    for event in events {
        let score = match_score(&event.payload_json, tokens) + match_score(&event.kind, tokens);
        if score == 0 {
            continue;
        }
        let payload = serde_json::from_str::<Value>(&event.payload_json).unwrap_or(Value::Null);
        groups.push(RecallGroup {
            source: "behavioral",
            id: event.id,
            title: event.kind,
            timestamp: event.timestamp as u64,
            score,
            lineage: payload
                .get("correlation_id")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            summary: "Behavioral event captured by the capability event bus.".to_string(),
            snippets: vec![build_snippet(&event.payload_json, tokens)],
        });
    }
    Ok(groups)
}

fn recall_from_telemetry(kind: &'static str, tokens: &[String]) -> Result<Vec<RecallGroup>> {
    let path = amux_protocol::ensure_amux_data_dir()?
        .join("semantic-logs")
        .join(format!("{kind}.jsonl"));
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = std::fs::read_to_string(path)?;
    let mut groups = Vec::new();
    for (index, line) in raw.lines().rev().take(MAX_TELEMETRY_SCAN_LINES).enumerate() {
        let score = match_score(line, tokens);
        if score == 0 {
            continue;
        }
        let value = serde_json::from_str::<Value>(line).unwrap_or(Value::Null);
        let timestamp = value.get("timestamp").and_then(Value::as_u64).unwrap_or(0);
        let title = value
            .get("execution_id")
            .and_then(Value::as_str)
            .map(|execution_id| format!("{kind}:{execution_id}"))
            .unwrap_or_else(|| format!("{kind}:{index}"));
        groups.push(RecallGroup {
            source: kind,
            id: title.clone(),
            title,
            timestamp,
            score,
            lineage: value
                .get("session_id")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned),
            summary: telemetry_summary(kind, &value),
            snippets: vec![build_snippet(line, tokens)],
        });
    }
    Ok(groups)
}

fn telemetry_summary(kind: &str, value: &Value) -> String {
    match kind {
        "operational" => {
            let command = value
                .get("command")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            let exit_code = value
                .get("exit_code")
                .and_then(Value::as_i64)
                .map(|code| code.to_string())
                .unwrap_or_else(|| "?".to_string());
            format!("Operational trace for `{command}` (exit {exit_code}).")
        }
        "cognitive" => {
            let rationale = value
                .get("rationale")
                .and_then(Value::as_str)
                .unwrap_or("no rationale recorded");
            format!(
                "Cognitive trace captured rationale: {}.",
                truncate_words(rationale, 12)
            )
        }
        other => format!("{other} telemetry entry."),
    }
}

fn summarize_roles(roles: &[String]) -> String {
    let mut counts = std::collections::BTreeMap::new();
    for role in roles {
        *counts.entry(role.clone()).or_insert(0usize) += 1;
    }
    counts
        .into_iter()
        .map(|(role, count)| format!("{role} x{count}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn build_snippet(text: &str, tokens: &[String]) -> String {
    let cleaned = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if cleaned.is_empty() {
        return String::new();
    }
    let lowered = cleaned.to_ascii_lowercase();
    let start_byte = tokens
        .iter()
        .filter_map(|token| lowered.find(token))
        .min()
        .unwrap_or(0);
    let start_char = lowered[..start_byte].chars().count().saturating_sub(48);
    cleaned
        .chars()
        .skip(start_char)
        .take(200)
        .collect::<String>()
        .trim()
        .to_string()
}

fn truncate_words(text: &str, max_words: usize) -> String {
    let words = text.split_whitespace().take(max_words).collect::<Vec<_>>();
    words.join(" ")
}

fn match_score(text: &str, tokens: &[String]) -> usize {
    if text.trim().is_empty() {
        return 0;
    }
    let lowered = text.to_ascii_lowercase();
    tokens
        .iter()
        .filter(|token| lowered.contains(token.as_str()))
        .count()
}

fn query_tokens(query: &str) -> Vec<String> {
    query
        .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_' && ch != '-')
        .filter_map(|token| {
            let token = token.trim().to_ascii_lowercase();
            if token.len() >= 2 {
                Some(token)
            } else {
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_tokens_drop_short_noise() {
        assert_eq!(
            query_tokens("did we fix rg in ui?"),
            vec!["did", "we", "fix", "rg", "in", "ui"]
        );
    }

    #[test]
    fn build_snippet_centers_match() {
        let snippet = build_snippet(
            "prefix words alpha beta gamma important-token delta",
            &[String::from("important-token")],
        );
        assert!(snippet.contains("important-token"));
    }
}
