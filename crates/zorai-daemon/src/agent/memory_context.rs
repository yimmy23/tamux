use crate::agent::task_prompt::MemoryPaths;
use crate::agent::types::AgentMemory;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::UNIX_EPOCH;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptMemoryInjectionState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_markdown_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_markdown_updated_at_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub soul_markdown_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub soul_markdown_updated_at_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_markdown_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_markdown_updated_at_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_markdown_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_markdown_updated_at_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structured_summary_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_markdown_injected_at_ms: Option<u64>,
    #[serde(default)]
    pub injected_after_compaction: bool,
}

impl PromptMemoryInjectionState {
    pub fn is_base_layer_injected(&self) -> bool {
        self.base_markdown_injected_at_ms.is_some()
    }

    pub fn is_base_layer_stale(
        &self,
        current_base_markdown_hash: Option<&str>,
        current_base_markdown_updated_at_ms: Option<u64>,
    ) -> bool {
        if !self.is_base_layer_injected() {
            return false;
        }

        self.base_markdown_hash.as_deref() != current_base_markdown_hash
            || self.base_markdown_updated_at_ms != current_base_markdown_updated_at_ms
    }

    pub fn is_structured_summary_stale(
        &self,
        current_structured_summary_hash: Option<&str>,
    ) -> bool {
        if !self.is_base_layer_injected() {
            return false;
        }

        self.structured_summary_hash.as_deref() != current_structured_summary_hash
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct StructuredMemorySummary {
    pub(crate) rendered_markdown: String,
    pub(crate) built_at_ms: u64,
    pub(crate) base_markdown_hash: Option<String>,
    pub(crate) base_markdown_updated_at_ms: Option<u64>,
    pub(crate) soul_markdown_hash: Option<String>,
    pub(crate) soul_markdown_updated_at_ms: Option<u64>,
    pub(crate) memory_markdown_hash: Option<String>,
    pub(crate) memory_markdown_updated_at_ms: Option<u64>,
    pub(crate) user_markdown_hash: Option<String>,
    pub(crate) user_markdown_updated_at_ms: Option<u64>,
    pub(crate) structured_summary_hash: Option<String>,
}

fn now_ms() -> u64 {
    chrono::Utc::now().timestamp_millis().max(0) as u64
}

fn file_updated_at_ms(path: &std::path::Path) -> Option<u64> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    let duration = modified.duration_since(UNIX_EPOCH).ok()?;
    Some(duration.as_millis() as u64)
}

fn compute_content_hash(content: &str) -> Option<String> {
    if content.trim().is_empty() {
        return None;
    }
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    Some(format!("sha256:{:x}", hasher.finalize()))
}

fn compute_base_markdown_hash(memory: &AgentMemory) -> Option<String> {
    compute_content_hash(&format!(
        "{}\n--\n{}\n--\n{}",
        memory.soul, memory.memory, memory.user_profile
    ))
}

fn compute_structured_summary_hash(
    base_markdown_hash: Option<&str>,
    continuity_summary: Option<&str>,
    negative_constraints: Option<&str>,
) -> Option<String> {
    let continuity = continuity_summary.unwrap_or_default().trim();
    let negative_constraints = negative_constraints.unwrap_or_default().trim();

    if base_markdown_hash.is_none() && continuity.is_empty() && negative_constraints.is_empty() {
        return None;
    }

    let mut hasher = Sha256::new();
    hasher.update(base_markdown_hash.unwrap_or_default().as_bytes());
    hasher.update(b"\n--continuity--\n");
    hasher.update(continuity.as_bytes());
    hasher.update(b"\n--constraints--\n");
    hasher.update(negative_constraints.as_bytes());
    Some(format!("sha256:{:x}", hasher.finalize()))
}

fn max_timestamp(values: impl IntoIterator<Item = Option<u64>>) -> Option<u64> {
    values.into_iter().flatten().max()
}

fn collect_summary_lines(content: &str, limit: usize) -> Vec<String> {
    content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .take(limit)
        .map(|line| {
            if line.starts_with("- ") {
                line.to_string()
            } else {
                format!("- {line}")
            }
        })
        .collect()
}

fn push_section(markdown: &mut String, title: &str, lines: Vec<String>) {
    if lines.is_empty() {
        return;
    }
    if !markdown.is_empty() {
        markdown.push_str("\n\n");
    }
    markdown.push_str(title);
    markdown.push('\n');
    for line in lines {
        markdown.push_str(&line);
        markdown.push('\n');
    }
    if markdown.ends_with('\n') {
        markdown.pop();
    }
}

pub(crate) fn build_structured_memory_summary(
    memory: &AgentMemory,
    memory_paths: &MemoryPaths,
    continuity_summary: Option<&str>,
    negative_constraints: Option<&str>,
) -> StructuredMemorySummary {
    let built_at_ms = now_ms();
    let soul_updated_at_ms = file_updated_at_ms(&memory_paths.soul_path);
    let memory_updated_at_ms = file_updated_at_ms(&memory_paths.memory_path);
    let user_updated_at_ms = file_updated_at_ms(&memory_paths.user_path);
    let base_markdown_updated_at_ms =
        max_timestamp([soul_updated_at_ms, memory_updated_at_ms, user_updated_at_ms]);

    let mut rendered_markdown = String::new();
    rendered_markdown.push_str("## Structured Memory Summary");

    push_section(
        &mut rendered_markdown,
        "## Soul Summary",
        collect_summary_lines(&memory.soul, 3),
    );
    push_section(
        &mut rendered_markdown,
        "## Memory Summary",
        collect_summary_lines(&memory.memory, 5),
    );
    push_section(
        &mut rendered_markdown,
        "## User Summary",
        collect_summary_lines(&memory.user_profile, 4),
    );
    push_section(
        &mut rendered_markdown,
        "## Continuity Summary",
        collect_summary_lines(continuity_summary.unwrap_or_default(), 4),
    );
    push_section(
        &mut rendered_markdown,
        "## Active Constraints",
        collect_summary_lines(negative_constraints.unwrap_or_default(), 4),
    );

    let freshness_lines = vec![
        format!("- Summary built at (ms): {built_at_ms}"),
        format!(
            "- SOUL.md updated at (ms): {}",
            soul_updated_at_ms
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        ),
        format!(
            "- MEMORY.md updated at (ms): {}",
            memory_updated_at_ms
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        ),
        format!(
            "- USER.md updated at (ms): {}",
            user_updated_at_ms
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        ),
    ];
    push_section(
        &mut rendered_markdown,
        "## Freshness Summary",
        freshness_lines,
    );

    StructuredMemorySummary {
        rendered_markdown,
        built_at_ms,
        base_markdown_hash: compute_base_markdown_hash(memory),
        base_markdown_updated_at_ms,
        soul_markdown_hash: compute_content_hash(&memory.soul),
        soul_markdown_updated_at_ms: soul_updated_at_ms,
        memory_markdown_hash: compute_content_hash(&memory.memory),
        memory_markdown_updated_at_ms: memory_updated_at_ms,
        user_markdown_hash: compute_content_hash(&memory.user_profile),
        user_markdown_updated_at_ms: user_updated_at_ms,
        structured_summary_hash: compute_structured_summary_hash(
            compute_base_markdown_hash(memory).as_deref(),
            continuity_summary,
            negative_constraints,
        ),
    }
}

pub(crate) fn build_prompt_memory_injection_state(
    summary: &StructuredMemorySummary,
    injected_after_compaction: bool,
) -> PromptMemoryInjectionState {
    PromptMemoryInjectionState {
        base_markdown_hash: summary.base_markdown_hash.clone(),
        base_markdown_updated_at_ms: summary.base_markdown_updated_at_ms,
        soul_markdown_hash: summary.soul_markdown_hash.clone(),
        soul_markdown_updated_at_ms: summary.soul_markdown_updated_at_ms,
        memory_markdown_hash: summary.memory_markdown_hash.clone(),
        memory_markdown_updated_at_ms: summary.memory_markdown_updated_at_ms,
        user_markdown_hash: summary.user_markdown_hash.clone(),
        user_markdown_updated_at_ms: summary.user_markdown_updated_at_ms,
        structured_summary_hash: summary.structured_summary_hash.clone(),
        base_markdown_injected_at_ms: Some(summary.built_at_ms),
        injected_after_compaction,
    }
}

pub(crate) fn should_inject_memory_context(
    existing_state: Option<&PromptMemoryInjectionState>,
    summary: &StructuredMemorySummary,
) -> bool {
    match existing_state {
        Some(state) => {
            !state.is_base_layer_injected()
                || state.is_base_layer_stale(
                    summary.base_markdown_hash.as_deref(),
                    summary.base_markdown_updated_at_ms,
                )
                || state.is_structured_summary_stale(summary.structured_summary_hash.as_deref())
        }
        None => true,
    }
}

pub(crate) fn append_structured_memory_summary_if_needed(
    prompt: &mut String,
    existing_state: Option<&PromptMemoryInjectionState>,
    summary: &StructuredMemorySummary,
    injected_after_compaction: bool,
) -> Option<PromptMemoryInjectionState> {
    if !should_inject_memory_context(existing_state, summary) {
        return None;
    }

    prompt.push_str("\n\n");
    prompt.push_str(&summary.rendered_markdown);
    Some(build_prompt_memory_injection_state(
        summary,
        injected_after_compaction,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_reinject_when_structured_summary_changes_without_base_markdown_change() {
        let root = tempfile::tempdir().expect("tempdir");
        let memory_paths = crate::agent::task_prompt::memory_paths_for_scope(
            root.path(),
            crate::agent::agent_identity::MAIN_AGENT_ID,
        );
        std::fs::create_dir_all(&memory_paths.memory_dir).expect("create memory dir");
        if let Some(parent) = memory_paths.user_path.parent() {
            std::fs::create_dir_all(parent).expect("create user dir");
        }
        std::fs::write(&memory_paths.soul_path, "# Soul\n\n- Stable soul fact\n")
            .expect("write soul");
        std::fs::write(
            &memory_paths.memory_path,
            "# Memory\n\n- Stable memory fact\n",
        )
        .expect("write memory");
        std::fs::write(&memory_paths.user_path, "# User\n\n- Stable user fact\n")
            .expect("write user");

        let memory = crate::agent::types::AgentMemory {
            soul: "# Soul\n\n- Stable soul fact\n".to_string(),
            memory: "# Memory\n\n- Stable memory fact\n".to_string(),
            user_profile: "# User\n\n- Stable user fact\n".to_string(),
        };

        let initial_summary = build_structured_memory_summary(
            &memory,
            &memory_paths,
            Some("## Working Continuity\n- Keep using approach alpha"),
            Some("## Ruled-out approaches\n- Do not retry beta"),
        );
        let existing_state = build_prompt_memory_injection_state(&initial_summary, false);

        let refreshed_summary = build_structured_memory_summary(
            &memory,
            &memory_paths,
            Some("## Working Continuity\n- Switch to approach gamma"),
            Some("## Ruled-out approaches\n- Do not retry beta"),
        );

        assert!(
            should_inject_memory_context(Some(&existing_state), &refreshed_summary),
            "summary changes outside base markdown should force reinjection"
        );
    }
}
