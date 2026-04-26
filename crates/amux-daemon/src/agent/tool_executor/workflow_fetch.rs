fn emit_workflow_notice_for_tool(
    event_tx: &broadcast::Sender<AgentEvent>,
    thread_id: &str,
    tool_name: &str,
    args: &serde_json::Value,
) {
    if thread_id.trim().is_empty() {
        return;
    }

    let (kind, message, details) = match tool_name {
        "update_todo" => {
            let count = args
                .get("items")
                .and_then(|value| value.as_array())
                .map(|items| items.len())
                .unwrap_or(0);
            (
                "plan-mode",
                format!("Agent updated plan mode with {count} todo item(s)."),
                None,
            )
        }
        "update_memory" => (
            "memory-updated",
            "Agent updated persistent memory.".to_string(),
            Some(args.to_string()),
        ),
        "read_memory" | "read_user" | "read_soul" => (
            "memory-consulted",
            format!("Agent consulted persistent memory via {tool_name}."),
            Some(args.to_string()),
        ),
        "list_tools" | "tool_search" => (
            "tool-catalog",
            format!("Agent inspected available tools via {tool_name}."),
            Some(args.to_string()),
        ),
        "discover_guidelines" | "list_guidelines" | "read_guideline" => (
            "guideline-consulted",
            format!("Agent consulted local guidelines via {tool_name}."),
            Some(args.to_string()),
        ),
        "discover_skills" | "list_skills" | "read_skill" => (
            "skill-consulted",
            format!("Agent consulted local skills via {tool_name}."),
            Some(args.to_string()),
        ),
        "onecontext_search" | "session_search" | "agent_query_memory" => (
            "history-consulted",
            format!("Agent consulted history via {tool_name}."),
            Some(args.to_string()),
        ),
        "semantic_query" => (
            "semantic-query",
            "Agent consulted local workspace semantics.".to_string(),
            Some(args.to_string()),
        ),
        _ => return,
    };

    let _ = event_tx.send(AgentEvent::WorkflowNotice {
        thread_id: thread_id.to_string(),
        kind: kind.to_string(),
        message,
        details,
    });
}

fn collect_skill_documents(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) -> Result<()> {
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

        // Include any .md file in the skills tree — covers SKILL.md, generated
        // skills, and curated skill documents alike.
        let is_md = path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("md"));
        if is_md {
            out.push(path);
        }
    }

    Ok(())
}

fn resolve_skill_path(
    skills_root: &std::path::Path,
    skill: &str,
    variant: Option<&SkillVariantRecord>,
) -> Result<std::path::PathBuf> {
    validate_read_path(skill)?;
    let root_canonical = std::fs::canonicalize(skills_root).unwrap_or(skills_root.to_path_buf());

    if let Some(variant) = variant {
        let (candidate, _) = crate::agent::skill_recommendation::resolve_skill_document_path(
            skills_root,
            &variant.relative_path,
        );
        let canonical = std::fs::canonicalize(&candidate)
            .with_context(|| format!("skill '{}' was not found", skill))?;
        if !canonical.starts_with(&root_canonical) {
            anyhow::bail!("skill path must stay inside {}", skills_root.display());
        }
        return Ok(canonical);
    }

    let direct_candidate = std::path::Path::new(skill);
    if direct_candidate.components().count() > 1 || direct_candidate.is_absolute() {
        let candidate = if direct_candidate.is_absolute() {
            direct_candidate.to_path_buf()
        } else {
            skills_root.join(direct_candidate)
        };
        let canonical = std::fs::canonicalize(&candidate)
            .with_context(|| format!("skill '{}' was not found", skill))?;
        if !canonical.starts_with(&root_canonical) {
            anyhow::bail!("skill path must stay inside {}", skills_root.display());
        }
        return Ok(canonical);
    }

    let mut files = Vec::new();
    collect_skill_documents(skills_root, &mut files)?;
    let normalized = skill.to_lowercase();

    files.sort();
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

async fn sync_skill_catalog(
    skills_root: &std::path::Path,
    history: &HistoryStore,
) -> Result<Vec<SkillVariantRecord>> {
    let mut files = Vec::new();
    collect_skill_documents(skills_root, &mut files)?;
    let mut entries = Vec::new();
    for path in files {
        if let Ok(record) = history.register_skill_document(&path).await {
            entries.push(record);
        }
    }
    entries.sort_by(|left, right| {
        left.skill_name
            .cmp(&right.skill_name)
            .then_with(|| left.variant_name.cmp(&right.variant_name))
            .then_with(|| left.relative_path.cmp(&right.relative_path))
    });
    Ok(entries)
}

async fn resolve_skill_context_tags(
    workspace_root: Option<&PathBuf>,
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
    .or_else(|| workspace_root.cloned())
    .or_else(|| std::env::current_dir().ok());

    root.filter(|path| path.is_dir())
        .map(|path| infer_workspace_context_tags(&path))
        .unwrap_or_default()
}

async fn execute_fetch_url(
    args: &serde_json::Value,
    http_client: &reqwest::Client,
    browse_provider: &str,
) -> Result<String> {
    let browser = resolve_browser(browse_provider);

    execute_fetch_url_with_runner(
        args,
        browser.is_some(),
        |url, timeout_seconds| async move {
            let browser = browser
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("no headless browser available"))?;
            fetch_with_headless_browser(browser, &url, timeout_seconds).await
        },
        |url, timeout_seconds| async move { fetch_raw_http(http_client, &url, timeout_seconds).await },
    )
    .await
}

async fn execute_fetch_url_with_runner<BrowserRunner, BrowserFut, HttpRunner, HttpFut>(
    args: &serde_json::Value,
    browser_available: bool,
    browser_runner: BrowserRunner,
    http_runner: HttpRunner,
) -> Result<String>
where
    BrowserRunner: FnOnce(String, u64) -> BrowserFut,
    BrowserFut: Future<Output = Result<String>>,
    HttpRunner: FnOnce(String, u64) -> HttpFut,
    HttpFut: Future<Output = Result<String>>,
{
    let request = fetch_url_request(args)?;
    let timeout_seconds = request.timeout_seconds;
    let started = tokio::time::Instant::now();
    let max_length = request.max_length;
    let url = request.url;

    let remaining_budget = |started: tokio::time::Instant| -> Result<std::time::Duration> {
        std::time::Duration::from_secs(timeout_seconds)
            .checked_sub(started.elapsed())
            .ok_or_else(|| anyhow::anyhow!("fetch_url timed out after {timeout_seconds} seconds"))
    };

    // Try headless browser for JS-rendered content, fall back to raw HTTP.
    let raw_html = if browser_available {
        match tokio::time::timeout(
            remaining_budget(started)?,
            browser_runner(url.clone(), timeout_seconds),
        )
        .await
        .map_err(|_| anyhow::anyhow!("fetch_url timed out after {timeout_seconds} seconds"))
        {
            Ok(Ok(html)) => html,
            Ok(Err(e)) => {
                if is_fetch_url_timeout_error(&e) {
                    return Err(anyhow::anyhow!(
                        "fetch_url timed out after {timeout_seconds} seconds"
                    ));
                }
                tracing::warn!("headless browser fetch failed, falling back to HTTP: {e}");
                tokio::time::timeout(
                    remaining_budget(started)?,
                    http_runner(url.clone(), timeout_seconds),
                )
                .await
                .map_err(|_| {
                    anyhow::anyhow!("fetch_url timed out after {timeout_seconds} seconds")
                })??
            }
            Err(err) => return Err(err),
        }
    } else {
        tokio::time::timeout(
            remaining_budget(started)?,
            http_runner(url.clone(), timeout_seconds),
        )
        .await
        .map_err(|_| anyhow::anyhow!("fetch_url timed out after {timeout_seconds} seconds"))??
    };

    let stripped = strip_html_tags(&raw_html);
    let truncated = if stripped.len() > max_length {
        format!(
            "{}...\n\n(truncated, {} chars total)",
            &stripped[..max_length],
            stripped.len()
        )
    } else {
        stripped
    };

    Ok(truncated)
}

fn is_fetch_url_timeout_error(error: &anyhow::Error) -> bool {
    error.to_string().to_ascii_lowercase().contains("timed out")
}

async fn fetch_raw_http(
    http_client: &reqwest::Client,
    url: &str,
    timeout_seconds: u64,
) -> Result<String> {
    let resp = http_client
        .get(url)
        .header("User-Agent", "tamux-agent/0.1")
        .timeout(std::time::Duration::from_secs(timeout_seconds))
        .send()
        .await?;
    let status = resp.status();
    let text = resp.text().await?;
    Ok(format!("HTTP {status}\n\n{text}"))
}

/// Detected headless browser binary and its args for dump-dom mode.
struct HeadlessBrowser {
    bin: String,
    /// Extra args to produce DOM text on stdout for a given URL.
    args_prefix: Vec<String>,
}

/// Resolve which headless browser to use.
/// "auto" tries lightpanda → chrome → chromium → none.
fn resolve_browser(preference: &str) -> Option<HeadlessBrowser> {
    match preference {
        "none" | "off" | "" => None,
        "lightpanda" => detect_lightpanda(),
        "chrome" | "chromium" => detect_chrome(),
        "auto" | _ => detect_lightpanda().or_else(detect_chrome),
    }
}

fn detect_lightpanda() -> Option<HeadlessBrowser> {
    which::which("lightpanda").ok().map(|path| HeadlessBrowser {
        bin: path.to_string_lossy().to_string(),
        args_prefix: vec![
            "fetch".to_string(),
            "--output".to_string(),
            "dom-text".to_string(),
        ],
    })
}

fn detect_chrome() -> Option<HeadlessBrowser> {
    let candidates = [
        "google-chrome-stable",
        "google-chrome",
        "chromium-browser",
        "chromium",
    ];
    for name in candidates {
        if let Ok(path) = which::which(name) {
            return Some(HeadlessBrowser {
                bin: path.to_string_lossy().to_string(),
                args_prefix: vec![
                    "--headless=new".to_string(),
                    "--no-sandbox".to_string(),
                    "--disable-gpu".to_string(),
                    "--dump-dom".to_string(),
                ],
            });
        }
    }
    None
}

async fn fetch_with_headless_browser(
    browser: &HeadlessBrowser,
    url: &str,
    timeout_seconds: u64,
) -> Result<String> {
    let mut args = browser.args_prefix.clone();
    args.push(url.to_string());

    let child = tokio::process::Command::new(&browser.bin)
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()?;

    let output = tokio::time::timeout(
        std::time::Duration::from_secs(timeout_seconds),
        child.wait_with_output(),
    )
    .await
    .map_err(|_| {
        anyhow::anyhow!("headless browser fetch timed out after {timeout_seconds} seconds")
    })??;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "headless browser exited with {}: {}",
            output.status,
            &stderr[..stderr.len().min(200)]
        );
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

// ---------------------------------------------------------------------------
// Web browsing setup tool
// ---------------------------------------------------------------------------
