async fn execute_python_execute(
    args: &serde_json::Value,
    session_manager: &Arc<SessionManager>,
    session_id: Option<SessionId>,
    cancel_token: Option<CancellationToken>,
) -> Result<String> {
    let code = args
        .get("code")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'code' argument"))?;
    let timeout_secs = args
        .get("timeout_seconds")
        .and_then(|value| value.as_u64())
        .unwrap_or(30)
        .min(600);
    let cwd = resolve_tool_cwd(args, session_manager, session_id).await?;

    let python_bin = if tokio::process::Command::new("python3")
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .is_ok_and(|status| status.success())
    {
        "python3"
    } else {
        "python"
    };

    let mut process = tokio::process::Command::new(python_bin);
    process
        .arg("-c")
        .arg(code)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(cwd) = cwd.as_deref() {
        process.current_dir(cwd);
    }

    let mut child = process
        .spawn()
        .with_context(|| format!("failed to spawn {python_bin} subprocess"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("python_execute stdout capture was unavailable"))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow::anyhow!("python_execute stderr capture was unavailable"))?;
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
            .map_err(|_| anyhow::anyhow!("python_execute timed out after {timeout_secs}s"))?
            .context("python_execute process wait failed")
    };

    let status = if let Some(token) = cancel_token.as_ref() {
        tokio::select! {
            result = wait_result => result?,
            _ = token.cancelled() => {
                let _ = child.start_kill();
                let _ = child.wait().await;
                let _ = stdout_task.await;
                let _ = stderr_task.await;
                anyhow::bail!("python_execute cancelled while waiting for command completion");
            }
        }
    } else {
        wait_result.await?
    };

    let stdout = stdout_task
        .await
        .context("stdout collection task panicked")?
        .context("failed to read python_execute stdout")?;
    let stderr = stderr_task
        .await
        .context("stderr collection task panicked")?
        .context("failed to read python_execute stderr")?;

    let stdout = String::from_utf8_lossy(&stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&stderr).trim().to_string();
    let cwd_suffix = cwd
        .as_ref()
        .map(|path| format!(" in {}", path.display()))
        .unwrap_or_default();

    if status.success() {
        let mut result = format!(
            "Python finished successfully{cwd_suffix} (exit_code: 0, interpreter: {python_bin})."
        );
        if !stdout.is_empty() {
            result.push_str(&format!("\n\nStdout:\n{stdout}"));
        }
        if !stderr.is_empty() {
            result.push_str(&format!("\n\nStderr:\n{stderr}"));
        }
        Ok(result)
    } else {
        let mut details = String::new();
        if !stdout.is_empty() {
            details.push_str(&format!("\n\nStdout:\n{stdout}"));
        }
        if !stderr.is_empty() {
            details.push_str(&format!("\n\nStderr:\n{stderr}"));
        }
        Err(anyhow::anyhow!(
            "Python failed{cwd_suffix} (exit_code: {:?}, interpreter: {}).{}",
            status.code(),
            python_bin,
            details
        ))
    }
}
