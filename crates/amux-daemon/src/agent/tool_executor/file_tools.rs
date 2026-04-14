async fn execute_list_files(
    args: &serde_json::Value,
    _session_manager: &Arc<SessionManager>,
    _preferred_session_id: Option<SessionId>,
) -> Result<String> {
    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
    validate_read_path(path)?;
    let mut rows = Vec::new();
    let mut read_dir = tokio::fs::read_dir(path).await?;
    while let Some(entry) = read_dir.next_entry().await? {
        let metadata = entry.metadata().await?;
        let kind = if metadata.is_dir() { "dir" } else { "file" };
        let size = metadata.len();
        let name = entry.file_name().to_string_lossy().to_string();
        rows.push(format!("{kind}\t{size}\t{name}"));
    }

    rows.sort();
    if rows.is_empty() {
        Ok("(empty directory)".to_string())
    } else {
        Ok(rows.join("\n"))
    }
}

async fn execute_read_file(args: &serde_json::Value) -> Result<String> {
    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'path' argument"))?;

    let offset = args.get("offset").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
    let limit_was_explicit = args.get("limit").is_some() || args.get("max_lines").is_some();
    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .or_else(|| args.get("max_lines").and_then(|v| v.as_u64()))
        .unwrap_or(250) as usize;

    let content = tokio::fs::read_to_string(path).await?;
    let total_lines = content.lines().count();
    let lines: Vec<&str> = content.lines().skip(offset).take(limit).collect();

    let mut result = lines.join("\n");
    let shown = lines.len();
    let remaining_after_window = total_lines.saturating_sub(offset + shown);
    if !limit_was_explicit && remaining_after_window > 0 {
        result.push_str(&format!(
            "\n\n... (truncated, showing {shown} of {total_lines} lines starting at line {offset})"
        ));
    }

    Ok(result)
}

async fn execute_get_git_line_statuses(args: &serde_json::Value) -> Result<String> {
    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'path' argument"))?;
    validate_read_path(path)?;

    if args
        .get("start_line")
        .is_some_and(|value| value.as_u64().is_none())
    {
        anyhow::bail!("'start_line' must be a positive integer");
    }
    if args
        .get("limit")
        .is_some_and(|value| value.as_u64().is_none())
    {
        anyhow::bail!("'limit' must be a positive integer");
    }

    let start_line = args
        .get("start_line")
        .and_then(|v| v.as_u64())
        .unwrap_or(1)
        .max(1) as usize;
    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(250)
        .clamp(1, 500) as usize;

    let report = crate::git::get_git_line_statuses(path, start_line, limit)?;
    serde_json::to_string_pretty(&report).map_err(Into::into)
}

async fn execute_create_file(args: &serde_json::Value) -> Result<String> {
    let raw_path = get_file_path_arg(args)
        .ok_or_else(|| anyhow::anyhow!("missing 'path' or 'filename' argument"))?;
    validate_write_path(raw_path)?;
    let content = get_file_content_arg(args)?;
    let overwrite = args
        .get("overwrite")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let cwd = get_explicit_cwd_arg(args);

    let target = resolve_tool_path(raw_path, cwd.map(Path::new));
    if target.exists() && !overwrite {
        anyhow::bail!("file already exists: {}", target.display());
    }
    write_text_file_atomically(&target, &content, overwrite).await?;
    Ok(format!(
        "Created file {} ({} bytes)",
        resolve_tool_path(raw_path, cwd.map(Path::new)).display(),
        content.len()
    ))
}

async fn execute_append_to_file(args: &serde_json::Value) -> Result<String> {
    let path = get_file_path_arg(args).ok_or_else(|| anyhow::anyhow!("missing 'path' argument"))?;
    validate_write_path(path)?;
    let content = get_file_content_arg(args)?;
    let create_if_missing = args
        .get("create_if_missing")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let target = std::path::Path::new(path);
    if !target.exists() && !create_if_missing {
        anyhow::bail!("file does not exist: {path}");
    }
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let mut existing = if target.exists() {
        tokio::fs::read_to_string(target).await?
    } else {
        String::new()
    };
    existing.push_str(&content);
    write_text_file_atomically(target, &existing, true).await?;
    Ok(format!("Appended {} bytes to {path}", content.len()))
}

async fn execute_replace_in_file(args: &serde_json::Value) -> Result<String> {
    let path = get_file_path_arg(args).ok_or_else(|| anyhow::anyhow!("missing 'path' argument"))?;
    validate_write_path(path)?;
    let old_text = get_string_arg(args, &["old_text", "search", "find"])
        .ok_or_else(|| anyhow::anyhow!("missing 'old_text' argument"))?;
    let new_text = get_string_arg(args, &["new_text", "replace", "replacement"])
        .ok_or_else(|| anyhow::anyhow!("missing 'new_text' argument"))?;
    let replace_all = args
        .get("replace_all")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    apply_exact_replacements(
        path,
        vec![(old_text.to_string(), new_text.to_string(), replace_all)],
    )
    .await
}

async fn execute_apply_file_patch(args: &serde_json::Value) -> Result<String> {
    let path = get_file_path_arg(args).ok_or_else(|| anyhow::anyhow!("missing 'path' argument"))?;
    validate_write_path(path)?;
    let edits = args
        .get("edits")
        .or_else(|| args.get("patches"))
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("missing 'edits' argument"))?;
    if edits.is_empty() {
        anyhow::bail!("'edits' must contain at least one edit");
    }

    let replacements = edits
        .iter()
        .enumerate()
        .map(|(index, edit)| {
            let old_text = edit
                .get("old_text")
                .or_else(|| edit.get("search"))
                .or_else(|| edit.get("find"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("edit {} is missing 'old_text'", index + 1))?;
            let new_text = edit
                .get("new_text")
                .or_else(|| edit.get("replace"))
                .or_else(|| edit.get("replacement"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("edit {} is missing 'new_text'", index + 1))?;
            let replace_all = edit
                .get("replace_all")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            Ok((old_text.to_string(), new_text.to_string(), replace_all))
        })
        .collect::<Result<Vec<_>>>()?;

    apply_exact_replacements(path, replacements).await
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum HarnessPatchAction {
    Add {
        path: String,
        content: String,
    },
    Update {
        path: String,
        old_text: String,
        new_text: String,
    },
    Delete {
        path: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StagedHarnessPatchFile {
    path: String,
    original_content: Option<String>,
    desired_content: Option<String>,
}

pub(crate) fn extract_apply_patch_paths(input: &str) -> Result<Vec<String>> {
    let mut paths = Vec::new();
    for action in parse_harness_patch_actions(input)? {
        let path = match action {
            HarnessPatchAction::Add { path, .. }
            | HarnessPatchAction::Update { path, .. }
            | HarnessPatchAction::Delete { path } => path,
        };
        if !paths.contains(&path) {
            paths.push(path);
        }
    }
    Ok(paths)
}

async fn execute_apply_patch(args: &serde_json::Value) -> Result<String> {
    if get_apply_patch_text_arg(args).is_none() {
        return execute_apply_file_patch(args).await;
    }

    let input = get_apply_patch_text_arg(args)
        .ok_or_else(|| anyhow::anyhow!("missing 'input' or 'patch' argument"))?;
    let actions = parse_harness_patch_actions(input)?;
    if actions.is_empty() {
        anyhow::bail!("patch did not contain any file actions");
    }

    let mut summary = Vec::with_capacity(actions.len());
    let mut staged_files = Vec::new();
    let mut staged_indices = std::collections::HashMap::new();

    for action in actions {
        match action {
            HarnessPatchAction::Add { path, content } => {
                validate_write_path(&path)?;
                let staged_index =
                    ensure_staged_patch_file(&path, &mut staged_files, &mut staged_indices).await?;
                let staged_file = &mut staged_files[staged_index];
                if staged_file.desired_content.is_some() {
                    anyhow::bail!("cannot add file that already exists: {path}");
                }
                staged_file.desired_content = Some(content);
                summary.push(format!("Added file {path}"));
            }
            HarnessPatchAction::Update {
                path,
                old_text,
                new_text,
            } => {
                validate_write_path(&path)?;
                let staged_index =
                    ensure_staged_patch_file(&path, &mut staged_files, &mut staged_indices).await?;
                let staged_file = &mut staged_files[staged_index];
                let current_content = staged_file
                    .desired_content
                    .clone()
                    .ok_or_else(|| anyhow::anyhow!("cannot update missing file: {path}"))?;
                let (updated_content, _) = apply_replacements_to_content(
                    &path,
                    current_content,
                    vec![(old_text, new_text, false)],
                )?;
                staged_file.desired_content = Some(updated_content);
                summary.push(format!("Updated file {path}"));
            }
            HarnessPatchAction::Delete { path } => {
                validate_write_path(&path)?;
                let staged_index =
                    ensure_staged_patch_file(&path, &mut staged_files, &mut staged_indices).await?;
                let staged_file = &mut staged_files[staged_index];
                if staged_file.desired_content.is_none() {
                    anyhow::bail!("cannot delete missing file: {path}");
                }
                staged_file.desired_content = None;
                summary.push(format!("Deleted file {path}"));
            }
        }
    }

    commit_staged_harness_patch_files(&staged_files).await?;
    Ok(summary.join("\n"))
}

fn parse_harness_patch_actions(input: &str) -> Result<Vec<HarnessPatchAction>> {
    let normalized = input.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = normalized.lines().collect();
    if lines.first().copied() != Some("*** Begin Patch") {
        anyhow::bail!("patch must start with '*** Begin Patch'");
    }
    if lines.last().copied() != Some("*** End Patch") {
        anyhow::bail!("patch must end with '*** End Patch'");
    }

    let mut actions = Vec::new();
    let mut index = 1;
    while index + 1 < lines.len() {
        let line = lines[index];
        if line.trim().is_empty() {
            index += 1;
            continue;
        }

        let (kind, path) = if let Some(path) = line.strip_prefix("*** Update File: ") {
            ("update", parse_harness_patch_path(path)?)
        } else if let Some(path) = line.strip_prefix("*** Add File: ") {
            ("add", parse_harness_patch_path(path)?)
        } else if let Some(path) = line.strip_prefix("*** Delete File: ") {
            ("delete", parse_harness_patch_path(path)?)
        } else {
            anyhow::bail!("unsupported patch line: {line}");
        };

        index += 1;
        let body_start = index;
        while index + 1 < lines.len() && !lines[index].starts_with("*** ") {
            index += 1;
        }
        let body = &lines[body_start..index];

        match kind {
            "update" => actions.extend(parse_harness_update_actions(&path, body)?),
            "add" => actions.push(parse_harness_add_action(&path, body)),
            "delete" => actions.push(HarnessPatchAction::Delete { path }),
            _ => unreachable!(),
        }
    }

    Ok(actions)
}

fn parse_harness_patch_path(raw: &str) -> Result<String> {
    let path = raw.split(" -> ").next().unwrap_or(raw).trim();
    if path.is_empty() {
        anyhow::bail!("patch file path must not be empty");
    }
    Ok(path.to_string())
}

fn parse_harness_update_actions(path: &str, body: &[&str]) -> Result<Vec<HarnessPatchAction>> {
    let mut actions = Vec::new();
    let mut current_hunk = Vec::new();

    for line in body {
        if line.starts_with("@@") {
            if !current_hunk.is_empty() {
                if hunk_has_changed_lines(&current_hunk) {
                    actions.push(build_harness_update_action(path, &current_hunk)?);
                }
                current_hunk.clear();
            }
            continue;
        }
        current_hunk.push(*line);
    }

    if !current_hunk.is_empty() {
        if hunk_has_changed_lines(&current_hunk) {
            actions.push(build_harness_update_action(path, &current_hunk)?);
        }
    }

    if actions.is_empty() {
        anyhow::bail!("update patch for {path} did not contain any hunks");
    }

    Ok(actions)
}

fn hunk_has_changed_lines(hunk: &[&str]) -> bool {
    hunk.iter()
        .any(|line| line.starts_with('+') || line.starts_with('-'))
}

fn build_harness_update_action(path: &str, hunk: &[&str]) -> Result<HarnessPatchAction> {
    let mut old_lines = Vec::new();
    let mut new_lines = Vec::new();
    let mut saw_change = false;

    for line in hunk {
        if let Some(rest) = line.strip_prefix('+') {
            new_lines.push(rest);
            saw_change = true;
        } else if let Some(rest) = line.strip_prefix('-') {
            old_lines.push(rest);
            saw_change = true;
        } else if let Some(rest) = line.strip_prefix(' ') {
            old_lines.push(rest);
            new_lines.push(rest);
        } else {
            old_lines.push(*line);
            new_lines.push(*line);
        }
    }

    if !saw_change {
        anyhow::bail!(
            "update patch for {path} did not contain any changed lines; expected at least one '+' or '-' line inside each @@ hunk"
        );
    }
    if old_lines.is_empty() {
        anyhow::bail!("update patch for {path} needs existing context to locate the change");
    }

    Ok(HarnessPatchAction::Update {
        path: path.to_string(),
        old_text: old_lines.join("\n"),
        new_text: new_lines.join("\n"),
    })
}

fn parse_harness_add_action(path: &str, body: &[&str]) -> HarnessPatchAction {
    let mut content_lines = Vec::new();
    for line in body {
        if line.starts_with("@@") {
            continue;
        }
        if let Some(rest) = line.strip_prefix('+') {
            content_lines.push(rest);
        } else {
            content_lines.push(*line);
        }
    }

    let mut content = content_lines.join("\n");
    if !content.is_empty() {
        content.push('\n');
    }
    HarnessPatchAction::Add {
        path: path.to_string(),
        content,
    }
}

async fn apply_exact_replacements(
    path: &str,
    replacements: Vec<(String, String, bool)>,
) -> Result<String> {
    let target = std::path::Path::new(path);
    let content = tokio::fs::read_to_string(target).await?;
    let (content, summary) = apply_replacements_to_content(path, content, replacements)?;
    write_text_file_atomically(target, &content, true).await?;
    Ok(summary)
}

async fn write_text_file_atomically(
    path: &Path,
    content: &str,
    overwrite_existing: bool,
) -> Result<()> {
    let path = path.to_path_buf();
    let content = content.to_string();
    tokio::task::spawn_blocking(move || {
        stage_text_file_write(
            &path,
            &content,
            overwrite_existing,
            persist_staged_text_file,
        )
    })
    .await
    .map_err(|error| anyhow::anyhow!("atomic file write task failed: {error}"))??;
    Ok(())
}

fn stage_text_file_write<F>(
    path: &Path,
    content: &str,
    overwrite_existing: bool,
    commit: F,
) -> Result<()>
where
    F: FnOnce(tempfile::NamedTempFile, &Path, bool) -> std::io::Result<()>,
{
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    if path.exists() && !overwrite_existing {
        anyhow::bail!("file already exists: {}", path.display());
    }

    let mut staged = tempfile::NamedTempFile::new_in(parent)?;
    std::io::Write::write_all(staged.as_file_mut(), content.as_bytes())?;
    staged.as_file_mut().sync_all()?;

    match commit(staged, path, overwrite_existing) {
        Ok(()) => Ok(()),
        Err(error) if !overwrite_existing && error.kind() == std::io::ErrorKind::AlreadyExists => {
            anyhow::bail!("file already exists: {}", path.display())
        }
        Err(error) => Err(error.into()),
    }
}

fn persist_staged_text_file(
    staged: tempfile::NamedTempFile,
    path: &Path,
    overwrite_existing: bool,
) -> std::io::Result<()> {
    if overwrite_existing {
        staged
            .persist(path)
            .map(|_| ())
            .map_err(|error| error.error)
    } else {
        staged
            .persist_noclobber(path)
            .map(|_| ())
            .map_err(|error| error.error)
    }
}

fn apply_replacements_to_content(
    path: &str,
    mut content: String,
    replacements: Vec<(String, String, bool)>,
) -> Result<(String, String)> {
    let mut summary = Vec::with_capacity(replacements.len());

    for (index, (old_text, new_text, replace_all)) in replacements.into_iter().enumerate() {
        if old_text.is_empty() {
            anyhow::bail!("edit {} has empty 'old_text'", index + 1);
        }

        let match_count = content.matches(&old_text).count();
        if match_count == 0 {
            anyhow::bail!("edit {} target text was not found in {}", index + 1, path);
        }
        if !replace_all && match_count != 1 {
            anyhow::bail!(
                "edit {} matched {} locations in {}; set replace_all=true or provide a more specific old_text",
                index + 1,
                match_count,
                path
            );
        }

        content = if replace_all {
            content.replace(&old_text, &new_text)
        } else {
            content.replacen(&old_text, &new_text, 1)
        };
        summary.push(format!(
            "edit {} replaced {} occurrence(s)",
            index + 1,
            if replace_all { match_count } else { 1 }
        ));
    }

    Ok((
        content,
        format!("Patched {} with {}.", path, summary.join(", ")),
    ))
}

async fn ensure_staged_patch_file(
    path: &str,
    staged_files: &mut Vec<StagedHarnessPatchFile>,
    staged_indices: &mut std::collections::HashMap<String, usize>,
) -> Result<usize> {
    if let Some(index) = staged_indices.get(path).copied() {
        return Ok(index);
    }

    let original_content = match tokio::fs::read_to_string(path).await {
        Ok(content) => Some(content),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };

    let index = staged_files.len();
    staged_files.push(StagedHarnessPatchFile {
        path: path.to_string(),
        desired_content: original_content.clone(),
        original_content,
    });
    staged_indices.insert(path.to_string(), index);
    Ok(index)
}

async fn commit_staged_harness_patch_files(staged_files: &[StagedHarnessPatchFile]) -> Result<()> {
    let mut committed_indices = Vec::new();

    for (index, staged_file) in staged_files.iter().enumerate() {
        if staged_file.original_content == staged_file.desired_content {
            continue;
        }

        let result = match &staged_file.desired_content {
            Some(content) => write_staged_patch_file(&staged_file.path, content).await,
            None => remove_staged_patch_file(&staged_file.path).await,
        };

        if let Err(error) = result {
            let rollback_result =
                rollback_staged_harness_patch_files(staged_files, &committed_indices).await;
            return match rollback_result {
                Ok(()) => Err(error),
                Err(rollback_error) => Err(anyhow::anyhow!(
                    "failed to apply patch changes: {error}; rollback also failed: {rollback_error}"
                )),
            };
        }

        committed_indices.push(index);
    }

    Ok(())
}

async fn rollback_staged_harness_patch_files(
    staged_files: &[StagedHarnessPatchFile],
    committed_indices: &[usize],
) -> Result<()> {
    let mut rollback_errors = Vec::new();

    for index in committed_indices.iter().rev() {
        let staged_file = &staged_files[*index];
        let restore_result = match &staged_file.original_content {
            Some(content) => write_staged_patch_file(&staged_file.path, content).await,
            None => remove_staged_patch_file(&staged_file.path).await,
        };

        if let Err(error) = restore_result {
            rollback_errors.push(format!("{}: {error}", staged_file.path));
        }
    }

    if rollback_errors.is_empty() {
        Ok(())
    } else {
        anyhow::bail!("rollback failed for {}", rollback_errors.join(", "))
    }
}

async fn write_staged_patch_file(path: &str, content: &str) -> Result<()> {
    write_text_file_atomically(std::path::Path::new(path), content, true).await?;
    Ok(())
}

async fn remove_staged_patch_file(path: &str) -> Result<()> {
    match tokio::fs::remove_file(path).await {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

async fn execute_write_file(
    args: &serde_json::Value,
    session_manager: &Arc<SessionManager>,
    preferred_session_id: Option<SessionId>,
) -> Result<String> {
    let path = get_file_path_arg(args).ok_or_else(|| anyhow::anyhow!("missing 'path' argument"))?;
    validate_write_path(path)?;

    let content = get_file_content_arg(args)?;

    let base_dir = resolve_tool_cwd(args, session_manager, preferred_session_id).await?;
    let target = resolve_tool_path(path, base_dir.as_deref());
    write_text_file_atomically(&target, &content, true).await?;
    Ok(format!(
        "Written {} bytes to {}",
        content.len(),
        target.display()
    ))
}

fn validate_write_path(path: &str) -> Result<()> {
    if path.is_empty() {
        return Err(anyhow::anyhow!("'path' must not be empty"));
    }
    if path.trim().is_empty() {
        return Err(anyhow::anyhow!("'path' must not be blank"));
    }
    if path.trim() != path {
        return Err(anyhow::anyhow!(
            "invalid 'path': leading/trailing whitespace is not allowed"
        ));
    }
    if path.chars().any(|ch| ch.is_control()) {
        return Err(anyhow::anyhow!(
            "invalid 'path': control characters are not allowed"
        ));
    }

    Ok(())
}

fn validate_read_path(path: &str) -> Result<()> {
    if path.is_empty() {
        return Err(anyhow::anyhow!("'path' must not be empty"));
    }
    if path.trim().is_empty() {
        return Err(anyhow::anyhow!("'path' must not be blank"));
    }
    if path.trim() != path {
        return Err(anyhow::anyhow!(
            "invalid 'path': leading/trailing whitespace is not allowed"
        ));
    }
    if path.chars().any(|ch| ch.is_control()) {
        return Err(anyhow::anyhow!(
            "invalid 'path': control characters are not allowed"
        ));
    }

    Ok(())
}

async fn resolve_tool_cwd(
    args: &serde_json::Value,
    session_manager: &Arc<SessionManager>,
    preferred_session_id: Option<SessionId>,
) -> Result<Option<PathBuf>> {
    if let Some(cwd) = get_explicit_cwd_arg(args) {
        return Ok(Some(PathBuf::from(cwd)));
    }

    let sessions = session_manager.list().await;
    if sessions.is_empty() {
        return Ok(None);
    }

    let resolved = if let Some(session_ref) = args.get("session").and_then(|value| value.as_str()) {
        sessions
            .iter()
            .find(|session| {
                session.id.to_string() == session_ref
                    || session.id.to_string().contains(session_ref)
            })
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("session not found: {session_ref}"))?
    } else {
        let resolved_id = preferred_session_id.unwrap_or(sessions[0].id);
        sessions
            .into_iter()
            .find(|session| session.id == resolved_id)
            .ok_or_else(|| anyhow::anyhow!("session not found: {resolved_id}"))?
    };

    Ok(resolved.cwd.map(PathBuf::from))
}

pub(crate) fn resolve_tool_path(path: &str, base_dir: Option<&Path>) -> PathBuf {
    let path = PathBuf::from(path);
    if path.is_absolute() {
        path
    } else if let Some(base_dir) = base_dir {
        base_dir.join(path)
    } else {
        path
    }
}

fn build_write_file_command(path: &str, content: &str) -> String {
    let path_b64 = base64::engine::general_purpose::STANDARD.encode(path.as_bytes());
    let content_b64 = base64::engine::general_purpose::STANDARD.encode(content.as_bytes());
    let script = build_write_file_script(&path_b64, &content_b64);

    let script_b64 = base64::engine::general_purpose::STANDARD.encode(script.as_bytes());
    format!(
        "if command -v python3 >/dev/null 2>&1; then \
             python3 -c \"import base64;exec(base64.b64decode('{script_b64}').decode('utf-8'))\"; \
         else \
             python -c \"import base64;exec(base64.b64decode('{script_b64}').decode('utf-8'))\"; \
         fi"
    )
}

fn build_write_file_script(path_b64: &str, content_b64: &str) -> String {
    let mut script = vec![
        "import base64, pathlib".to_string(),
        format!("p = pathlib.Path(base64.b64decode('{path_b64}').decode('utf-8'))"),
        format!("data = base64.b64decode('{content_b64}')"),
        "p.parent.mkdir(parents=True, exist_ok=True)".to_string(),
        "p.write_bytes(data)".to_string(),
        "actual = p.stat().st_size".to_string(),
        "expected = len(data)".to_string(),
        "if actual != expected:".to_string(),
        "    raise SystemExit(f'size mismatch: expected {expected}, got {actual}')".to_string(),
        "print(f'written {actual} bytes to {p}')".to_string(),
    ]
    .join("\n");
    script.push('\n');
    script
}

fn build_list_files_script(path_b64: &str, token: &str) -> String {
    let mut script = vec![
        "import base64, pathlib, sys".to_string(),
        format!("p = pathlib.Path(base64.b64decode('{path_b64}').decode('utf-8'))"),
        "try:".to_string(),
        "    rows = []".to_string(),
        "    for entry in sorted(p.iterdir(), key=lambda item: item.name):".to_string(),
        "        kind = 'dir' if entry.is_dir() else 'file'".to_string(),
        "        size = entry.stat().st_size".to_string(),
        "        rows.append(f'{kind}\\t{size}\\t{entry.name}')".to_string(),
        "    payload = '\\n'.join(rows) if rows else '(empty directory)'".to_string(),
        "    status = 0".to_string(),
        "except Exception as exc:".to_string(),
        "    payload = f'Error: {exc}'".to_string(),
        "    status = 1".to_string(),
        "encoded = base64.b64encode(payload.encode('utf-8')).decode('ascii')".to_string(),
        format!("print('__AMUX_CAPTURE_BEGIN_{token}__')"),
        "print(encoded)".to_string(),
        format!("print(f'__AMUX_CAPTURE_END_{token}__:{{status}}')"),
        "sys.exit(status)".to_string(),
    ]
    .join("\n");
    script.push('\n');
    script
}
