use super::*;
use crate::history::SkillVariantRecord;
use amux_protocol::SessionId;
use std::collections::BTreeSet;
use std::path::Path;

const MAX_SKILL_PREFLIGHT_MATCHES: usize = 3;
const MAX_SKILL_PREFLIGHT_LINES: usize = 40;
const MAX_SKILL_PREFLIGHT_CHARS: usize = 2400;

impl AgentEngine {
    pub(super) async fn build_skill_preflight_context(
        &self,
        content: &str,
        session_id: Option<SessionId>,
    ) -> Result<Option<String>> {
        if !should_run_skill_preflight(content) {
            return Ok(None);
        }

        let skills_root = skills_dir(&self.data_dir);
        sync_skill_catalog(&skills_root, &self.history).await?;
        let context_tags = resolve_skill_context_tags(&self.session_manager, session_id).await;
        let matches = select_skill_matches(&self.history, &skills_root, content, &context_tags).await?;
        if matches.is_empty() {
            return Ok(None);
        }

        let mut body = String::from(
            "Daemon skill preflight loaded these local skills before tool execution. Reuse them when they fit the task.\n",
        );
        for skill_match in matches {
            let tags = if skill_match.record.context_tags.is_empty() {
                "none".to_string()
            } else {
                skill_match.record.context_tags.join(", ")
            };
            body.push_str(&format!(
                "\n- {} [{} | status={} | uses={} | success={:.0}% | tags={}]\n  Reason: {}\n  Path: {}\n{}\n",
                skill_match.record.skill_name,
                skill_match.record.variant_name,
                skill_match.record.status,
                skill_match.record.use_count,
                skill_match.record.success_rate() * 100.0,
                tags,
                skill_match.reason,
                skill_match.record.relative_path,
                skill_match.excerpt
            ));
        }

        Ok(Some(body))
    }
}

struct SkillPreflightMatch {
    record: SkillVariantRecord,
    reason: String,
    excerpt: String,
    score: i32,
}

async fn select_skill_matches(
    history: &HistoryStore,
    skills_root: &Path,
    content: &str,
    context_tags: &[String],
) -> Result<Vec<SkillPreflightMatch>> {
    let request_tokens = tokenize(content);
    let request_text = content.to_ascii_lowercase();
    let tool_heavy_request = looks_tool_heavy(&request_text);
    let mut matches = Vec::new();

    for record in history.list_skill_variants(None, 256).await? {
        if matches!(record.status.as_str(), "archived" | "merged" | "draft") {
            continue;
        }

        let score = score_skill_variant(&record, &request_tokens, context_tags, tool_heavy_request);
        if score < 24 {
            continue;
        }

        let skill_path = skills_root.join(&record.relative_path);
        let content = std::fs::read_to_string(&skill_path).with_context(|| {
            format!(
                "failed to read skill preflight file {}",
                skill_path.display()
            )
        })?;
        matches.push(SkillPreflightMatch {
            reason: build_reason(&record, &request_tokens, context_tags, tool_heavy_request),
            excerpt: excerpt_skill(&content),
            record,
            score,
        });
    }

    matches.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| right.record.success_count.cmp(&left.record.success_count))
            .then_with(|| right.record.use_count.cmp(&left.record.use_count))
            .then_with(|| left.record.relative_path.cmp(&right.record.relative_path))
    });

    let mut selected = Vec::new();
    let mut seen_families = BTreeSet::new();
    for skill_match in matches {
        if seen_families.insert(skill_match.record.skill_name.clone()) {
            selected.push(skill_match);
        }
        if selected.len() >= MAX_SKILL_PREFLIGHT_MATCHES {
            break;
        }
    }

    Ok(selected)
}

async fn sync_skill_catalog(skills_root: &Path, history: &HistoryStore) -> Result<()> {
    let mut files = Vec::new();
    collect_skill_documents(skills_root, &mut files)?;
    for path in files {
        let _ = history.register_skill_document(&path).await;
    }
    Ok(())
}

fn collect_skill_documents(dir: &Path, out: &mut Vec<std::path::PathBuf>) -> Result<()> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };

    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_skill_documents(&path, out)?;
        } else if path.extension().and_then(|value| value.to_str()) == Some("md") {
            out.push(path);
        }
    }
    Ok(())
}

async fn resolve_skill_context_tags(
    session_manager: &Arc<SessionManager>,
    session_id: Option<SessionId>,
) -> Vec<String> {
    let root = if let Some(session_id) = session_id {
        let sessions = session_manager.list().await;
        sessions
            .iter()
            .find(|session| session.id == session_id)
            .and_then(|session| session.cwd.clone())
            .map(PathBuf::from)
    } else {
        None
    }
    .or_else(|| std::env::current_dir().ok());

    root.filter(|path| path.is_dir())
        .map(|path| super::semantic_env::infer_workspace_context_tags(&path))
        .unwrap_or_default()
}

fn should_run_skill_preflight(content: &str) -> bool {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.len() >= 48 || trimmed.lines().count() > 1 {
        return true;
    }

    let normalized = trimmed.to_ascii_lowercase();
    [
        "fix",
        "debug",
        "build",
        "implement",
        "refactor",
        "investigate",
        "review",
        "goal",
        "thread",
        "workspace",
        "terminal",
        "session",
        "tool",
    ]
    .iter()
    .any(|keyword| normalized.contains(keyword))
}

fn looks_tool_heavy(content: &str) -> bool {
    [
        "tool", "command", "terminal", "session", "browser", "mcp", "api",
    ]
    .iter()
    .any(|keyword| content.contains(keyword))
}

fn score_skill_variant(
    record: &SkillVariantRecord,
    request_tokens: &BTreeSet<String>,
    context_tags: &[String],
    tool_heavy_request: bool,
) -> i32 {
    let searchable = format!(
        "{} {} {} {}",
        record.skill_name,
        record.variant_name,
        record.relative_path,
        record.context_tags.join(" ")
    )
    .to_ascii_lowercase();
    let overlap = request_tokens
        .iter()
        .filter(|token| searchable.contains(token.as_str()))
        .count() as i32;
    let context_overlap = context_tags
        .iter()
        .filter(|tag| {
            record
                .context_tags
                .iter()
                .any(|record_tag| record_tag == *tag)
        })
        .count() as i32;
    let mut score = overlap * 28 + context_overlap * 10;
    score += status_bonus(&record.status);
    score += record.use_count.min(20) as i32;
    score += (record.success_rate() * 10.0).round() as i32;
    if record.is_canonical() {
        score += 2;
    }
    if tool_heavy_request && record.relative_path.contains("builtin/cheatsheet") {
        score += 20;
    }
    score
}

fn status_bonus(status: &str) -> i32 {
    match status {
        "promoted-to-canonical" => 18,
        "active" => 12,
        "deprecated" => 3,
        _ => 0,
    }
}

fn build_reason(
    record: &SkillVariantRecord,
    request_tokens: &BTreeSet<String>,
    context_tags: &[String],
    tool_heavy_request: bool,
) -> String {
    let searchable = format!(
        "{} {} {} {}",
        record.skill_name,
        record.variant_name,
        record.relative_path,
        record.context_tags.join(" ")
    )
    .to_ascii_lowercase();
    let matched_tokens = request_tokens
        .iter()
        .filter(|token| searchable.contains(token.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    let matched_context = context_tags
        .iter()
        .filter(|tag| {
            record
                .context_tags
                .iter()
                .any(|record_tag| record_tag == *tag)
        })
        .cloned()
        .collect::<Vec<_>>();

    let mut reasons = Vec::new();
    if !matched_tokens.is_empty() {
        reasons.push(format!("request matched {}", matched_tokens.join(", ")));
    }
    if !matched_context.is_empty() {
        reasons.push(format!(
            "workspace context matched {}",
            matched_context.join(", ")
        ));
    }
    if tool_heavy_request && record.relative_path.contains("builtin/cheatsheet") {
        reasons
            .push("request is tool-heavy, so the builtin tool cheatsheet is relevant".to_string());
    }
    if reasons.is_empty() {
        reasons.push("historical success and skill ranking made this a likely fit".to_string());
    }
    reasons.join("; ")
}

fn excerpt_skill(content: &str) -> String {
    let mut excerpt = content
        .lines()
        .take(MAX_SKILL_PREFLIGHT_LINES)
        .collect::<Vec<_>>()
        .join("\n");
    if excerpt.len() > MAX_SKILL_PREFLIGHT_CHARS {
        let new_len = excerpt
            .char_indices()
            .take_while(|(idx, ch)| *idx + ch.len_utf8() <= MAX_SKILL_PREFLIGHT_CHARS)
            .map(|(idx, ch)| idx + ch.len_utf8())
            .last()
            .unwrap_or(0);
        excerpt.truncate(new_len);
        excerpt.push_str("\n...");
    } else if content.lines().count() > MAX_SKILL_PREFLIGHT_LINES {
        excerpt.push_str("\n...");
    }
    excerpt
}

fn tokenize(content: &str) -> BTreeSet<String> {
    content
        .split(|character: char| {
            !character.is_ascii_alphanumeric() && character != '-' && character != '_'
        })
        .map(|token| token.trim_matches(|character: char| !character.is_ascii_alphanumeric()))
        .filter(|token| token.len() >= 3)
        .map(|token| token.to_ascii_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excerpt_skill_truncates_on_utf8_boundary() {
        let content = format!("{}\n{}", "a".repeat(MAX_SKILL_PREFLIGHT_CHARS - 2), "│tail");
        let excerpt = excerpt_skill(&content);
        let trimmed = excerpt.strip_suffix("\n...").unwrap_or(&excerpt);

        assert!(trimmed.is_char_boundary(trimmed.len()));
        assert!(trimmed.len() <= MAX_SKILL_PREFLIGHT_CHARS);
        assert!(!trimmed.ends_with('\u{FFFD}'));
    }
}
