use crate::agent::skill_recommendation::types::SkillDocumentMetadata;
use serde_yaml::Value;
use std::collections::BTreeSet;

pub(crate) fn extract_skill_metadata(relative_path: &str, content: &str) -> SkillDocumentMetadata {
    let (frontmatter, body) = split_frontmatter(content);
    let headings = extract_headings(&body);
    let mut keywords = extract_frontmatter_list(frontmatter.as_ref(), "keywords");
    let mut triggers = extract_frontmatter_list(frontmatter.as_ref(), "triggers");
    triggers.extend(extract_trigger_section(&body));

    dedupe_strings(&mut keywords);
    dedupe_strings(&mut triggers);

    let summary = extract_frontmatter_string(frontmatter.as_ref(), "description")
        .or_else(|| extract_frontmatter_string(frontmatter.as_ref(), "summary"))
        .or_else(|| extract_first_paragraph(&body));

    let search_text = build_search_text(
        relative_path,
        summary.as_deref(),
        &headings,
        &keywords,
        &triggers,
        &body,
        frontmatter.as_ref(),
    );

    SkillDocumentMetadata {
        summary,
        headings,
        keywords,
        triggers,
        search_text,
        built_in: relative_path.replace('\\', "/").starts_with("builtin/"),
    }
}

fn split_frontmatter(content: &str) -> (Option<Value>, String) {
    let mut lines = content.lines();
    if !matches!(lines.next().map(str::trim), Some("---")) {
        return (None, content.to_string());
    }

    let mut yaml = Vec::new();
    let mut body_start = None;
    for (index, line) in content.lines().enumerate().skip(1) {
        if line.trim() == "---" {
            body_start = Some(index + 1);
            break;
        }
        yaml.push(line);
    }

    let Some(body_start) = body_start else {
        return (None, content.to_string());
    };

    let parsed = serde_yaml::from_str::<Value>(&yaml.join("\n")).ok();
    let body = content
        .lines()
        .skip(body_start)
        .collect::<Vec<_>>()
        .join("\n");
    (parsed, body)
}

fn extract_headings(body: &str) -> Vec<String> {
    body.lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            let title = trimmed.trim_start_matches('#').trim();
            if trimmed.starts_with('#') && !title.is_empty() {
                Some(title.to_string())
            } else {
                None
            }
        })
        .collect()
}

fn extract_trigger_section(body: &str) -> Vec<String> {
    let mut triggers = Vec::new();
    let mut in_triggers = false;

    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            let heading = trimmed.trim_start_matches('#').trim().to_ascii_lowercase();
            in_triggers = heading == "triggers";
            continue;
        }
        if !in_triggers || trimmed.is_empty() {
            continue;
        }
        if let Some(item) = trimmed
            .strip_prefix("- ")
            .or_else(|| trimmed.strip_prefix("* "))
        {
            triggers.push(item.trim().to_string());
            continue;
        }
        if let Some((_, item)) = trimmed.split_once(". ") {
            if trimmed
                .chars()
                .next()
                .map(|value| value.is_ascii_digit())
                .unwrap_or(false)
            {
                triggers.push(item.trim().to_string());
                continue;
            }
        }
        if !trimmed.starts_with('<') {
            triggers.push(trimmed.to_string());
        }
    }

    triggers
}

fn extract_first_paragraph(body: &str) -> Option<String> {
    let mut paragraph = Vec::new();
    let mut started = false;
    for line in body.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if started {
                break;
            }
            continue;
        }
        if trimmed.starts_with('#') || trimmed.starts_with('<') || trimmed.starts_with("```") {
            continue;
        }
        paragraph.push(trimmed);
        started = true;
    }
    (!paragraph.is_empty()).then(|| paragraph.join(" "))
}

fn extract_frontmatter_string(frontmatter: Option<&Value>, key: &str) -> Option<String> {
    frontmatter
        .and_then(|value| value.as_mapping())
        .and_then(|mapping| mapping.get(&Value::String(key.to_string())))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn extract_frontmatter_list(frontmatter: Option<&Value>, key: &str) -> Vec<String> {
    let Some(value) = frontmatter
        .and_then(|data| data.as_mapping())
        .and_then(|mapping| mapping.get(&Value::String(key.to_string())))
    else {
        return Vec::new();
    };

    match value {
        Value::String(item) => vec![item.trim().to_string()],
        Value::Sequence(items) => items
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .map(ToOwned::to_owned)
            .collect(),
        _ => Vec::new(),
    }
}

fn build_search_text(
    relative_path: &str,
    summary: Option<&str>,
    headings: &[String],
    keywords: &[String],
    triggers: &[String],
    body: &str,
    frontmatter: Option<&Value>,
) -> String {
    let mut parts = vec![relative_path.replace('/', " ")];
    if let Some(name) = extract_frontmatter_string(frontmatter, "name") {
        parts.push(name);
    }
    if let Some(summary) = summary {
        parts.push(summary.to_string());
    }
    parts.extend(headings.iter().cloned());
    parts.extend(keywords.iter().cloned());
    parts.extend(triggers.iter().cloned());
    parts.push(
        body.lines()
            .filter_map(|line| {
                let trimmed = line.trim();
                if trimmed.starts_with('#')
                    || trimmed.starts_with('<')
                    || trimmed.starts_with("```")
                {
                    None
                } else {
                    Some(trimmed)
                }
            })
            .take(24)
            .collect::<Vec<_>>()
            .join(" "),
    );
    parts
        .into_iter()
        .filter(|part| !part.trim().is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn dedupe_strings(values: &mut Vec<String>) {
    let mut seen = BTreeSet::new();
    values.retain(|value| seen.insert(value.to_ascii_lowercase()));
}
