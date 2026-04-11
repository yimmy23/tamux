use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde_yaml::Value;

pub(super) fn tamux_root_dir() -> PathBuf {
    if cfg!(windows) {
        std::env::var("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                std::env::var("USERPROFILE")
                    .map(PathBuf::from)
                    .unwrap_or_default()
                    .join("AppData")
                    .join("Local")
            })
            .join("tamux")
    } else {
        std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_default()
            .join(".tamux")
    }
}

pub(super) fn tamux_skills_dir() -> PathBuf {
    tamux_root_dir().join("skills")
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

pub(super) fn resolve_skill_path(skills_root: &Path, skill: &str) -> Result<PathBuf> {
    if skill.trim().is_empty() {
        anyhow::bail!("skill must not be empty");
    }

    let root_canonical = std::fs::canonicalize(skills_root).unwrap_or(skills_root.to_path_buf());
    let candidate = Path::new(skill);
    if candidate.components().count() > 1 || candidate.is_absolute() {
        let full = if candidate.is_absolute() {
            candidate.to_path_buf()
        } else {
            skills_root.join(candidate)
        };
        let canonical = std::fs::canonicalize(&full)
            .with_context(|| format!("skill '{}' was not found", skill))?;
        if !canonical.starts_with(&root_canonical) {
            anyhow::bail!("skill path must stay inside {}", skills_root.display());
        }
        return Ok(canonical);
    }

    let mut files = Vec::new();
    collect_skill_documents(skills_root, &mut files)?;
    files.sort();
    let normalized = normalize_skill_lookup(skill);

    for path in &files {
        let relative = path
            .strip_prefix(&root_canonical)
            .or_else(|_| path.strip_prefix(skills_root))
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
            .or_else(|_| path.strip_prefix(skills_root))
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
        "skill '{}' was not found under {}",
        skill,
        skills_root.display()
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
}
