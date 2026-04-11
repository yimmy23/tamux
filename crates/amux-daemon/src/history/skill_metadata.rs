use super::*;
use serde_yaml::Value;

#[derive(Debug)]
pub(crate) struct DerivedSkillMetadata {
    pub(crate) skill_name: String,
    pub(crate) variant_name: String,
    pub(crate) context_tags: Vec<String>,
}

#[derive(Debug, Clone)]
pub(super) struct BranchCandidate {
    pub(super) source_variant_id: String,
    pub(super) source_relative_path: String,
    pub(super) branch_tags: Vec<String>,
    pub(super) success_count: u32,
}

pub(crate) fn derive_skill_metadata(relative_path: &str, content: &str) -> DerivedSkillMetadata {
    let normalized_path = relative_path.replace('\\', "/");
    let path = Path::new(&normalized_path);
    let file_name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let file_name_is_skill = file_name.eq_ignore_ascii_case("skill.md");
    let base_skill_name = if file_name_is_skill {
        path.parent()
            .and_then(|parent| parent.file_name())
            .and_then(|value| value.to_str())
            .unwrap_or(stem)
            .to_string()
    } else {
        stem.to_string()
    };
    let mut skill_name = base_skill_name.clone();
    let mut variant_name = "canonical".to_string();

    if let Some((name, variant)) = base_skill_name.split_once("--") {
        if !name.trim().is_empty() && !variant.trim().is_empty() {
            skill_name = name.to_string();
            variant_name = normalize_skill_lookup(variant);
        }
    } else if !file_name_is_skill && normalized_path.contains("/generated/") {
        variant_name = "canonical".to_string();
    }

    if let Some(explicit_name) = extract_skill_frontmatter_name(content) {
        skill_name = explicit_name;
    }

    let skill_name = normalize_skill_lookup(&skill_name);
    let mut tags = BTreeSet::new();
    infer_skill_tags(&normalized_path, content, &mut tags);

    DerivedSkillMetadata {
        skill_name: if skill_name.is_empty() {
            "skill".to_string()
        } else {
            skill_name
        },
        variant_name,
        context_tags: tags.into_iter().collect(),
    }
}

fn extract_skill_frontmatter_name(content: &str) -> Option<String> {
    let rest = content.strip_prefix("---\n")?;
    let split_at = rest.find("\n---\n")?;
    let yaml = &rest[..split_at];
    let frontmatter = serde_yaml::from_str::<Value>(yaml).ok()?;
    frontmatter
        .as_mapping()
        .and_then(|mapping| mapping.get(Value::String("name".to_string())))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub(super) fn excerpt_on_char_boundary(input: &str, max_bytes: usize) -> &str {
    if input.len() <= max_bytes {
        return input;
    }

    let mut end = max_bytes;
    while end > 0 && !input.is_char_boundary(end) {
        end -= 1;
    }
    &input[..end]
}

pub(super) fn extract_markdown_title(content: &str) -> Option<String> {
    content
        .lines()
        .find_map(|line| line.trim().strip_prefix("# "))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub(super) fn normalize_skill_lookup(value: &str) -> String {
    value
        .trim()
        .trim_matches('/')
        .trim_end_matches(".md")
        .trim_end_matches("/skill")
        .trim_end_matches("/SKILL")
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else if matches!(ch, '/' | '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

pub(super) fn skill_variant_matches(record: &SkillVariantRecord, normalized: &str) -> bool {
    let relative = record.relative_path.to_ascii_lowercase();
    let skill_name = record.skill_name.to_ascii_lowercase();
    let variant_name = record.variant_name.to_ascii_lowercase();
    let stem = Path::new(&relative)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();

    skill_name == normalized
        || stem == normalized
        || relative == normalized
        || relative.ends_with(&format!("/{normalized}.md"))
        || format!("{skill_name}--{variant_name}") == normalized
        || relative.contains(normalized)
}

pub(super) fn compare_skill_variants(
    left: &SkillVariantRecord,
    right: &SkillVariantRecord,
    context_tags: &[String],
) -> std::cmp::Ordering {
    let left_overlap = skill_context_overlap(left, context_tags);
    let right_overlap = skill_context_overlap(right, context_tags);
    let left_status_rank = skill_status_rank(&left.status);
    let right_status_rank = skill_status_rank(&right.status);

    right_overlap
        .cmp(&left_overlap)
        .then_with(|| right_status_rank.cmp(&left_status_rank))
        .then_with(|| {
            right
                .success_rate()
                .partial_cmp(&left.success_rate())
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .then_with(|| right.use_count.cmp(&left.use_count))
        .then_with(|| right.is_canonical().cmp(&left.is_canonical()))
        .then_with(|| right.updated_at.cmp(&left.updated_at))
        .then_with(|| left.relative_path.cmp(&right.relative_path))
}

pub(super) fn describe_skill_variant_lifecycle(record: &SkillVariantRecord, now: u64) -> String {
    match record.status.as_str() {
        "merged" => {
            if record.parent_variant_id.is_some() {
                "merged back into its parent/canonical variant after proving stable enough to fold in"
                    .to_string()
            } else {
                "merged into another variant after proving stable enough to fold in".to_string()
            }
        }
        "promoted-to-canonical" => {
            "promoted because it outperformed the previous canonical path on real usage".to_string()
        }
        "deprecated" => "kept for reference after a stronger branch displaced it".to_string(),
        "archived" => {
            let idle_secs = record
                .last_used_at
                .map(|value| now.saturating_sub(value))
                .unwrap_or_else(|| now.saturating_sub(record.created_at));
            let stale = record.use_count >= SKILL_ARCHIVE_MIN_USES
                && idle_secs >= SKILL_ARCHIVE_MAX_IDLE_SECS;
            let low_value = record.use_count >= SKILL_ARCHIVE_MIN_USES
                && record.success_rate() < SKILL_ARCHIVE_SUCCESS_RATE_THRESHOLD;
            match (stale, low_value) {
                (true, true) => format!(
                    "archived because it went stale for {} day(s) and underperformed at {:.0}% success over {} uses",
                    idle_secs / 86_400,
                    record.success_rate() * 100.0,
                    record.use_count
                ),
                (true, false) => format!(
                    "archived because it went stale for {} day(s) without enough recent demand",
                    idle_secs / 86_400
                ),
                (false, true) => format!(
                    "archived because it underperformed at {:.0}% success over {} uses",
                    record.success_rate() * 100.0,
                    record.use_count
                ),
                (false, false) => "archived by lifecycle policy".to_string(),
            }
        }
        "active" if record.is_canonical() => {
            "active canonical fallback for this skill family".to_string()
        }
        "active" => "active branch variant still competing on context fit and historical success"
            .to_string(),
        other => format!("current lifecycle status is {other}"),
    }
}

pub(super) fn describe_skill_variant_selection(
    record: &SkillVariantRecord,
    context_tags: &[String],
) -> String {
    let matched_context = context_tags
        .iter()
        .filter(|tag| {
            record
                .context_tags
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(tag))
        })
        .cloned()
        .collect::<Vec<_>>();
    let context_reason = if matched_context.is_empty() {
        if record.is_canonical() {
            "no context tags matched, so canonical fallback weight dominated".to_string()
        } else {
            "no context tags matched, so ranking fell back to lifecycle status and history"
                .to_string()
        }
    } else {
        format!("matched context tags: {}", matched_context.join(", "))
    };
    format!(
        "{}; status={} with {:.0}% success across {} use(s)",
        context_reason,
        record.status,
        record.success_rate() * 100.0,
        record.use_count
    )
}

pub(super) fn skill_status_rank(status: &str) -> u8 {
    match status {
        "promoted-to-canonical" => 4,
        "active" => 3,
        "deprecated" => 2,
        "archived" => 1,
        "merged" => 0,
        _ => 0,
    }
}

pub(super) fn skill_variant_covers_branch_tags(
    variant: &SkillVariantRecord,
    branch_tags: &[String],
) -> bool {
    branch_tags.iter().all(|tag| {
        variant
            .context_tags
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(tag))
    })
}

pub(super) fn rebalance_skill_variant_status<'a>(
    variant: &'a SkillVariantRecord,
    promoted_variant_id: Option<&str>,
    now: u64,
) -> &'a str {
    if variant.status == "merged" {
        return "merged";
    }
    if !variant.is_canonical() {
        let idle_secs = variant
            .last_used_at
            .map(|value| now.saturating_sub(value))
            .unwrap_or_else(|| now.saturating_sub(variant.created_at));
        let is_stale =
            variant.use_count >= SKILL_ARCHIVE_MIN_USES && idle_secs >= SKILL_ARCHIVE_MAX_IDLE_SECS;
        let is_low_value = variant.use_count >= SKILL_ARCHIVE_MIN_USES
            && variant.success_rate() < SKILL_ARCHIVE_SUCCESS_RATE_THRESHOLD;
        if is_stale || is_low_value {
            return "archived";
        }
    }

    if Some(variant.variant_id.as_str()) == promoted_variant_id {
        "promoted-to-canonical"
    } else if variant.is_canonical() && promoted_variant_id.is_some() {
        "deprecated"
    } else {
        "active"
    }
}

pub(super) fn skill_context_overlap(record: &SkillVariantRecord, context_tags: &[String]) -> usize {
    if context_tags.is_empty() {
        return 0;
    }
    let tags = record
        .context_tags
        .iter()
        .map(|value| value.to_ascii_lowercase())
        .collect::<BTreeSet<_>>();
    context_tags
        .iter()
        .filter(|tag| tags.contains(&tag.to_ascii_lowercase()))
        .count()
}

pub(super) fn skill_content_similarity(left: &str, right: &str) -> f64 {
    let left_tokens = tokenize_skill_similarity(left);
    let right_tokens = tokenize_skill_similarity(right);
    if left_tokens.is_empty() || right_tokens.is_empty() {
        return 0.0;
    }
    let overlap = left_tokens.intersection(&right_tokens).count() as f64;
    let union = left_tokens.union(&right_tokens).count() as f64;
    if union == 0.0 {
        0.0
    } else {
        overlap / union
    }
}

pub(super) fn tokenize_skill_similarity(content: &str) -> BTreeSet<String> {
    content
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.starts_with("> Auto-branched from")
                && !trimmed.starts_with("## Learned Variant Contexts")
                && !trimmed.starts_with("- Merged ")
        })
        .flat_map(|line| {
            line.split(|ch: char| !ch.is_ascii_alphanumeric() && !matches!(ch, '-' | '_'))
                .map(str::trim)
                .filter(|token| token.len() >= 3)
                .map(|token| token.to_ascii_lowercase())
                .collect::<Vec<_>>()
        })
        .collect()
}

pub(super) fn skill_merge_note(variant: &SkillVariantRecord, similarity: f64) -> String {
    format!(
        "- Merged `{}` back into canonical after {} uses, {:.0}% success, {:.0}% content overlap. Use when context includes: {}.",
        variant.variant_name,
        variant.use_count,
        variant.success_rate() * 100.0,
        similarity * 100.0,
        if variant.context_tags.is_empty() {
            "the learned variant context".to_string()
        } else {
            variant.context_tags.join(", ")
        }
    )
}

pub(super) fn append_skill_merge_notes(canonical_content: &str, notes: &[String]) -> String {
    if notes.is_empty() {
        return canonical_content.to_string();
    }
    let mut next = canonical_content.trim_end().to_string();
    if !next.contains("## Learned Variant Contexts") {
        next.push_str("\n\n## Learned Variant Contexts\n");
    }
    for note in notes {
        if !next.contains(note) {
            next.push('\n');
            next.push_str(note);
        }
    }
    next.push('\n');
    next
}

pub(super) fn skill_merge_section(
    variant: &SkillVariantRecord,
    variant_content: &str,
    similarity: f64,
) -> String {
    let body = extract_mergeable_variant_body(variant_content);
    format!(
        "### Variant `{}`\n\nSuccess rate: {:.0}% across {} uses with {:.0}% overlap to canonical.\n\n{}\n",
        variant.variant_name,
        variant.success_rate() * 100.0,
        variant.use_count,
        similarity * 100.0,
        if body.is_empty() {
            format!(
                "Use when context includes: {}.",
                if variant.context_tags.is_empty() {
                    "the learned variant context".to_string()
                } else {
                    variant.context_tags.join(", ")
                }
            )
        } else {
            body
        }
    )
}

pub(super) fn extract_mergeable_variant_body(content: &str) -> String {
    content
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.starts_with("# ") && !trimmed.starts_with("> Auto-branched from")
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

pub(super) fn append_skill_merge_sections(canonical_content: &str, sections: &[String]) -> String {
    if sections.is_empty() {
        return canonical_content.to_string();
    }
    let mut next = canonical_content.trim_end().to_string();
    if !next.contains("## Merged Variant Playbooks") {
        next.push_str("\n\n## Merged Variant Playbooks\n");
    }
    for section in sections {
        let marker = section
            .lines()
            .next()
            .unwrap_or_default()
            .trim()
            .to_string();
        if !marker.is_empty() && next.contains(&marker) {
            continue;
        }
        next.push('\n');
        next.push_str(section.trim());
        next.push('\n');
    }
    next
}

pub(super) fn map_skill_variant_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<SkillVariantRecord> {
    let context_tags_json: String = row.get(6)?;
    let context_tags =
        serde_json::from_str::<Vec<String>>(&context_tags_json).unwrap_or_else(|_| Vec::new());
    Ok(SkillVariantRecord {
        variant_id: row.get(0)?,
        skill_name: row.get(1)?,
        variant_name: row.get(2)?,
        relative_path: row.get(3)?,
        parent_variant_id: row.get(4)?,
        version: row.get(5)?,
        context_tags,
        use_count: row.get::<_, i64>(7)? as u32,
        success_count: row.get::<_, i64>(8)? as u32,
        failure_count: row.get::<_, i64>(9)? as u32,
        status: row.get(10)?,
        last_used_at: row.get::<_, Option<i64>>(11)?.map(|value| value as u64),
        created_at: row.get::<_, i64>(12)? as u64,
        updated_at: row.get::<_, i64>(13)? as u64,
    })
}
