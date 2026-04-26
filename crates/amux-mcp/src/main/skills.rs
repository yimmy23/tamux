use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_yaml::Value;

pub(super) fn tamux_skills_dir() -> PathBuf {
    amux_protocol::tamux_skills_dir()
}

pub(super) fn tamux_guidelines_dir() -> PathBuf {
    amux_protocol::tamux_guidelines_dir()
}

pub(super) fn collect_skill_documents(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_skill_documents(&path, out)?;
            continue;
        }
        if !file_type.is_file() {
            continue;
        }

        let file_name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        let include = file_name.eq_ignore_ascii_case("SKILL.md")
            || (path
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("md"))
                && path
                    .components()
                    .any(|component| component.as_os_str() == "generated"));
        if include {
            out.push(path);
        }
    }

    Ok(())
}

pub(super) fn collect_guideline_documents(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_guideline_documents(&path, out)?;
            continue;
        }
        if !file_type.is_file() {
            continue;
        }

        let include = path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("md"));
        if include {
            out.push(path);
        }
    }

    Ok(())
}

pub(super) fn resolve_skill_path(skills_root: &Path, skill: &str) -> Result<PathBuf> {
    resolve_document_path(
        skills_root,
        skill,
        "skill",
        collect_skill_documents,
        "skill must not be empty",
    )
}

pub(super) fn resolve_guideline_path(guidelines_root: &Path, guideline: &str) -> Result<PathBuf> {
    resolve_document_path(
        guidelines_root,
        guideline,
        "guideline",
        collect_guideline_documents,
        "guideline must not be empty",
    )
}

fn resolve_document_path(
    documents_root: &Path,
    lookup: &str,
    kind: &str,
    collect_documents: fn(&Path, &mut Vec<PathBuf>) -> Result<()>,
    empty_message: &str,
) -> Result<PathBuf> {
    if lookup.trim().is_empty() {
        anyhow::bail!("{empty_message}");
    }

    let root_canonical =
        std::fs::canonicalize(documents_root).unwrap_or(documents_root.to_path_buf());
    let candidate = Path::new(lookup);
    if candidate.components().count() > 1 || candidate.is_absolute() {
        let full = if candidate.is_absolute() {
            candidate.to_path_buf()
        } else {
            documents_root.join(candidate)
        };
        let canonical = std::fs::canonicalize(&full)
            .with_context(|| format!("{kind} '{}' was not found", lookup))?;
        if !canonical.starts_with(&root_canonical) {
            anyhow::bail!("{kind} path must stay inside {}", documents_root.display());
        }
        return Ok(canonical);
    }

    let mut files = Vec::new();
    collect_documents(documents_root, &mut files)?;
    files.sort();
    let normalized = normalize_skill_lookup(lookup);

    for path in &files {
        let relative = path
            .strip_prefix(&root_canonical)
            .or_else(|_| path.strip_prefix(documents_root))
            .unwrap_or(path.as_path())
            .to_string_lossy()
            .replace('\\', "/");
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let keys = skill_lookup_keys(path, &relative, &content);
        if keys.iter().any(|key| key == &normalized) {
            return Ok(path.clone());
        }
    }

    for path in &files {
        let relative = path
            .strip_prefix(&root_canonical)
            .or_else(|_| path.strip_prefix(documents_root))
            .unwrap_or(path.as_path())
            .to_string_lossy()
            .replace('\\', "/");
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let keys = skill_lookup_keys(path, &relative, &content);
        if keys.iter().any(|key| key.contains(&normalized)) {
            return Ok(path.clone());
        }
    }

    anyhow::bail!(
        "{kind} '{}' was not found under {}",
        lookup,
        documents_root.display()
    )
}

fn skill_lookup_keys(path: &Path, relative: &str, content: &str) -> Vec<String> {
    let mut keys = Vec::new();
    let normalized_relative = normalize_skill_lookup(relative);
    if !normalized_relative.is_empty() {
        keys.push(normalized_relative);
    }

    let file_name_is_skill = path
        .file_name()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("skill.md"));
    let base_name = if file_name_is_skill {
        path.parent()
            .and_then(|parent| parent.file_name())
            .and_then(|value| value.to_str())
            .unwrap_or_default()
    } else {
        path.file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or_default()
    };
    let normalized_base_name = normalize_skill_lookup(base_name);
    if !normalized_base_name.is_empty() {
        keys.push(normalized_base_name);
    }

    if let Some(explicit_name) = extract_skill_frontmatter_name(content) {
        let normalized_name = normalize_skill_lookup(&explicit_name);
        if !normalized_name.is_empty() {
            keys.push(normalized_name);
        }
    }

    keys.sort();
    keys.dedup();
    keys
}

fn extract_skill_frontmatter_name(content: &str) -> Option<String> {
    extract_frontmatter_string(content, "name")
}

fn extract_frontmatter(content: &str) -> Option<Value> {
    let rest = content.strip_prefix("---\n")?;
    let split_at = rest.find("\n---\n")?;
    let yaml = &rest[..split_at];
    serde_yaml::from_str::<Value>(yaml).ok()
}

fn extract_frontmatter_string(content: &str, key: &str) -> Option<String> {
    let frontmatter = extract_frontmatter(content)?;
    frontmatter
        .as_mapping()
        .and_then(|mapping| mapping.get(Value::String(key.to_string())))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn normalize_skill_lookup(value: &str) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_skill_path_matches_frontmatter_name_for_nested_skill() {
        let root = std::env::temp_dir().join(format!(
            "tamux-mcp-skills-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ));
        let skills_root = root.join("skills");
        let skill_path = skills_root
            .join("development")
            .join("superpowers")
            .join("alias-dir")
            .join("SKILL.md");
        std::fs::create_dir_all(skill_path.parent().expect("skill directory"))
            .expect("create skill directory");
        std::fs::write(
            &skill_path,
            "---\nname: subagent-driven-development\ndescription: Execute implementation work through subagents.\n---\n# Subagent-Driven Development\n",
        )
        .expect("write skill");

        let resolved = resolve_skill_path(&skills_root, "subagent-driven-development")
            .expect("skill should resolve by frontmatter name");

        assert_eq!(resolved, skill_path);

        std::fs::remove_dir_all(root).expect("remove temp directory");
    }

    #[test]
    fn resolve_guideline_path_matches_frontmatter_name() {
        let root = std::env::temp_dir().join(format!(
            "tamux-mcp-guidelines-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time")
                .as_nanos()
        ));
        let guidelines_root = root.join("guidelines");
        let guideline_path = guidelines_root.join("coding-task.md");
        std::fs::create_dir_all(&guidelines_root).expect("create guideline directory");
        std::fs::write(
            &guideline_path,
            "---\nname: coding-task\ndescription: Use before implementing code.\nrecommended_skills:\n  - test-driven-development\n---\n# Coding Task\n",
        )
        .expect("write guideline");

        let resolved = resolve_guideline_path(&guidelines_root, "coding-task")
            .expect("guideline should resolve by frontmatter name");

        assert_eq!(resolved, guideline_path);

        std::fs::remove_dir_all(root).expect("remove temp directory");
    }
}
