use crate::history::ExecutionTraceRow;

use super::types::GenePoolCandidate;

pub(crate) fn parse_tool_sequence(trace: &ExecutionTraceRow) -> Vec<String> {
    trace
        .tool_sequence_json
        .as_deref()
        .and_then(|json| serde_json::from_str::<Vec<String>>(json).ok())
        .unwrap_or_default()
        .into_iter()
        .filter(|tool| !tool.trim().is_empty())
        .collect()
}

pub(crate) fn build_candidate(
    trace: &ExecutionTraceRow,
    infer_tags: impl Fn(&[String], &str) -> Vec<String>,
    sanitize_name: impl Fn(&str) -> String,
) -> Option<GenePoolCandidate> {
    let task_type = trace
        .task_type
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("unknown");
    let tool_sequence = parse_tool_sequence(trace);
    if tool_sequence.is_empty() {
        return None;
    }

    let prefix = tool_sequence
        .iter()
        .take(2)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join("-");
    let proposed_skill_name = sanitize_name(&format!("{task_type}-{prefix}"));
    let proposed_skill_name = if proposed_skill_name.is_empty() {
        sanitize_name(task_type)
    } else {
        proposed_skill_name
    };

    Some(GenePoolCandidate {
        trace_id: trace.id.clone(),
        proposed_skill_name,
        task_type: task_type.to_string(),
        context_tags: infer_tags(&tool_sequence, task_type),
        quality_score: trace.quality_score.unwrap_or(0.0),
        tool_sequence,
    })
}
