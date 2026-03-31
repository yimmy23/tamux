fn default_task_title(description: &str, command: Option<&str>) -> String {
    let source = command.unwrap_or(description).trim();
    if source.is_empty() {
        return "Queued task".to_string();
    }

    let mut title = source.lines().next().unwrap_or(source).trim().to_string();
    if title.len() > 72 {
        title.truncate(69);
        title.push_str("...");
    }
    title
}

fn parse_scheduled_at(args: &serde_json::Value) -> Result<Option<u64>> {
    if let Some(timestamp) = args.get("scheduled_at").and_then(|value| value.as_u64()) {
        return Ok(Some(timestamp));
    }

    if let Some(value) = args.get("schedule_at").and_then(|value| value.as_str()) {
        let timestamp = humantime::parse_rfc3339_weak(value)
            .map_err(|error| anyhow::anyhow!("invalid 'schedule_at' value: {error}"))?;
        let millis = timestamp
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| anyhow::anyhow!("invalid 'schedule_at' value: {error}"))?
            .as_millis() as u64;
        return Ok(Some(millis));
    }

    if let Some(delay_seconds) = args.get("delay_seconds").and_then(|value| value.as_u64()) {
        return Ok(Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|error| anyhow::anyhow!("system clock error: {error}"))?
                .as_millis() as u64
                + delay_seconds.saturating_mul(1000),
        ));
    }

    Ok(None)
}

async fn execute_type_in_terminal(
    args: &serde_json::Value,
    session_manager: &Arc<SessionManager>,
) -> Result<String> {
    let sessions = session_manager.list().await;
    if sessions.is_empty() {
        return Err(anyhow::anyhow!("No active terminal sessions to type into"));
    }

    let target_id = if let Some(pane) = args.get("pane").and_then(|v| v.as_str()) {
        sessions
            .iter()
            .find(|s| s.id.to_string().contains(pane))
            .map(|s| s.id)
    } else {
        sessions.first().map(|s| s.id)
    };

    let sid = target_id.ok_or_else(|| anyhow::anyhow!("Target session not found"))?;

    // Check if sending a special key
    let description;
    let input: Vec<u8> = if let Some(key) = args.get("key").and_then(|v| v.as_str()) {
        description = format!("key:{key}");
        resolve_key_sequence(key)
    } else {
        let text = args.get("text").and_then(|v| v.as_str()).unwrap_or("");
        let press_enter = args
            .get("press_enter")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        description = if press_enter {
            format!("{text} + Enter")
        } else {
            text.to_string()
        };

        // Send text first
        if !text.is_empty() {
            session_manager.write_input(sid, text.as_bytes()).await?;
        }
        if press_enter {
            // Small delay so the TUI processes the text before Enter
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            session_manager.write_input(sid, b"\r").await?;
        }

        // Signal that we already sent — skip the write_input below
        Vec::new()
    };

    if !input.is_empty() {
        session_manager.write_input(sid, &input).await?;
    }

    // Wait for the terminal to process input
    tokio::time::sleep(std::time::Duration::from_millis(800)).await;

    // Read back terminal content to see the result
    match session_manager.get_scrollback(sid, None).await {
        Ok(data) => {
            let stripped = strip_ansi_escapes::strip(&data);
            let text_out = String::from_utf8_lossy(&stripped);
            let lines: Vec<&str> = text_out.lines().collect();
            let start = if lines.len() > 30 {
                lines.len() - 30
            } else {
                0
            };
            let visible: Vec<&str> = lines[start..]
                .iter()
                .filter(|l| !l.trim().is_empty())
                .copied()
                .collect();

            Ok(format!(
                "Sent '{description}' to session {sid}\n\nTerminal output (last 30 lines):\n{}",
                visible.join("\n"),
            ))
        }
        Err(_) => Ok(format!("Sent '{description}' to session {sid}")),
    }
}

/// Resolve a key name to its terminal escape sequence bytes.
fn resolve_key_sequence(key: &str) -> Vec<u8> {
    match key.to_lowercase().as_str() {
        "enter" | "return" => vec![b'\r'],
        "ctrl+c" => vec![0x03],
        "ctrl+d" => vec![0x04],
        "ctrl+z" => vec![0x1a],
        "ctrl+l" => vec![0x0c],
        "ctrl+a" => vec![0x01],
        "ctrl+e" => vec![0x05],
        "ctrl+u" => vec![0x15],
        "ctrl+k" => vec![0x0b],
        "ctrl+w" => vec![0x17],
        "ctrl+r" => vec![0x12],
        "ctrl+p" => vec![0x10],
        "ctrl+n" => vec![0x0e],
        "escape" | "esc" => vec![0x1b],
        "tab" => vec![b'\t'],
        "backspace" => vec![0x7f],
        "delete" => vec![0x1b, b'[', b'3', b'~'],
        "up" => vec![0x1b, b'[', b'A'],
        "down" => vec![0x1b, b'[', b'B'],
        "right" => vec![0x1b, b'[', b'C'],
        "left" => vec![0x1b, b'[', b'D'],
        "home" => vec![0x1b, b'[', b'H'],
        "end" => vec![0x1b, b'[', b'F'],
        "page_up" => vec![0x1b, b'[', b'5', b'~'],
        "page_down" => vec![0x1b, b'[', b'6', b'~'],
        // F-keys
        "f1" => vec![0x1b, b'O', b'P'],
        "f2" => vec![0x1b, b'O', b'Q'],
        "f3" => vec![0x1b, b'O', b'R'],
        "f4" => vec![0x1b, b'O', b'S'],
        // Default: send as raw text
        other => other.as_bytes().to_vec(),
    }
}
