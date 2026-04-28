use super::*;
use crate::agent::skills_dir;

pub(super) async fn render_conventions(
    root: &Path,
    graph: &SemanticGraph,
    history: &HistoryStore,
    agent_data_dir: &Path,
    target: Option<&str>,
    limit: usize,
) -> Result<String> {
    let target_tokens = target.map(tokenize_convention_query).unwrap_or_default();
    let report = history
        .memory_provenance_report(None, limit.saturating_mul(4).max(20))
        .await?;
    let mut matching_entries = report
        .entries
        .into_iter()
        .filter(|entry| entry.mode != "remove")
        .filter(|entry| entry.status != "retracted")
        .filter(|entry| convention_entry_matches(entry, &target_tokens))
        .collect::<Vec<_>>();
    matching_entries.sort_by(|left, right| {
        right
            .confidence
            .partial_cmp(&left.confidence)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| right.created_at.cmp(&left.created_at))
    });

    let skill_matches = collect_matching_skills(&skills_dir(agent_data_dir), target, limit);
    let package_match = target
        .and_then(|value| resolve_target_package(graph, Some(value)).ok())
        .map(|package| {
            let dependent_count = graph
                .packages
                .iter()
                .filter(|candidate| candidate.dependencies.iter().any(|dep| dep == &package.name))
                .count();
            format!(
                "- Package context: [{}] {} has {} direct dependency(ies) and {} local dependent(s).",
                package.ecosystem,
                package.name,
                package.dependencies.len(),
                dependent_count
            )
        });

    if matching_entries.is_empty() && skill_matches.is_empty() && package_match.is_none() {
        return Ok(match target {
            Some(target) => format!(
                "No durable conventions matched `{target}` under {}.",
                root.display()
            ),
            None => format!("No durable conventions found for {}.", root.display()),
        });
    }

    let mut lines = vec![match target {
        Some(target) => format!("Conventions related to `{target}`:"),
        None => format!("Conventions for {}:", root.display()),
    }];

    for entry in matching_entries.into_iter().take(limit) {
        lines.push(format!(
            "- [{} | {} | {:.0}% confidence | {:.1}d old] {}",
            entry.target,
            entry.source_kind,
            entry.confidence * 100.0,
            entry.age_days,
            summarize_text(&entry.content, 140)
        ));
    }

    if let Some(package_line) = package_match {
        lines.push(package_line);
    }

    if !skill_matches.is_empty() {
        lines.push("Relevant local skills/workflows:".to_string());
        for skill in skill_matches {
            lines.push(format!("- {}", skill));
        }
    }

    Ok(lines.join("\n"))
}

pub(super) async fn render_temporal(
    root: &Path,
    history: &HistoryStore,
    target: Option<&str>,
    limit: usize,
) -> Result<String> {
    let normalized_target = target.map(normalize_convention_text);
    let commands = history
        .query_command_log(None, None, Some(limit.saturating_mul(12).max(40)))
        .await?
        .into_iter()
        .filter(|entry| {
            entry
                .cwd
                .as_deref()
                .map(|cwd| path_is_under_root(root, cwd))
                .unwrap_or(false)
                || entry
                    .path
                    .as_deref()
                    .map(|path| path_is_under_root(root, path))
                    .unwrap_or(false)
        })
        .filter(|entry| {
            normalized_target.as_ref().is_none_or(|target| {
                target_matches_text(&entry.command, target)
                    || entry
                        .path
                        .as_deref()
                        .is_some_and(|path| target_matches_text(path, target))
                    || entry
                        .cwd
                        .as_deref()
                        .is_some_and(|cwd| target_matches_text(cwd, target))
            })
        })
        .collect::<Vec<_>>();

    let transcripts = if normalized_target.is_some() {
        history
            .list_transcript_index(None)
            .await?
            .into_iter()
            .filter(|entry| {
                normalized_target.as_ref().is_some_and(|target| {
                    target_matches_text(&entry.filename, target)
                        || entry
                            .preview
                            .as_deref()
                            .is_some_and(|preview| target_matches_text(preview, target))
                })
            })
            .take(limit)
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    let memory_matches = if normalized_target.is_some() {
        history
            .memory_provenance_report(target, limit.saturating_mul(2).max(12))
            .await?
            .entries
            .into_iter()
            .filter(|entry| entry.mode != "remove" && entry.status != "retracted")
            .take(limit)
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    if commands.is_empty() && transcripts.is_empty() && memory_matches.is_empty() {
        return Ok(match target {
            Some(target) => format!(
                "No temporal workspace history matched `{target}` under {}.",
                root.display()
            ),
            None => format!(
                "No temporal workspace history found under {}.",
                root.display()
            ),
        });
    }

    let success_count = commands
        .iter()
        .filter(|entry| entry.exit_code.unwrap_or_default() == 0)
        .count();
    let failure_count = commands
        .iter()
        .filter(|entry| entry.exit_code.is_some_and(|code| code != 0))
        .count();

    let mut lines = vec![match target {
        Some(target) => format!("Temporal history related to `{target}`:"),
        None => format!("Temporal history for {}:", root.display()),
    }];
    if !commands.is_empty() {
        lines.push(format!(
            "- Recent matching commands: {} total, {} success, {} failure.",
            commands.len(),
            success_count,
            failure_count
        ));
        for entry in commands.iter().take(limit) {
            lines.push(format!(
                "- [cmd @ {}] {}{}",
                entry.timestamp,
                summarize_text(&entry.command, 100),
                match entry.exit_code {
                    Some(code) => format!(" (exit {code})"),
                    None => String::new(),
                }
            ));
        }
    }
    if !transcripts.is_empty() {
        lines.push("Global matching transcript/session captures:".to_string());
        for entry in transcripts {
            lines.push(format!(
                "- [capture @ {}] {}{}",
                entry.captured_at,
                entry.filename,
                entry
                    .preview
                    .as_deref()
                    .map(|preview| format!(" — {}", summarize_text(preview, 80)))
                    .unwrap_or_default()
            ));
        }
    }
    if !memory_matches.is_empty() {
        lines.push("Global durable learned history matching the target:".to_string());
        for entry in memory_matches {
            lines.push(format!(
                "- [{} | {:.0}% confidence | {:.1}d old] {}",
                entry.source_kind,
                entry.confidence * 100.0,
                entry.age_days,
                summarize_text(&entry.content, 100)
            ));
        }
    }

    Ok(lines.join("\n"))
}
