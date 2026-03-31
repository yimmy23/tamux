use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

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

        let file_name = path.file_name().and_then(|value| value.to_str()).unwrap_or("");
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
    let normalized = skill.to_lowercase();

    for path in &files {
        let relative = path
            .strip_prefix(&root_canonical)
            .or_else(|_| path.strip_prefix(skills_root))
            .unwrap_or(path.as_path())
            .to_string_lossy()
            .replace('\\', "/");
        let stem = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_lowercase();
        if stem == normalized || relative.to_lowercase() == normalized {
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
        let stem = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_lowercase();
        if stem.contains(&normalized) || relative.to_lowercase().contains(&normalized) {
            return Ok(path.clone());
        }
    }

    anyhow::bail!(
        "skill '{}' was not found under {}",
        skill,
        skills_root.display()
    )
}
