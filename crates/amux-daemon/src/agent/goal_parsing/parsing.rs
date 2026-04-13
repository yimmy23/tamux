use super::*;
use anyhow::Result;

/// Attempt to repair malformed JSON from LLM output using the jsonrepair crate.
pub(crate) fn repair_json(raw: &str) -> String {
    jsonrepair::repair_json(raw, &jsonrepair::Options::default())
        .unwrap_or_else(|_| raw.to_string())
}

/// JSON schema for structured output - forces the API to produce valid GoalPlanResponse.
pub(crate) fn goal_plan_json_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "title": { "type": "string" },
            "summary": { "type": "string" },
            "steps": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "title": { "type": "string" },
                        "instructions": { "type": "string" },
                        "kind": { "type": "string", "enum": ["reason", "command", "research", "memory", "skill", "divergent", "debate"] },
                        "success_criteria": { "type": "string" },
                        "session_id": { "type": ["string", "null"] },
                        "llm_confidence": { "type": ["string", "null"] },
                        "llm_confidence_rationale": { "type": ["string", "null"] }
                    },
                    "required": ["title", "instructions", "kind", "success_criteria", "session_id"],
                    "additionalProperties": false
                }
            },
            "rejected_alternatives": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Alternative approaches you considered but rejected, each with a brief reason. Keep the list short."
            }
        },
        "required": ["title", "summary", "steps", "rejected_alternatives"],
        "additionalProperties": false
    })
}

/// Parse a numbered markdown list into a GoalPlanResponse-compatible JSON value.
pub(crate) fn parse_markdown_steps<T: serde::de::DeserializeOwned>(raw: &str) -> Result<T> {
    let mut steps = Vec::new();

    for line in raw.lines() {
        let line = line.trim();
        let content = if let Some(rest) = line.strip_prefix(|c: char| c.is_ascii_digit()) {
            rest.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.')
                .trim()
        } else if let Some(rest) = line.strip_prefix("- ") {
            rest.trim()
        } else {
            continue;
        };

        if content.is_empty() {
            continue;
        }

        let (kind, rest) = if content.starts_with('[') {
            if let Some(close) = content.find(']') {
                let k = &content[1..close];
                let remainder = content[close + 1..].trim();
                (k.to_string(), remainder.to_string())
            } else {
                ("command".to_string(), content.to_string())
            }
        } else {
            ("command".to_string(), content.to_string())
        };

        let (main_part, criteria) = if let Some(pos) = rest.to_lowercase().find("success:") {
            (
                rest[..pos].trim().to_string(),
                rest[pos + 8..].trim().to_string(),
            )
        } else if let Some(pos) = rest.to_lowercase().find("criteria:") {
            (
                rest[..pos].trim().to_string(),
                rest[pos + 9..].trim().to_string(),
            )
        } else {
            (rest.clone(), "Step completed successfully".to_string())
        };

        let (title, instructions) = if let Some(colon) = main_part.find(':') {
            (
                main_part[..colon].trim().to_string(),
                main_part[colon + 1..].trim().to_string(),
            )
        } else {
            (main_part.clone(), main_part)
        };

        steps.push(serde_json::json!({
            "title": title,
            "instructions": instructions,
            "kind": kind,
            "success_criteria": criteria.trim_end_matches('.'),
            "session_id": null,
            "llm_confidence": null,
            "llm_confidence_rationale": null,
        }));
    }

    if steps.is_empty() {
        anyhow::bail!("no steps parsed from markdown");
    }

    let plan = serde_json::json!({
        "title": steps.first().and_then(|s| s["title"].as_str()).unwrap_or("Goal plan"),
        "summary": format!("Plan with {} steps parsed from markdown", steps.len()),
        "steps": steps,
    });

    serde_json::from_value::<T>(plan)
        .map_err(|e| anyhow::anyhow!("markdown plan conversion failed: {e}"))
}

pub(crate) fn parse_yaml_block<T: serde::de::DeserializeOwned>(raw: &str) -> Result<T> {
    let trimmed = raw.trim();

    if let Ok(parsed) = serde_yaml::from_str::<T>(trimmed) {
        return Ok(parsed);
    }

    let without_fence = trimmed
        .strip_prefix("```yaml")
        .or_else(|| trimmed.strip_prefix("```yml"))
        .or_else(|| trimmed.strip_prefix("```"))
        .map(str::trim)
        .and_then(|v| v.strip_suffix("```"))
        .map(str::trim)
        .unwrap_or(trimmed);

    if let Ok(parsed) = serde_yaml::from_str::<T>(without_fence) {
        return Ok(parsed);
    }

    anyhow::bail!("failed to parse YAML from model output")
}

/// Build a correction prompt when the model fails to return valid JSON.
pub(crate) fn build_json_retry_prompt(original_prompt: &str, broken_output: &str) -> String {
    format!(
        "Your previous response was not valid JSON and could not be parsed.\n\
         Here is what you returned:\n\
         ---\n{}\n---\n\n\
         Please try again. Return ONLY the raw JSON object, no markdown fences, no explanation.\n\n\
         Original request:\n{}",
        broken_output.chars().take(2000).collect::<String>(),
        original_prompt
    )
}

pub(crate) fn parse_json_block<T: serde::de::DeserializeOwned>(raw: &str) -> Result<T> {
    let trimmed = raw.trim();
    if let Ok(parsed) = serde_json::from_str::<T>(trimmed) {
        return Ok(parsed);
    }

    let without_fence = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .map(str::trim)
        .and_then(|value| value.strip_suffix("```"))
        .map(str::trim)
        .unwrap_or(trimmed);

    if let Ok(parsed) = serde_json::from_str::<T>(without_fence) {
        return Ok(parsed);
    }

    let object_candidate = without_fence
        .find('{')
        .zip(without_fence.rfind('}'))
        .and_then(|(start, end)| (start < end).then_some(&without_fence[start..=end]));
    if let Some(candidate) = object_candidate {
        if let Ok(parsed) = serde_json::from_str::<T>(candidate) {
            return Ok(parsed);
        }
    }

    // Try unwrapping {"answer":"..."} wrapper pattern
    if let Some(candidate) = object_candidate {
        if let Ok(wrapper) = serde_json::from_str::<serde_json::Value>(candidate) {
            if let Some(inner) = wrapper.get("answer").and_then(|v| v.as_str()) {
                if let Ok(parsed) = serde_json::from_str::<T>(inner) {
                    tracing::info!("parsed JSON after unwrapping answer wrapper");
                    return Ok(parsed);
                }
                let inner_repaired = repair_json(inner);
                if let Ok(parsed) = serde_json::from_str::<T>(&inner_repaired) {
                    tracing::info!("parsed JSON after unwrapping + repairing answer wrapper");
                    return Ok(parsed);
                }
            }
        }
    }

    // Try repairing the JSON using jsonrepair
    let repaired = repair_json(without_fence);
    if let Ok(parsed) = serde_json::from_str::<T>(&repaired) {
        tracing::info!("parsed JSON after jsonrepair");
        return Ok(parsed);
    }

    tracing::warn!(raw_len = raw.len(), raw_output = %raw, "failed to parse structured JSON from model output");
    anyhow::bail!("failed to parse structured JSON from model output")
}
