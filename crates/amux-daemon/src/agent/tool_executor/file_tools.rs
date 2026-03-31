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

async fn execute_create_file(args: &serde_json::Value) -> Result<String> {
    let raw_path = get_file_path_arg(args)
        .ok_or_else(|| anyhow::anyhow!("missing 'path' or 'filename' argument"))?;
    validate_write_path(raw_path)?;
    let content = get_file_content_arg(args)?;
    let overwrite = args
        .get("overwrite")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let cwd = args
        .get("cwd")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let target = resolve_tool_path(raw_path, cwd.map(Path::new));
    if target.exists() && !overwrite {
        anyhow::bail!("file already exists: {}", target.display());
    }

    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(target, &content).await?;
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
    tokio::fs::write(target, existing).await?;
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

async fn apply_exact_replacements(
    path: &str,
    replacements: Vec<(String, String, bool)>,
) -> Result<String> {
    let target = std::path::Path::new(path);
    let mut content = tokio::fs::read_to_string(target).await?;
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

    tokio::fs::write(target, content).await?;
    Ok(format!("Patched {} with {}.", path, summary.join(", ")))
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
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(&target, &content).await?;
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
    if let Some(cwd) = args
        .get("cwd")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
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

fn resolve_tool_path(path: &str, base_dir: Option<&Path>) -> PathBuf {
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

