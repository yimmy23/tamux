use sha2::{Digest, Sha256};

use crate::agent::morphogenesis::types::{AdaptationType, MorphogenesisAffinity, SoulAdaptation};

const SPECIALIZATION_HEADING: &str = "## Current Specialization";
const SPECIALIZATION_AFFINITY_THRESHOLD: f64 = 0.70;
const SPECIALIZATION_TASK_COUNT_THRESHOLD: u64 = 10;

fn specialization_key(domain: &str) -> String {
    format!("- {domain}:")
}

fn content_hash(content: &str) -> Option<String> {
    if content.is_empty() {
        return None;
    }
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    Some(format!("sha256:{:x}", hasher.finalize()))
}

pub(crate) fn is_specialized(affinity: &MorphogenesisAffinity) -> bool {
    affinity.affinity_score >= SPECIALIZATION_AFFINITY_THRESHOLD
        && affinity.task_count >= SPECIALIZATION_TASK_COUNT_THRESHOLD
}

pub(crate) fn render_specialization_snippet(
    affinities: &[MorphogenesisAffinity],
) -> Option<String> {
    let specialized = affinities
        .iter()
        .filter(|affinity| is_specialized(affinity))
        .collect::<Vec<_>>();
    if specialized.is_empty() {
        return None;
    }

    let mut lines = vec![SPECIALIZATION_HEADING.to_string()];
    for affinity in specialized {
        lines.push(format!(
            "- {}: {:.2} affinity across {} tasks ({} success / {} failure)",
            affinity.domain,
            affinity.affinity_score,
            affinity.task_count,
            affinity.success_count,
            affinity.failure_count
        ));
    }
    Some(lines.join("\n"))
}

fn replace_specialization_section(current_soul: &str, snippet: Option<&str>) -> String {
    let mut lines = current_soul.lines().map(str::to_string).collect::<Vec<_>>();
    let start = lines
        .iter()
        .position(|line| line.trim() == SPECIALIZATION_HEADING);
    if let Some(start_idx) = start {
        let mut end_idx = lines.len();
        for idx in (start_idx + 1)..lines.len() {
            if lines[idx].starts_with("## ") {
                end_idx = idx;
                break;
            }
        }
        lines.drain(start_idx..end_idx);
        while start_idx < lines.len() && lines[start_idx].trim().is_empty() {
            lines.remove(start_idx);
        }
    }

    let mut normalized = lines.join("\n").trim_end().to_string();
    if let Some(snippet) = snippet.filter(|snippet| !snippet.trim().is_empty()) {
        if !normalized.is_empty() {
            normalized.push_str("\n\n");
        }
        normalized.push_str(snippet.trim());
        normalized.push('\n');
    } else if !normalized.ends_with('\n') {
        normalized.push('\n');
    }
    normalized
}

pub(crate) fn apply_specialization_section(
    current_soul: &str,
    affinities: &[MorphogenesisAffinity],
) -> (String, Option<String>) {
    let snippet = render_specialization_snippet(affinities);
    let updated = replace_specialization_section(current_soul, snippet.as_deref());
    (updated, snippet)
}

pub(crate) fn build_soul_adaptation(
    agent_id: &str,
    domain: &str,
    old_affinity: Option<&MorphogenesisAffinity>,
    new_affinity: &MorphogenesisAffinity,
    current_soul: &str,
    all_affinities: &[MorphogenesisAffinity],
    created_at_ms: u64,
) -> Option<SoulAdaptation> {
    let old_specialized = old_affinity.is_some_and(is_specialized);
    let new_specialized = is_specialized(new_affinity);
    let adaptation_type = match (old_specialized, new_specialized) {
        (false, true) => AdaptationType::Added,
        (true, false) => AdaptationType::Removed,
        (true, true) => AdaptationType::Updated,
        (false, false) => return None,
    };

    let (updated_soul, snippet) = apply_specialization_section(current_soul, all_affinities);
    let snippet = match adaptation_type {
        AdaptationType::Removed => {
            let old_domain = specialization_key(domain);
            current_soul
                .lines()
                .find(|line| line.trim_start().starts_with(&old_domain))
                .map(|line| format!("{SPECIALIZATION_HEADING}\n{line}"))
                .unwrap_or_else(|| SPECIALIZATION_HEADING.to_string())
        }
        _ => snippet.unwrap_or_else(|| SPECIALIZATION_HEADING.to_string()),
    };

    Some(SoulAdaptation {
        agent_id: agent_id.to_string(),
        domain: domain.to_string(),
        adaptation_type,
        soul_snippet: snippet,
        old_soul_hash: content_hash(current_soul),
        new_soul_hash: content_hash(&updated_soul),
        created_at_ms,
    })
}
