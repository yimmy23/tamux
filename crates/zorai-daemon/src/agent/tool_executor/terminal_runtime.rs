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
        if session_manager.list().await.is_empty()
            && can_run_headless_when_terminal_unavailable(args)
        {
            return execute_headless_shell_command(
                args,
                session_manager,
                session_id,
                tool_names::RUN_TERMINAL_COMMAND,
                cancel_token,
                None,
            )
            .await;
        }
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
            tool_names::RUN_TERMINAL_COMMAND,
            cancel_token,
            None,
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
    let forced_background_args;
    let args = if bash_command_should_force_background(args) {
        forced_background_args = bash_command_args_with_wait_false(args);
        &forced_background_args
    } else {
        args
    };
    let client_surface = resolve_shell_tool_client_surface(agent, thread_id, task_id).await;
    let foreground_detach_after = headless_foreground_detach_after_for_surface(client_surface);
    if should_use_managed_execution_for_surface(client_surface, args) {
        if session_manager.list().await.is_empty()
            && can_run_headless_when_terminal_unavailable(args)
        {
            return execute_headless_shell_command(
                args,
                session_manager,
                session_id,
                tool_names::BASH_COMMAND,
                cancel_token,
                foreground_detach_after,
            )
            .await;
        }
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
            tool_names::BASH_COMMAND,
            cancel_token,
            foreground_detach_after,
        )
        .await
    }
}

const TUI_HEADLESS_FOREGROUND_GRACE_SECS: u64 = 1;

fn bash_command_should_force_background(args: &serde_json::Value) -> bool {
    let wait_for_completion = args
        .get("wait_for_completion")
        .and_then(|value| value.as_bool())
        .unwrap_or(true);
    if !wait_for_completion {
        return false;
    }

    let requested_timeout = args
        .get("timeout_seconds")
        .and_then(|value| value.as_u64())
        .unwrap_or(30);
    if requested_timeout > 600 {
        return false;
    }

    !bash_command_can_wait_for_completion(args)
}

fn bash_command_args_with_wait_false(args: &serde_json::Value) -> serde_json::Value {
    let mut mapped = args.as_object().cloned().unwrap_or_default();
    mapped.insert(
        "wait_for_completion".to_string(),
        serde_json::Value::Bool(false),
    );
    serde_json::Value::Object(mapped)
}

fn bash_command_can_wait_for_completion(args: &serde_json::Value) -> bool {
    if args
        .get("wait_for_completion")
        .and_then(|value| value.as_bool())
        == Some(false)
    {
        return false;
    }

    let Some(command) = args
        .get("command")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return false;
    };

    command_is_known_quick_shell_command(command)
}

fn command_is_known_quick_shell_command(command: &str) -> bool {
    let trimmed = command.trim();
    if trimmed.is_empty()
        || trimmed.contains('\n')
        || trimmed.contains('|')
        || trimmed.contains('`')
        || trimmed.contains("$(")
    {
        return false;
    }

    trimmed
        .split(';')
        .flat_map(|part| part.split("&&"))
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .all(shell_command_segment_is_known_quick)
}

fn shell_command_segment_is_known_quick(segment: &str) -> bool {
    let first = segment
        .split_whitespace()
        .next()
        .unwrap_or_default()
        .trim_matches(|ch: char| ch == '(' || ch == ')' || ch == '"' || ch == '\'')
        .rsplit('/')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();

    matches!(
        first.as_str(),
        "pwd"
            | "printf"
            | "echo"
            | "true"
            | "false"
            | "test"
            | "["
            | "ls"
            | "cat"
            | "head"
            | "tail"
            | "wc"
            | "stat"
            | "date"
            | "whoami"
            | "id"
            | "uname"
            | "basename"
            | "dirname"
            | "which"
            | "command"
            | "type"
            | "realpath"
            | "readlink"
    )
}

fn headless_foreground_detach_after_for_surface(
    client_surface: Option<zorai_protocol::ClientSurface>,
) -> Option<std::time::Duration> {
    matches!(client_surface, Some(zorai_protocol::ClientSurface::Tui))
        .then(|| std::time::Duration::from_secs(TUI_HEADLESS_FOREGROUND_GRACE_SECS))
}

async fn resolve_shell_tool_client_surface(
    agent: &AgentEngine,
    thread_id: &str,
    task_id: Option<&str>,
) -> Option<zorai_protocol::ClientSurface> {
    if let Some(client_surface) = agent.get_thread_client_surface(thread_id).await {
        return Some(client_surface);
    }

    let task_id = task_id?;
    let (task_thread_ids, goal_run_id) = {
        let tasks = agent.tasks.lock().await;
        let task = tasks
            .iter()
            .find(|task| task.id == task_id)?;
        let mut thread_ids = Vec::new();
        if let Some(task_thread_id) = task.thread_id.as_deref() {
            thread_ids.push(task_thread_id.to_string());
        }
        if let Some(parent_thread_id) = task.parent_thread_id.as_deref() {
            thread_ids.push(parent_thread_id.to_string());
        }
        (thread_ids, task.goal_run_id.clone())
    };

    for task_thread_id in task_thread_ids {
        if task_thread_id == thread_id {
            continue;
        }
        if let Some(client_surface) = agent.get_thread_client_surface(&task_thread_id).await {
            return Some(client_surface);
        }
    }

    if let Some(goal_run_id) = goal_run_id {
        return agent.get_goal_run_client_surface(&goal_run_id).await;
    }

    None
}

fn should_use_managed_execution(args: &serde_json::Value) -> bool {
    should_use_managed_execution_for_surface(None, args)
}

fn should_use_managed_execution_for_surface(
    client_surface: Option<zorai_protocol::ClientSurface>,
    args: &serde_json::Value,
) -> bool {
    if matches!(client_surface, Some(zorai_protocol::ClientSurface::Tui)) {
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

fn can_run_headless_when_terminal_unavailable(args: &serde_json::Value) -> bool {
    if args
        .get("session")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .is_some_and(|value| !value.is_empty())
    {
        return false;
    }

    let Some(command) = args
        .get("command")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return false;
    };

    if command_requires_managed_state(command) || command_looks_interactive(command) {
        return false;
    }

    let security_level = args.get("security_level").and_then(|value| value.as_str());
    if matches!(security_level, Some("highest")) {
        return false;
    }

    matches!(security_level, Some("yolo")) || !command_matches_policy_risk(command)
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

struct HeadlessOutputCapture {
    buffer: Arc<std::sync::Mutex<Vec<u8>>>,
    task: tokio::task::JoinHandle<Result<(), std::io::Error>>,
}

fn spawn_headless_output_capture<R>(stream: R) -> HeadlessOutputCapture
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    let buffer = Arc::new(std::sync::Mutex::new(Vec::new()));
    let task_buffer = Arc::clone(&buffer);
    let task = tokio::spawn(async move {
        let mut reader = tokio::io::BufReader::new(stream);
        let mut chunk = [0u8; 8192];
        loop {
            let read = reader.read(&mut chunk).await?;
            if read == 0 {
                return Ok(());
            }
            let mut captured = task_buffer
                .lock()
                .expect("headless command output buffer mutex poisoned");
            captured.extend_from_slice(&chunk[..read]);
        }
    });

    HeadlessOutputCapture { buffer, task }
}

async fn collect_headless_output_capture(
    mut capture: HeadlessOutputCapture,
    max_chars: usize,
    drain_grace: std::time::Duration,
) -> Option<String> {
    tokio::select! {
        _ = &mut capture.task => {}
        _ = tokio::time::sleep(drain_grace) => {
            capture.task.abort();
            let _ = capture.task.await;
        }
    }

    let bytes = capture
        .buffer
        .lock()
        .expect("headless command output buffer mutex poisoned")
        .clone();
    Some(compact_background_output(&bytes, max_chars)).filter(|value| !value.is_empty())
}

async fn execute_headless_shell_command(
    args: &serde_json::Value,
    session_manager: &Arc<SessionManager>,
    session_id: Option<SessionId>,
    tool_name: &str,
    cancel_token: Option<CancellationToken>,
    foreground_detach_after: Option<std::time::Duration>,
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
    let stdout_capture = spawn_headless_output_capture(stdout);
    let stderr_capture = spawn_headless_output_capture(stderr);

    let full_timeout = std::time::Duration::from_secs(timeout_secs);
    let foreground_wait = foreground_detach_after
        .map(|duration| duration.min(full_timeout))
        .unwrap_or(full_timeout);
    let wait_result = tokio::time::timeout(foreground_wait, child.wait());

    let status = if let Some(token) = cancel_token.as_ref() {
        tokio::select! {
            result = wait_result => result,
            _ = token.cancelled() => {
                let _ = child.start_kill();
                let _ = child.wait().await;
                let _ = collect_headless_output_capture(
                    stdout_capture,
                    usize::MAX,
                    std::time::Duration::from_secs(5),
                ).await;
                let _ = collect_headless_output_capture(
                    stderr_capture,
                    usize::MAX,
                    std::time::Duration::from_secs(5),
                ).await;
                anyhow::bail!("{tool_name} cancelled while waiting for command completion");
            }
        }
    } else {
        wait_result.await
    };

    let status = match status {
        Ok(result) => result.with_context(|| format!("{tool_name} process wait failed"))?,
        Err(_) if foreground_detach_after.is_some() && foreground_wait < full_timeout => {
            let operation = crate::server::operation_registry().accept_operation(tool_name, None);
            let operation_id = operation.operation_id.clone();
            crate::server::operation_registry().mark_started(&operation_id);
            spawn_headless_shell_monitor(
                operation_id.clone(),
                command.to_string(),
                cwd.clone(),
                child,
                stdout_capture,
                stderr_capture,
            );
            let cwd_suffix = cwd
                .as_ref()
                .map(|path| format!(" in {}", path.display()))
                .unwrap_or_default();
            return Ok((
                format!(
                    "Headless command detached{cwd_suffix} as background operation {operation_id} after {}s foreground grace.\nbackground_task_id: {operation_id}\noperation_id: {operation_id}\nwait_for_completion=true exceeded the TUI foreground grace window. A background monitor will notify this thread when the command completes. Use get_operation_status with this operation_id if you need more details before then.",
                    foreground_wait.as_secs()
                ),
                None,
            ));
        }
        Err(_) => {
            anyhow::bail!("{tool_name} timed out after {timeout_secs}s");
        }
    };

    let stdout = collect_headless_output_capture(
        stdout_capture,
        usize::MAX,
        std::time::Duration::from_secs(5),
    )
    .await
    .unwrap_or_default();
    let stderr = collect_headless_output_capture(
        stderr_capture,
        usize::MAX,
        std::time::Duration::from_secs(5),
    )
    .await
    .unwrap_or_default();
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
    let stdout_capture = spawn_headless_output_capture(stdout);
    let stderr_capture = spawn_headless_output_capture(stderr);

    crate::server::operation_registry().mark_started(&operation_id);
    spawn_headless_shell_monitor(
        operation_id.clone(),
        command.to_string(),
        cwd.clone(),
        child,
        stdout_capture,
        stderr_capture,
    );

    let cwd_suffix = cwd
        .as_ref()
        .map(|path| format!(" in {}", path.display()))
        .unwrap_or_default();
    let queued_summary =
        format!("Headless command queued{cwd_suffix} as background operation {operation_id}.");

    if auto_background {
        Ok((
            format!(
                "{queued_summary}\nbackground_task_id: {operation_id}\noperation_id: {operation_id}\nCommand auto-backgrounded (requested timeout {}s > max 600s). A background monitor will notify this thread when the command completes. Use get_operation_status with this operation_id if you need more details before then.",
                requested_timeout,
            ),
            None,
        ))
    } else {
        Ok((
            format!(
                "{queued_summary}\nbackground_task_id: {operation_id}\noperation_id: {operation_id}\nNot waiting for completion because wait_for_completion=false. A background monitor will notify this thread when the command completes. Use get_operation_status with this operation_id if you need more details before then."
            ),
            None,
        ))
    }
}

fn spawn_headless_shell_monitor(
    operation_id: String,
    command: String,
    cwd: Option<PathBuf>,
    mut child: tokio::process::Child,
    stdout_capture: HeadlessOutputCapture,
    stderr_capture: HeadlessOutputCapture,
) {
    let cwd_for_task = cwd.as_ref().map(|path| path.display().to_string());
    let started_at = std::time::Instant::now();
    tokio::spawn(async move {
        let outcome = child.wait().await;
        let stdout = collect_headless_output_capture(
            stdout_capture,
            4000,
            std::time::Duration::from_millis(100),
        )
        .await;
        let stderr = collect_headless_output_capture(
            stderr_capture,
            4000,
            std::time::Duration::from_millis(100),
        )
        .await;
        let duration_ms = started_at.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;

        match outcome {
            Ok(status) if status.success() => {
                crate::server::operation_registry().mark_completed_with_terminal_result(
                    &operation_id,
                    serde_json::json!({
                        "command": command,
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
                    &operation_id,
                    serde_json::json!({
                        "command": command,
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
                    &operation_id,
                    serde_json::json!({
                        "command": command,
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
}
