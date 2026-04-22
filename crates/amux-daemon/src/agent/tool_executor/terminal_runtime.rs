async fn execute_read_terminal(
    args: &serde_json::Value,
    session_manager: &Arc<SessionManager>,
) -> Result<String> {
    let sessions = session_manager.list().await;
    if sessions.is_empty() {
        return Ok("No active terminal sessions.".into());
    }

    let target_id = if let Some(pane) = args.get("pane").and_then(|v| v.as_str()) {
        sessions
            .iter()
            .find(|s| s.id.to_string().contains(pane))
            .map(|s| s.id)
    } else {
        None
    };

    let sid = target_id.unwrap_or(sessions[0].id);

    // Read full scrollback, no line limit — get everything the session has
    match session_manager.get_scrollback(sid, None).await {
        Ok(data) => {
            if data.is_empty() {
                return Ok("(terminal buffer is empty)".into());
            }

            // Strip ANSI escapes using the strip-ansi-escapes crate (already in deps)
            let stripped = strip_ansi_escapes::strip(&data);
            let text = String::from_utf8_lossy(&stripped);

            // Take last 200 lines to keep output manageable
            let lines: Vec<&str> = text.lines().collect();
            let start = if lines.len() > 200 {
                lines.len() - 200
            } else {
                0
            };
            let visible: Vec<&str> = lines[start..]
                .iter()
                .filter(|l| !l.trim().is_empty())
                .copied()
                .collect();

            if visible.is_empty() {
                Ok("(terminal buffer contains only whitespace/control characters)".into())
            } else {
                let mut result = visible.join("\n");
                if start > 0 {
                    result = format!("... ({} earlier lines omitted)\n\n{result}", start);
                }
                Ok(result)
            }
        }
        Err(e) => Ok(format!("Failed to read terminal: {e}")),
    }
}

async fn execute_run_terminal_command(
    args: &serde_json::Value,
    agent: &AgentEngine,
    task_id: Option<&str>,
    session_manager: &Arc<SessionManager>,
    session_id: Option<SessionId>,
    event_tx: &broadcast::Sender<AgentEvent>,
    thread_id: &str,
    cancel_token: Option<CancellationToken>,
) -> Result<(String, Option<ToolPendingApproval>)> {
    let client_surface = resolve_shell_tool_client_surface(agent, thread_id, task_id).await;
    if should_use_managed_execution_for_surface(client_surface, args) {
        let managed_args =
            managed_alias_args(args, "Run a shell command in a managed terminal session");
        execute_managed_command(
            &managed_args,
            agent,
            session_manager,
            session_id,
            event_tx,
            thread_id,
            cancel_token,
        )
        .await
    } else {
        execute_headless_shell_command(
            args,
            session_manager,
            session_id,
            "run_terminal_command",
            cancel_token,
        )
        .await
    }
}

async fn execute_bash_command(
    args: &serde_json::Value,
    agent: &AgentEngine,
    task_id: Option<&str>,
    session_manager: &Arc<SessionManager>,
    session_id: Option<SessionId>,
    event_tx: &broadcast::Sender<AgentEvent>,
    thread_id: &str,
    cancel_token: Option<CancellationToken>,
) -> Result<(String, Option<ToolPendingApproval>)> {
    let client_surface = resolve_shell_tool_client_surface(agent, thread_id, task_id).await;
    if should_use_managed_execution_for_surface(client_surface, args) {
        let managed_args =
            managed_alias_args(args, "Run a shell command in a managed terminal session");
        execute_managed_command(
            &managed_args,
            agent,
            session_manager,
            session_id,
            event_tx,
            thread_id,
            cancel_token,
        )
        .await
    } else {
        execute_headless_shell_command(
            args,
            session_manager,
            session_id,
            "bash_command",
            cancel_token,
        )
        .await
    }
}

async fn resolve_shell_tool_client_surface(
    agent: &AgentEngine,
    thread_id: &str,
    task_id: Option<&str>,
) -> Option<amux_protocol::ClientSurface> {
    if let Some(client_surface) = agent.get_thread_client_surface(thread_id).await {
        return Some(client_surface);
    }

    let task_id = task_id?;
    let goal_run_id = {
        let tasks = agent.tasks.lock().await;
        tasks
            .iter()
            .find(|task| task.id == task_id)
            .and_then(|task| task.goal_run_id.clone())
    }?;

    agent.get_goal_run_client_surface(&goal_run_id).await
}

fn should_use_managed_execution(args: &serde_json::Value) -> bool {
    should_use_managed_execution_for_surface(None, args)
}

fn should_use_managed_execution_for_surface(
    client_surface: Option<amux_protocol::ClientSurface>,
    args: &serde_json::Value,
) -> bool {
    if matches!(client_surface, Some(amux_protocol::ClientSurface::Tui)) {
        return false;
    }

    if args
        .get("__weles_force_headless")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    {
        return false;
    }

    if args
        .get("session")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .is_some_and(|value| !value.is_empty())
    {
        return true;
    }

    if args
        .get("wait_for_completion")
        .and_then(|value| value.as_bool())
        == Some(false)
    {
        return true;
    }

    if args
        .get("timeout_seconds")
        .and_then(|value| value.as_u64())
        .is_some_and(|value| value > 600)
    {
        return true;
    }

    if args
        .get("sandbox_enabled")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    {
        return true;
    }

    if args
        .get("allow_network")
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
    {
        return true;
    }

    let security_level = args.get("security_level").and_then(|value| value.as_str());

    if matches!(security_level, Some("highest")) {
        return true;
    }

    if !matches!(security_level, Some("yolo"))
        && args
            .get("command")
            .and_then(|value| value.as_str())
            .is_some_and(command_matches_policy_risk)
    {
        return true;
    }

    args.get("command")
        .and_then(|value| value.as_str())
        .map(|command| {
            command_requires_managed_state(command) || command_looks_interactive(command)
        })
        .unwrap_or(false)
}

fn command_matches_policy_risk(command: &str) -> bool {
    if command.trim().is_empty() {
        return false;
    }
    let request = ManagedCommandRequest {
        command: command.to_string(),
        rationale: "policy preflight".to_string(),
        allow_network: false,
        sandbox_enabled: false,
        security_level: SecurityLevel::Lowest,
        cwd: None,
        language_hint: None,
        source: ManagedCommandSource::Agent,
    };
    matches!(
        crate::policy::evaluate_command("tool-exec-routing-check".to_string(), &request, None),
        crate::policy::PolicyDecision::RequireApproval(_)
    )
}

fn command_requires_managed_state(command: &str) -> bool {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return false;
    }

    let normalized = trimmed.to_ascii_lowercase();
    let first = normalized
        .split(|ch: char| ch.is_whitespace() || ch == ';' || ch == '&' || ch == '|')
        .next()
        .unwrap_or_default();

    let requires_persistent_session = matches!(
        first,
        "cd" | "pushd"
            | "popd"
            | "export"
            | "unset"
            | "alias"
            | "unalias"
            | "source"
            | "."
            | "set"
            | "ulimit"
            | "umask"
            | "bind"
            | "shopt"
            | "complete"
            | "compgen"
            | "fg"
            | "bg"
            | "jobs"
    );

    requires_persistent_session && !command_has_followup_work(trimmed)
}

fn command_has_followup_work(command: &str) -> bool {
    ["&&", "||", ";", "\n"].iter().any(|separator| {
        command
            .find(separator)
            .map(|index| !command[index + separator.len()..].trim().is_empty())
            .unwrap_or(false)
    })
}

fn command_looks_interactive(command: &str) -> bool {
    let trimmed = command.trim();
    if trimmed.is_empty() {
        return false;
    }

    let mut parts = trimmed.split_whitespace();
    let first = parts.next().unwrap_or_default().to_ascii_lowercase();
    let second = parts.next().map(|value| value.to_ascii_lowercase());

    match first.as_str() {
        "vim" | "nvim" | "nano" | "less" | "more" | "top" | "htop" | "watch" => true,
        "tail" => matches!(second.as_deref(), Some("-f")),
        "ssh" | "sftp" | "scp" | "man" => true,
        "bash" | "zsh" | "fish" => matches!(
            second.as_deref(),
            None | Some("-i") | Some("--interactive") | Some("-l") | Some("--login")
        ),
        "python" | "python3" | "ipython" | "node" => second.is_none(),
        _ => false,
    }
}

fn compact_background_output(raw: &[u8], max_chars: usize) -> String {
    let text = String::from_utf8_lossy(raw).trim().to_string();
    if text.chars().count() <= max_chars {
        return text;
    }

    let tail: String = text
        .chars()
        .rev()
        .take(max_chars)
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    format!("... truncated ...\n{tail}")
}

async fn execute_headless_shell_command(
    args: &serde_json::Value,
    session_manager: &Arc<SessionManager>,
    session_id: Option<SessionId>,
    tool_name: &str,
    cancel_token: Option<CancellationToken>,
) -> Result<(String, Option<ToolPendingApproval>)> {
    let command = args
        .get("command")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'command' argument"))?;
    let requested_timeout = args
        .get("timeout_seconds")
        .and_then(|value| value.as_u64())
        .unwrap_or(30);
    let auto_background = requested_timeout > 600;
    let wait_for_completion = if auto_background {
        false
    } else {
        args.get("wait_for_completion")
            .and_then(|value| value.as_bool())
            .unwrap_or(true)
    };
    let timeout_secs = requested_timeout.min(600);
    let cwd = resolve_tool_cwd(args, session_manager, session_id).await?;

    if !wait_for_completion {
        return spawn_headless_shell_command_background(
            command,
            cwd,
            tool_name,
            requested_timeout,
            auto_background,
        );
    }

    let mut process = tokio::process::Command::new("bash");
    process
        .arg("-lc")
        .arg(command)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(cwd) = cwd.as_deref() {
        process.current_dir(cwd);
    }

    let mut child = process
        .spawn()
        .with_context(|| format!("failed to spawn {tool_name} subprocess"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("{tool_name} stdout capture was unavailable"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow::anyhow!("{tool_name} stderr capture was unavailable"))?;
    let stdout_task = tokio::spawn(async move {
        let mut reader = tokio::io::BufReader::new(stdout);
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).await?;
        Ok::<Vec<u8>, std::io::Error>(buf)
    });
    let stderr_task = tokio::spawn(async move {
        let mut reader = tokio::io::BufReader::new(stderr);
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).await?;
        Ok::<Vec<u8>, std::io::Error>(buf)
    });

    let wait_result = async {
        tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), child.wait())
            .await
            .map_err(|_| anyhow::anyhow!("{tool_name} timed out after {timeout_secs}s"))?
            .with_context(|| format!("{tool_name} process wait failed"))
    };

    let status = if let Some(token) = cancel_token.as_ref() {
        tokio::select! {
            result = wait_result => result?,
            _ = token.cancelled() => {
                let _ = child.start_kill();
                let _ = child.wait().await;
                let _ = stdout_task.await;
                let _ = stderr_task.await;
                anyhow::bail!("{tool_name} cancelled while waiting for command completion");
            }
        }
    } else {
        wait_result.await?
    };

    let stdout = stdout_task
        .await
        .context("stdout collection task panicked")?
        .context("failed to read command stdout")?;
    let stderr = stderr_task
        .await
        .context("stderr collection task panicked")?
        .context("failed to read command stderr")?;

    let stdout = String::from_utf8_lossy(&stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&stderr).trim().to_string();
    let cwd_suffix = cwd
        .as_ref()
        .map(|path| format!(" in {}", path.display()))
        .unwrap_or_default();

    if status.success() {
        let mut result = format!("Command finished successfully{cwd_suffix} (exit_code: 0).");
        if !stdout.is_empty() {
            result.push_str(&format!("\n\nStdout:\n{stdout}"));
        }
        if !stderr.is_empty() {
            result.push_str(&format!("\n\nStderr:\n{stderr}"));
        }
        Ok((result, None))
    } else {
        let mut details = String::new();
        if !stdout.is_empty() {
            details.push_str(&format!("\n\nStdout:\n{stdout}"));
        }
        if !stderr.is_empty() {
            details.push_str(&format!("\n\nStderr:\n{stderr}"));
        }
        Err(anyhow::anyhow!(
            "Command failed{cwd_suffix} (exit_code: {:?}).{}",
            status.code(),
            details
        ))
    }
}

fn spawn_headless_shell_command_background(
    command: &str,
    cwd: Option<std::path::PathBuf>,
    tool_name: &str,
    requested_timeout: u64,
    auto_background: bool,
) -> Result<(String, Option<ToolPendingApproval>)> {
    let operation = crate::server::operation_registry().accept_operation(tool_name, None);
    let operation_id = operation.operation_id.clone();

    let mut process = tokio::process::Command::new("bash");
    process
        .arg("-lc")
        .arg(command)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(cwd) = cwd.as_deref() {
        process.current_dir(cwd);
    }

    let mut child = match process.spawn() {
        Ok(child) => child,
        Err(error) => {
            crate::server::operation_registry().mark_failed(&operation_id);
            return Err(error).with_context(|| format!("failed to spawn {tool_name} subprocess"));
        }
    };
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("{tool_name} stdout capture was unavailable"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow::anyhow!("{tool_name} stderr capture was unavailable"))?;
    let stdout_task = tokio::spawn(async move {
        let mut reader = tokio::io::BufReader::new(stdout);
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).await?;
        Ok::<Vec<u8>, std::io::Error>(buf)
    });
    let stderr_task = tokio::spawn(async move {
        let mut reader = tokio::io::BufReader::new(stderr);
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).await?;
        Ok::<Vec<u8>, std::io::Error>(buf)
    });

    crate::server::operation_registry().mark_started(&operation_id);

    let operation_id_for_task = operation_id.clone();
    let command_for_task = command.to_string();
    let cwd_for_task = cwd.as_ref().map(|path| path.display().to_string());
    let started_at = std::time::Instant::now();
    tokio::spawn(async move {
        let outcome = child.wait().await;
        let stdout = stdout_task
            .await
            .ok()
            .and_then(|result| result.ok())
            .map(|bytes| compact_background_output(&bytes, 4000))
            .filter(|value| !value.is_empty());
        let stderr = stderr_task
            .await
            .ok()
            .and_then(|result| result.ok())
            .map(|bytes| compact_background_output(&bytes, 4000))
            .filter(|value| !value.is_empty());
        let duration_ms = started_at.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;

        match outcome {
            Ok(status) if status.success() => {
                crate::server::operation_registry().mark_completed_with_terminal_result(
                    &operation_id_for_task,
                    serde_json::json!({
                        "command": command_for_task,
                        "cwd": cwd_for_task,
                        "exit_code": status.code(),
                        "duration_ms": duration_ms,
                        "stdout": stdout,
                        "stderr": stderr,
                    }),
                );
            }
            Ok(status) => {
                crate::server::operation_registry().mark_failed_with_terminal_result(
                    &operation_id_for_task,
                    serde_json::json!({
                        "command": command_for_task,
                        "cwd": cwd_for_task,
                        "exit_code": status.code(),
                        "duration_ms": duration_ms,
                        "stdout": stdout,
                        "stderr": stderr,
                    }),
                );
            }
            Err(error) => {
                crate::server::operation_registry().mark_failed_with_terminal_result(
                    &operation_id_for_task,
                    serde_json::json!({
                        "command": command_for_task,
                        "cwd": cwd_for_task,
                        "duration_ms": duration_ms,
                        "spawn_error": error.to_string(),
                        "stdout": stdout,
                        "stderr": stderr,
                    }),
                );
            }
        }
    });

    let cwd_suffix = cwd
        .as_ref()
        .map(|path| format!(" in {}", path.display()))
        .unwrap_or_default();
    let queued_summary =
        format!("Headless command queued{cwd_suffix} as background operation {operation_id}.");

    if auto_background {
        Ok((
            format!(
                "{queued_summary}\noperation_id: {operation_id}\nCommand auto-backgrounded (requested timeout {}s > max 600s). Use get_operation_status with this operation_id for explicit polling.",
                requested_timeout,
            ),
            None,
        ))
    } else {
        Ok((
            format!(
                "{queued_summary}\noperation_id: {operation_id}\nNot waiting for completion because wait_for_completion=false. Use get_operation_status with this operation_id for explicit polling."
            ),
            None,
        ))
    }
}
