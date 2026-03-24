//! Persistent memory helpers for SOUL.md, MEMORY.md, and USER.md.

use super::*;
use crate::history::{HistoryStore, MemoryProvenanceRecord};

const SOUL_LIMIT_CHARS: usize = 1_500;
const MEMORY_LIMIT_CHARS: usize = 2_200;
const USER_LIMIT_CHARS: usize = 1_375;

const DEFAULT_SOUL: &str = "# Identity
I am tamux's built-in agent. I help operators manage terminal sessions, tasks, goals, and cross-session memory.

# Principles
- Be concise and high-signal.
- Show risk and blast radius before risky execution.
- Treat memory as curated state, not a diary.
";

const DEFAULT_MEMORY: &str = "# Memory
Stable workspace facts, conventions, and learned patterns belong here.
";

const DEFAULT_USER: &str = "# User
Stable operator preferences, constraints, and workflow habits belong here.
";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MemoryTarget {
    Soul,
    Memory,
    User,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MemoryUpdateMode {
    Replace,
    Append,
    Remove,
}

pub(super) struct MemoryWriteContext<'a> {
    pub source_kind: &'a str,
    pub thread_id: Option<&'a str>,
    pub task_id: Option<&'a str>,
    pub goal_run_id: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MemoryFactCandidate {
    pub(super) key: String,
    pub(super) normalized: String,
    pub(super) display: String,
}

impl MemoryTarget {
    pub(super) fn parse(raw: &str) -> Result<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "soul" => Ok(Self::Soul),
            "memory" => Ok(Self::Memory),
            "user" | "user_profile" => Ok(Self::User),
            other => Err(anyhow::anyhow!(
                "invalid memory target `{other}`; expected soul, memory, or user"
            )),
        }
    }

    pub(super) fn file_name(self) -> &'static str {
        match self {
            Self::Soul => "SOUL.md",
            Self::Memory => "MEMORY.md",
            Self::User => "USER.md",
        }
    }

    pub(super) fn label(self) -> &'static str {
        self.file_name()
    }

    pub(super) fn limit_chars(self) -> usize {
        match self {
            Self::Soul => SOUL_LIMIT_CHARS,
            Self::Memory => MEMORY_LIMIT_CHARS,
            Self::User => USER_LIMIT_CHARS,
        }
    }

    fn default_content(self) -> &'static str {
        match self {
            Self::Soul => DEFAULT_SOUL,
            Self::Memory => DEFAULT_MEMORY,
            Self::User => DEFAULT_USER,
        }
    }
}

impl MemoryUpdateMode {
    pub(super) fn parse(raw: &str) -> Result<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "replace" => Ok(Self::Replace),
            "append" => Ok(Self::Append),
            "remove" => Ok(Self::Remove),
            other => Err(anyhow::anyhow!(
                "invalid memory mode `{other}`; expected replace, append, or remove"
            )),
        }
    }
}

pub(super) fn memory_curation_guidance() -> &'static str {
    "SAVE:
- user preferences, constraints, and workflow habits
- stable workspace facts and project conventions
- recurring corrections and learned patterns

DO NOT SAVE:
- task progress or work-in-progress state
- temporary TODOs or short-lived outcomes
- details that can be trivially rediscovered from the environment"
}

pub(super) async fn ensure_memory_files(agent_data_dir: &std::path::Path) -> Result<()> {
    let memory_dir = active_memory_dir(agent_data_dir);
    tokio::fs::create_dir_all(&memory_dir).await?;

    for target in [MemoryTarget::Soul, MemoryTarget::Memory, MemoryTarget::User] {
        let path = memory_dir.join(target.file_name());
        if !path.exists() {
            tokio::fs::write(&path, target.default_content()).await?;
        }
    }

    Ok(())
}

pub(super) async fn apply_memory_update(
    agent_data_dir: &std::path::Path,
    history: &HistoryStore,
    target: MemoryTarget,
    mode: MemoryUpdateMode,
    content: &str,
    context: MemoryWriteContext<'_>,
) -> Result<String> {
    ensure_memory_files(agent_data_dir).await?;

    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Err(anyhow::anyhow!("memory content must not be empty"));
    }

    let memory_dir = active_memory_dir(agent_data_dir);
    let path = memory_dir.join(target.file_name());
    let existing = tokio::fs::read_to_string(&path).await.unwrap_or_default();

    if matches!(mode, MemoryUpdateMode::Append | MemoryUpdateMode::Replace) {
        validate_no_memory_contradictions(target, &existing, trimmed)?;
    }

    let next = match mode {
        MemoryUpdateMode::Replace => trimmed.to_string(),
        MemoryUpdateMode::Append => append_content(&existing, trimmed),
        MemoryUpdateMode::Remove => remove_content(&existing, trimmed)?,
    };

    validate_memory_size(target, &next)?;
    tokio::fs::write(&path, &next).await?;
    record_memory_provenance(history, target, mode, trimmed, &context).await?;

    Ok(format!(
        "Updated {} using {} mode ({} / {} chars).",
        target.label(),
        match mode {
            MemoryUpdateMode::Replace => "replace",
            MemoryUpdateMode::Append => "append",
            MemoryUpdateMode::Remove => "remove",
        },
        next.chars().count(),
        target.limit_chars()
    ))
}

pub(super) async fn append_goal_memory_note(
    agent_data_dir: &std::path::Path,
    history: &HistoryStore,
    update: &str,
    goal_run_id: Option<&str>,
) -> Result<()> {
    ensure_memory_files(agent_data_dir).await?;

    let trimmed = update.trim();
    if trimmed.is_empty() {
        return Ok(());
    }

    let path = active_memory_dir(agent_data_dir).join(MemoryTarget::Memory.file_name());
    let existing = tokio::fs::read_to_string(&path).await.unwrap_or_default();
    if existing.contains(trimmed) {
        return Ok(());
    }
    validate_no_memory_contradictions(MemoryTarget::Memory, &existing, trimmed)?;

    let heading = "## Learned During Goal Runs";
    let bullet = format!("- {trimmed}");
    let mut next = existing.trim_end().to_string();
    if next.is_empty() {
        next.push_str(DEFAULT_MEMORY.trim_end());
    }
    if !next.contains(heading) {
        if !next.ends_with('\n') {
            next.push('\n');
        }
        next.push('\n');
        next.push_str(heading);
        next.push('\n');
    }
    if !next.ends_with('\n') {
        next.push('\n');
    }
    next.push_str(&bullet);
    next.push('\n');

    validate_memory_size(MemoryTarget::Memory, &next)?;
    tokio::fs::write(&path, next).await?;
    record_memory_provenance(
        history,
        MemoryTarget::Memory,
        MemoryUpdateMode::Append,
        trimmed,
        &MemoryWriteContext {
            source_kind: "goal_reflection",
            thread_id: None,
            task_id: None,
            goal_run_id,
        },
    ).await?;
    Ok(())
}

fn append_content(existing: &str, addition: &str) -> String {
    let existing = existing.trim_end();
    if existing.is_empty() {
        addition.to_string()
    } else {
        format!("{existing}\n\n{addition}")
    }
}

fn remove_content(existing: &str, fragment: &str) -> Result<String> {
    if !existing.contains(fragment) {
        return Err(anyhow::anyhow!(
            "requested memory fragment was not found; use exact text for remove mode"
        ));
    }

    let updated = existing.replace(fragment, "");
    Ok(updated
        .lines()
        .collect::<Vec<_>>()
        .join("\n")
        .replace("\n\n\n", "\n\n")
        .trim()
        .to_string())
}

fn validate_memory_size(target: MemoryTarget, content: &str) -> Result<()> {
    let chars = content.chars().count();
    let limit = target.limit_chars();
    if chars > limit {
        return Err(anyhow::anyhow!(
            "{} would exceed its limit ({} > {} chars). Use remove or replace to make room.\n\n{}",
            target.label(),
            chars,
            limit,
            memory_curation_guidance()
        ));
    }
    Ok(())
}

fn validate_no_memory_contradictions(
    target: MemoryTarget,
    existing: &str,
    incoming: &str,
) -> Result<()> {
    let existing_facts = extract_memory_fact_candidates(existing);
    let incoming_facts = extract_memory_fact_candidates(incoming);
    if existing_facts.is_empty() || incoming_facts.is_empty() {
        return Ok(());
    }

    let mut contradictions = Vec::new();
    for candidate in incoming_facts {
        if let Some(current) = existing_facts
            .iter()
            .find(|fact| fact.key == candidate.key && fact.normalized != candidate.normalized)
        {
            contradictions.push((current.display.clone(), candidate.display.clone()));
        }
    }
    contradictions.dedup();

    if contradictions.is_empty() {
        return Ok(());
    }

    let details = contradictions
        .into_iter()
        .take(3)
        .map(|(current, proposed)| format!("- current: {current}\n  proposed: {proposed}"))
        .collect::<Vec<_>>()
        .join("\n");
    Err(anyhow::anyhow!(
        "Potential contradiction detected while updating {}.\n{}\nUse remove mode to clear the old fact before writing a conflicting replacement.",
        target.label(),
        details
    ))
}

async fn record_memory_provenance(
    history: &HistoryStore,
    target: MemoryTarget,
    mode: MemoryUpdateMode,
    content: &str,
    context: &MemoryWriteContext<'_>,
) -> Result<()> {
    let fact_keys = extract_memory_fact_candidates(content)
        .into_iter()
        .map(|candidate| candidate.key)
        .collect::<Vec<_>>();
    history.record_memory_provenance(&MemoryProvenanceRecord {
        id: &format!("memprov_{}", Uuid::new_v4()),
        target: target.label(),
        mode: match mode {
            MemoryUpdateMode::Replace => "replace",
            MemoryUpdateMode::Append => "append",
            MemoryUpdateMode::Remove => "remove",
        },
        source_kind: context.source_kind,
        content,
        fact_keys: &fact_keys,
        thread_id: context.thread_id,
        task_id: context.task_id,
        goal_run_id: context.goal_run_id,
        created_at: now_millis(),
    }).await
}

pub(super) fn extract_memory_fact_candidates(content: &str) -> Vec<MemoryFactCandidate> {
    let mut facts = Vec::new();
    for raw_line in content.lines() {
        let cleaned = strip_memory_markup(raw_line);
        if cleaned.is_empty() || cleaned.starts_with('#') {
            continue;
        }
        let Some(key) = derive_fact_key(&cleaned) else {
            continue;
        };
        let normalized = normalize_fact_text(&cleaned);
        if normalized.is_empty() {
            continue;
        }
        facts.push(MemoryFactCandidate {
            key,
            normalized,
            display: cleaned,
        });
    }
    facts.sort_by(|left, right| {
        left.key
            .cmp(&right.key)
            .then(left.normalized.cmp(&right.normalized))
    });
    facts.dedup();
    facts
}

fn strip_memory_markup(line: &str) -> String {
    let mut cleaned = line.trim();
    while let Some(rest) = cleaned.strip_prefix(['-', '*', '>', ' ']) {
        cleaned = rest.trim_start();
    }
    let bytes = cleaned.as_bytes();
    let mut idx = 0usize;
    while idx < bytes.len() && bytes[idx].is_ascii_digit() {
        idx += 1;
    }
    if idx > 0 && bytes.get(idx) == Some(&b'.') {
        cleaned = cleaned[idx + 1..].trim_start();
    }
    cleaned
        .trim_matches('`')
        .trim_matches('*')
        .trim_matches('_')
        .trim()
        .to_string()
}

fn derive_fact_key(line: &str) -> Option<String> {
    if let Some((left, _)) = line.split_once(':') {
        let key = normalize_fact_text(left);
        if (2..=80).contains(&key.len()) {
            return Some(key);
        }
    }

    for separator in [
        " is ",
        " are ",
        " uses ",
        " use ",
        " prefers ",
        " prefer ",
        " runs on ",
        " run on ",
        " lives in ",
        " work in ",
        " works in ",
        " = ",
    ] {
        if let Some((left, _)) = line.split_once(separator) {
            let key = normalize_fact_text(left);
            if (2..=80).contains(&key.len()) {
                return Some(key);
            }
        }
    }

    None
}

fn normalize_fact_text(value: &str) -> String {
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

// ---------------------------------------------------------------------------
// Memory fact supersession (D-06, Pitfall 2)
// ---------------------------------------------------------------------------

impl super::engine::AgentEngine {
    /// Supersede a memory fact with tombstone-before-update ordering per D-06.
    ///
    /// 1. Write tombstone to SQLite FIRST (durable, crash-safe per Pitfall 2)
    /// 2. Mark the original fact with `## [SUPERSEDED]` prefix in the memory file
    /// 3. Append replacement content if non-empty
    /// 4. Record provenance event for audit trail (MEMO-04)
    pub(super) async fn supersede_memory_fact(
        &self,
        target: MemoryTarget,
        original_content: &str,
        fact_key: &str,
        replacement_content: &str,
        source_kind: &str,
    ) -> Result<()> {
        let tombstone_id = format!("tomb_{}", Uuid::new_v4());
        let now = now_millis();

        // Step 1: Write tombstone FIRST (durable, per Pitfall 2)
        self.history
            .insert_memory_tombstone(
                &tombstone_id,
                target.label(),
                original_content,
                Some(fact_key),
                if replacement_content.is_empty() {
                    None
                } else {
                    Some(replacement_content)
                },
                source_kind,
                None,
                now,
            )
            .await?;

        // Step 2: Mark the original fact with [SUPERSEDED] prefix in the memory file.
        // D-06 says: "Superseded facts get a `## [SUPERSEDED]` prefix and are moved to
        // the tombstone table." The file only grows or has lines replaced -- never shrinks.
        let superseded_content = format!("## [SUPERSEDED]\n{}", original_content);

        let memory_path = active_memory_dir(&self.data_dir).join(target.file_name());
        let existing = tokio::fs::read_to_string(&memory_path)
            .await
            .unwrap_or_default();
        let updated = existing.replace(original_content, &superseded_content);

        // If replacement content is non-empty, append it as a new section
        let final_content = if replacement_content.is_empty() {
            updated
        } else {
            format!("{}\n\n{}", updated, replacement_content)
        };

        tokio::fs::write(&memory_path, &final_content).await?;

        // Step 3: Record provenance (audit trail, MEMO-04)
        self.record_provenance_event(
            "memory_consolidation",
            &format!(
                "Superseded fact '{}' in {} with [SUPERSEDED] marker",
                fact_key,
                target.label()
            ),
            serde_json::json!({
                "tombstone_id": tombstone_id,
                "fact_key": fact_key,
                "has_replacement": !replacement_content.is_empty()
            }),
            None,
            None,
            None,
            None,
            None,
        )
        .await;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_memory_size_rejects_over_limit() {
        let err = validate_memory_size(MemoryTarget::Soul, &"x".repeat(1_501)).unwrap_err();
        assert!(err.to_string().contains("SOUL.md would exceed its limit"));
    }

    #[test]
    fn append_content_separates_blocks() {
        assert_eq!(append_content("alpha", "beta"), "alpha\n\nbeta");
    }

    #[test]
    fn remove_content_requires_exact_match() {
        let err = remove_content("alpha", "beta").unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn extract_fact_candidates_uses_subject_before_colon() {
        let facts = extract_memory_fact_candidates("- Shell: bash\n- editor: helix");
        assert_eq!(facts.len(), 2);
        assert_eq!(facts[0].key, "editor");
        assert_eq!(facts[1].key, "shell");
    }

    #[test]
    fn contradiction_detection_blocks_conflicting_subject_fact() {
        let err =
            validate_no_memory_contradictions(MemoryTarget::User, "- shell: bash", "- shell: zsh")
                .unwrap_err();
        assert!(err.to_string().contains("Potential contradiction detected"));
    }

    #[test]
    fn contradiction_detection_allows_matching_fact() {
        validate_no_memory_contradictions(
            MemoryTarget::Memory,
            "- package manager: cargo",
            "- package manager: cargo",
        )
        .expect("identical facts should not conflict");
    }
}
