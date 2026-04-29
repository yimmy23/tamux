use anyhow::{Context, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};
use zorai_protocol::{SessionId, SessionInfo};

use crate::cli::GuidelineAction;
use crate::client;

#[cfg(test)]
use super::guideline_sync::RemoteGuidelineDocument;
use super::guideline_sync::{fetch_remote_guideline_documents, sync_guideline_documents};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct GuidelineEntry {
    name: String,
    relative_path: String,
    description: Option<String>,
}

pub(crate) async fn run(action: GuidelineAction) -> Result<()> {
    let root = zorai_protocol::zorai_guidelines_dir();
    match action {
        GuidelineAction::Discover {
            query,
            session,
            limit,
            cursor,
        } => {
            let session_id = resolve_guideline_discovery_session(session.as_deref()).await?;
            let result = client::send_guideline_discover(&query, session_id, limit, cursor).await?;
            println!("{}", render_guideline_discovery(&result));
        }
        GuidelineAction::Inspect { name } => match read_guideline_file(&root, &name)? {
            Some((entry, content)) => {
                println!("Guideline:   {}", entry.name);
                println!("Path:        {}", entry.relative_path);
                if let Some(description) = entry.description {
                    println!("Description: {}", description);
                }
                println!("\n--- GUIDELINE.md ---\n{}", content);
            }
            None => eprintln!("Guideline not found: {}", name),
        },
        GuidelineAction::Install {
            source,
            name,
            force,
        } => {
            let installed = install_guideline_command(&source, name.as_deref(), force)?;
            println!("Installed guideline: {}", installed.display());
            println!("Guidelines root: {}", root.display());
        }
        GuidelineAction::Sync { force } => {
            let documents = fetch_remote_guideline_documents().await?;
            let summary = sync_guideline_documents(&root, &documents, force)?;
            println!(
                "Synced guidelines from https://github.com/mkurman/zorai/tree/main/guidelines"
            );
            println!("Guidelines root: {}", root.display());
            println!(
                "Installed: {} | Overwritten: {} | Skipped existing: {}",
                summary.installed, summary.overwritten, summary.skipped_existing
            );
        }
        GuidelineAction::List { json } => {
            let entries = list_guideline_files(&root)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&entries)?);
            } else if entries.is_empty() {
                println!("No guidelines found under {}.", root.display());
            } else {
                println!("Guidelines under {}:", root.display());
                for entry in entries {
                    match entry.description.as_deref() {
                        Some(description) => {
                            println!(
                                "- {} ({}) - {}",
                                entry.name, entry.relative_path, description
                            )
                        }
                        None => println!("- {} ({})", entry.name, entry.relative_path),
                    }
                }
            }
        }
    }
    Ok(())
}

fn render_guideline_discovery(result: &zorai_protocol::SkillDiscoveryResultPublic) -> String {
    let mut lines = vec![
        format!("Confidence: {}", display_or_none(&result.confidence_tier)),
        format!(
            "Normalized intent: {}",
            display_or_none(&result.normalized_intent)
        ),
        format!(
            "Next action: {}",
            display_or_none(&result.recommended_action)
        ),
        format!("Mesh state: {}", display_or_none(&result.mesh_state)),
    ];

    if !result.rationale.is_empty() {
        lines.push(format!("Rationale: {}", result.rationale.join(", ")));
    }
    if !result.capability_family.is_empty() {
        lines.push(format!(
            "Capability family: {}",
            result.capability_family.join(" / ")
        ));
    }

    if result.candidates.is_empty() {
        lines.push("No matching guidelines found.".to_string());
        return lines.join("\n");
    }

    for (index, candidate) in result.candidates.iter().enumerate() {
        lines.push(format!(
            "{}. {} [{}] score={}",
            index + 1,
            candidate.skill_name,
            candidate.status,
            (candidate.score * 100.0).round() as u32
        ));
        let reasons = if candidate.reasons.is_empty() {
            "none".to_string()
        } else {
            candidate.reasons.join(", ")
        };
        lines.push(format!("   reasons: {reasons}"));
        if !candidate.matched_intents.is_empty() {
            lines.push(format!(
                "   matched intents: {}",
                candidate.matched_intents.join(", ")
            ));
        }
        lines.push(format!(
            "   trust/risk: {} / {}",
            display_or_none(&candidate.trust_tier),
            display_or_none(&candidate.risk_level)
        ));
    }

    if let Some(next_cursor) = result.next_cursor.as_deref() {
        lines.push(format!("Next cursor: {next_cursor}"));
    }

    lines.join("\n")
}

fn display_or_none(value: &str) -> &str {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        "none"
    } else {
        trimmed
    }
}

pub(super) fn install_guideline_command(
    source: &str,
    name: Option<&str>,
    force: bool,
) -> Result<PathBuf> {
    install_guideline_file(
        Path::new(source),
        &zorai_protocol::zorai_guidelines_dir(),
        name,
        force,
    )
}

fn install_guideline_file(
    source: &Path,
    guidelines_root: &Path,
    name: Option<&str>,
    force: bool,
) -> Result<PathBuf> {
    let source = std::fs::canonicalize(source)
        .with_context(|| format!("guideline source was not found: {}", source.display()))?;
    if !source.is_file() {
        anyhow::bail!("guideline source must be a file: {}", source.display());
    }
    if !source
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("md"))
    {
        anyhow::bail!("guideline source must be a markdown .md file");
    }

    let filename = match name {
        Some(value) => validate_destination_name(value)?,
        None => source
            .file_name()
            .and_then(|value| value.to_str())
            .map(ToOwned::to_owned)
            .ok_or_else(|| anyhow::anyhow!("guideline source has no filename"))?,
    };
    if !filename.to_ascii_lowercase().ends_with(".md") {
        anyhow::bail!("guideline destination name must end with .md");
    }

    std::fs::create_dir_all(guidelines_root).with_context(|| {
        format!(
            "failed to create guidelines directory {}",
            guidelines_root.display()
        )
    })?;
    let destination = guidelines_root.join(filename);
    if destination.exists() && !force {
        anyhow::bail!(
            "guideline already exists: {} (use --force to overwrite)",
            destination.display()
        );
    }
    std::fs::copy(&source, &destination).with_context(|| {
        format!(
            "failed to copy guideline {} to {}",
            source.display(),
            destination.display()
        )
    })?;
    Ok(destination)
}

fn validate_destination_name(value: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        anyhow::bail!("guideline destination name must not be empty");
    }
    let path = Path::new(trimmed);
    if path.is_absolute() || path.components().count() != 1 {
        anyhow::bail!("guideline destination name must be a filename, not a path");
    }
    Ok(trimmed.to_string())
}

fn list_guideline_files(guidelines_root: &Path) -> Result<Vec<GuidelineEntry>> {
    let mut files = Vec::new();
    collect_markdown_files(guidelines_root, &mut files)?;
    files.sort();

    let mut entries = Vec::new();
    for path in files {
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read guideline {}", path.display()))?;
        let relative_path = path
            .strip_prefix(guidelines_root)
            .unwrap_or(path.as_path())
            .to_string_lossy()
            .replace('\\', "/");
        let name = frontmatter_value(&content, "name").unwrap_or_else(|| {
            path.file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("guideline")
                .to_string()
        });
        let description = frontmatter_value(&content, "description");
        entries.push(GuidelineEntry {
            name,
            relative_path,
            description,
        });
    }
    Ok(entries)
}

fn read_guideline_file(
    guidelines_root: &Path,
    identifier: &str,
) -> Result<Option<(GuidelineEntry, String)>> {
    let identifier = identifier.trim();
    if identifier.is_empty() {
        return Ok(None);
    }

    let mut files = Vec::new();
    collect_markdown_files(guidelines_root, &mut files)?;
    files.sort();

    for path in files {
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read guideline {}", path.display()))?;
        let relative_path = path
            .strip_prefix(guidelines_root)
            .unwrap_or(path.as_path())
            .to_string_lossy()
            .replace('\\', "/");
        let file_stem = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        let name = frontmatter_value(&content, "name").unwrap_or_else(|| file_stem.to_string());
        let relative_without_extension =
            relative_path.strip_suffix(".md").unwrap_or(&relative_path);
        let matches = identifier == name
            || identifier == relative_path
            || identifier == relative_without_extension
            || identifier == file_stem;
        if matches {
            let description = frontmatter_value(&content, "description");
            return Ok(Some((
                GuidelineEntry {
                    name,
                    relative_path,
                    description,
                },
                content,
            )));
        }
    }

    Ok(None)
}

fn collect_markdown_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_markdown_files(&path, out)?;
        } else if file_type.is_file()
            && path
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("md"))
        {
            out.push(path);
        }
    }
    Ok(())
}

fn frontmatter_value(content: &str, key: &str) -> Option<String> {
    let rest = content.strip_prefix("---\n")?;
    let frontmatter = rest.split_once("\n---\n")?.0;
    for line in frontmatter.lines() {
        let Some((line_key, line_value)) = line.split_once(':') else {
            continue;
        };
        if line_key.trim() == key {
            let value = line_value.trim().trim_matches('"').trim_matches('\'');
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn parse_guideline_discovery_session(value: Option<&str>) -> Result<Option<SessionId>> {
    value
        .map(|session| {
            session
                .parse()
                .with_context(|| format!("invalid session ID `{session}`"))
        })
        .transpose()
}

fn infer_guideline_discovery_session_for_cwd(
    sessions: &[SessionInfo],
    cwd: &Path,
) -> Option<SessionId> {
    sessions
        .iter()
        .filter(|session| session.is_alive)
        .filter_map(|session| {
            let session_cwd = session.cwd.as_deref()?;
            let session_path = Path::new(session_cwd);
            cwd.starts_with(session_path)
                .then_some((session_path.components().count(), session.id))
        })
        .max_by_key(|(depth, _)| *depth)
        .map(|(_, session_id)| session_id)
}

async fn resolve_guideline_discovery_session(value: Option<&str>) -> Result<Option<SessionId>> {
    if let Some(session_id) = parse_guideline_discovery_session(value)? {
        return Ok(Some(session_id));
    }

    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(_) => return Ok(None),
    };
    let sessions = client::list_sessions().await?;
    Ok(infer_guideline_discovery_session_for_cwd(&sessions, &cwd))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn install_guideline_copies_markdown_without_overwriting() {
        let temp = tempfile::tempdir().expect("tempdir");
        let source = temp.path().join("coding-task.md");
        let root = temp.path().join("guidelines");
        std::fs::write(&source, "# Coding Task\n").expect("write source");

        let installed = install_guideline_file(&source, &root, None, false).expect("install");
        assert_eq!(installed, root.join("coding-task.md"));
        assert_eq!(
            std::fs::read_to_string(&installed).expect("read installed"),
            "# Coding Task\n"
        );

        let error =
            install_guideline_file(&source, &root, None, false).expect_err("overwrite blocked");
        assert!(error.to_string().contains("already exists"));
    }

    #[test]
    fn list_guidelines_reads_from_guidelines_root() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("guidelines");
        std::fs::create_dir_all(&root).expect("create root");
        std::fs::write(
            root.join("coding-task.md"),
            "---\nname: coding-task\ndescription: Coding work\n---\n# Coding Task\n",
        )
        .expect("write guideline");

        let entries = list_guideline_files(&root).expect("list guidelines");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "coding-task");
        assert_eq!(entries[0].relative_path, "coding-task.md");
        assert_eq!(entries[0].description.as_deref(), Some("Coding work"));
    }

    #[test]
    fn read_guideline_matches_name_stem_and_relative_path() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("guidelines");
        let nested = root.join("research");
        std::fs::create_dir_all(&nested).expect("create nested guidelines dir");
        std::fs::write(
            nested.join("lit-review.md"),
            "---\nname: academic-literature-review-task\ndescription: Research synthesis\n---\n# Literature Review\n",
        )
        .expect("write guideline");

        let by_name = read_guideline_file(&root, "academic-literature-review-task")
            .expect("read by name")
            .expect("guideline by name");
        assert_eq!(by_name.0.name, "academic-literature-review-task");
        assert_eq!(by_name.0.relative_path, "research/lit-review.md");
        assert!(by_name.1.contains("# Literature Review"));

        let by_stem = read_guideline_file(&root, "lit-review")
            .expect("read by stem")
            .expect("guideline by stem");
        assert_eq!(by_stem.0.name, "academic-literature-review-task");

        let by_relative = read_guideline_file(&root, "research/lit-review")
            .expect("read by relative path")
            .expect("guideline by relative path");
        assert_eq!(
            by_relative.0.description.as_deref(),
            Some("Research synthesis")
        );
    }

    #[test]
    fn sync_guidelines_skips_existing_files_without_force() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("guidelines");
        std::fs::create_dir_all(&root).expect("create root");
        std::fs::write(root.join("coding-task.md"), "# Local Coding\n").expect("write local");

        let documents = vec![
            RemoteGuidelineDocument {
                relative_path: "coding-task.md".to_string(),
                content: "# Upstream Coding\n".to_string(),
            },
            RemoteGuidelineDocument {
                relative_path: "research/research-task.md".to_string(),
                content: "# Upstream Research\n".to_string(),
            },
        ];

        let summary = sync_guideline_documents(&root, &documents, false).expect("sync guidelines");

        assert_eq!(summary.installed, 1);
        assert_eq!(summary.skipped_existing, 1);
        assert_eq!(summary.overwritten, 0);
        assert_eq!(
            std::fs::read_to_string(root.join("coding-task.md")).expect("read coding"),
            "# Local Coding\n"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("research/research-task.md")).expect("read research"),
            "# Upstream Research\n"
        );
    }

    #[test]
    fn sync_guidelines_force_overwrites_existing_files() {
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("guidelines");
        std::fs::create_dir_all(&root).expect("create root");
        std::fs::write(root.join("coding-task.md"), "# Local Coding\n").expect("write local");

        let documents = vec![RemoteGuidelineDocument {
            relative_path: "coding-task.md".to_string(),
            content: "# Upstream Coding\n".to_string(),
        }];

        let summary = sync_guideline_documents(&root, &documents, true).expect("sync guidelines");

        assert_eq!(summary.installed, 0);
        assert_eq!(summary.skipped_existing, 0);
        assert_eq!(summary.overwritten, 1);
        assert_eq!(
            std::fs::read_to_string(root.join("coding-task.md")).expect("read coding"),
            "# Upstream Coding\n"
        );
    }
}
