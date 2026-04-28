use super::*;

pub(super) const WHATSAPP_LINK_TIMEOUT_SECS: u64 = 120;

pub(super) fn parse_whatsapp_setup_allowlist(raw: &str) -> Option<Vec<String>> {
    let parsed = parse_whatsapp_allowed_contacts(raw);
    if parsed.is_empty() {
        None
    } else {
        Some(parsed)
    }
}

pub(super) fn resolve_whatsapp_allowlist_prompt(
    outcome: WhatsAppAllowlistPromptOutcome,
) -> WhatsAppAllowlistPromptResolution<'static> {
    match outcome {
        WhatsAppAllowlistPromptOutcome::Submitted(raw)
            if parse_whatsapp_setup_allowlist(&raw).is_some() =>
        {
            WhatsAppAllowlistPromptResolution::Accept(raw)
        }
        WhatsAppAllowlistPromptOutcome::Submitted(_) => WhatsAppAllowlistPromptResolution::Retry(
            "Enter at least one valid WhatsApp phone number before linking.",
        ),
        WhatsAppAllowlistPromptOutcome::Cancelled | WhatsAppAllowlistPromptOutcome::EndOfInput => {
            WhatsAppAllowlistPromptResolution::Cancel
        }
    }
}

pub(super) fn whatsapp_gateway_config_writes(raw_allowlist: &str) -> Result<Vec<ConfigWrite>> {
    let parsed_allowlist = parse_whatsapp_setup_allowlist(raw_allowlist).ok_or_else(|| {
        anyhow::anyhow!("Enter at least one valid WhatsApp phone number before linking.")
    })?;
    let normalized_allowlist = parsed_allowlist.join(",");

    Ok(vec![
        ConfigWrite {
            key_path: "/gateway/whatsapp_allowed_contacts".to_string(),
            value_json: serde_json::to_string(&normalized_allowlist)
                .context("Failed to encode WhatsApp allowlist")?,
        },
        ConfigWrite {
            key_path: "/gateway/enabled".to_string(),
            value_json: "true".to_string(),
        },
    ])
}

pub(super) fn collect_whatsapp_allowlist_input() -> Result<WhatsAppAllowlistPromptOutcome> {
    let mut stdout = io::stdout();
    let mut lines = Vec::new();

    println!(
        "Before linking WhatsApp, zorai requires an allowlist to avoid replying in every chat."
    );
    println!("Enter allowed phone numbers now before QR linking starts.");
    println!("You can paste comma-separated values or enter one contact per line.");
    println!("Press Enter on an empty line when finished, or type /back to cancel.");

    loop {
        if lines.is_empty() {
            write!(stdout, "Allowed contacts: ")?;
        } else {
            write!(stdout, "> ")?;
        }
        stdout.flush()?;

        let mut line = String::new();
        let bytes_read = io::stdin()
            .read_line(&mut line)
            .context("Failed to read WhatsApp allowlist input")?;

        if bytes_read == 0 {
            return Ok(WhatsAppAllowlistPromptOutcome::EndOfInput);
        }

        let line = line.trim_end_matches(['\r', '\n']);
        if line == "/back" {
            return Ok(WhatsAppAllowlistPromptOutcome::Cancelled);
        }
        if line.is_empty() {
            break;
        }
        lines.push(line.to_string());
    }

    Ok(WhatsAppAllowlistPromptOutcome::Submitted(lines.join("\n")))
}

pub(super) fn gateway_choice_items() -> [(&'static str, &'static str); 5] {
    [
        ("Slack", "slack"),
        ("Discord", "discord"),
        ("Telegram", "telegram"),
        ("WhatsApp", "whatsapp"),
        ("Skip", ""),
    ]
}

pub(super) fn whatsapp_timeout_choices() -> [(&'static str, &'static str); 2] {
    [
        ("Retry WhatsApp linking", ""),
        ("Skip for now", "continue setup"),
    ]
}

pub(super) fn whatsapp_timeout_retry_selected(index: usize) -> bool {
    index == 0
}

pub(super) fn format_raw_mode_line_output(text: &str) -> String {
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut output = normalized.replace('\n', "\r\n");
    output.push_str("\r\n");
    output
}

fn write_raw_mode_line(stdout: &mut impl Write, text: &str) -> Result<()> {
    stdout
        .write_all(format_raw_mode_line_output(text).as_bytes())
        .context("Failed to write raw-mode terminal output")?;
    stdout
        .flush()
        .context("Failed to flush raw-mode terminal output")
}

pub(super) fn poll_for_setup_cancel_key() -> Result<bool> {
    if event::poll(std::time::Duration::from_millis(0)).context("Failed to poll keyboard input")? {
        if let Event::Key(KeyEvent {
            code,
            modifiers,
            kind,
            ..
        }) = event::read().context("Failed to read keyboard input")?
        {
            if !is_actionable_key_event_kind(kind) {
                return Ok(false);
            }
            match code {
                KeyCode::Esc => return Ok(true),
                KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                    anyhow::bail!("Setup cancelled by user");
                }
                _ => {}
            }
        }
    }
    Ok(false)
}

pub(super) async fn whatsapp_link_unsubscribe(
    framed: &mut Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, ZoraiCodec>,
) -> Result<()> {
    wizard_send(framed, ClientMessage::AgentWhatsAppLinkUnsubscribe)
        .await
        .context("Failed to unsubscribe from WhatsApp link updates")
}

pub(super) async fn whatsapp_link_stop_and_unsubscribe(
    framed: &mut Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, ZoraiCodec>,
) -> Result<()> {
    wizard_send(framed, ClientMessage::AgentWhatsAppLinkStop)
        .await
        .context("Failed to stop WhatsApp link workflow")?;
    wizard_send(framed, ClientMessage::AgentWhatsAppLinkUnsubscribe)
        .await
        .context("Failed to unsubscribe from WhatsApp link updates")
}

pub(super) async fn run_whatsapp_link_attempt(
    framed: &mut Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, ZoraiCodec>,
) -> Result<WhatsAppLinkAttemptOutcome> {
    println!();
    println!("{}", "Starting WhatsApp linking...".bold());
    println!("Scan the QR code when it appears. Press Esc to skip.");

    wizard_send(framed, ClientMessage::AgentWhatsAppLinkSubscribe).await?;
    wizard_send(framed, ClientMessage::AgentWhatsAppLinkStart).await?;

    let _raw_mode = RawModeGuard::new()?;
    let deadline =
        tokio::time::Instant::now() + std::time::Duration::from_secs(WHATSAPP_LINK_TIMEOUT_SECS);
    let mut last_qr: Option<String> = None;
    let mut last_status: Option<String> = None;
    let mut stdout = io::stdout();

    loop {
        if poll_for_setup_cancel_key()? {
            whatsapp_link_stop_and_unsubscribe(framed).await?;
            write_raw_mode_line(&mut stdout, "")?;
            write_raw_mode_line(&mut stdout, "Skipped WhatsApp linking.")?;
            return Ok(WhatsAppLinkAttemptOutcome::CancelledByUser);
        }

        let now = tokio::time::Instant::now();
        if now >= deadline {
            whatsapp_link_stop_and_unsubscribe(framed).await?;
            return Ok(WhatsAppLinkAttemptOutcome::TimedOut);
        }

        let wait = deadline
            .saturating_duration_since(now)
            .min(std::time::Duration::from_millis(500));
        let message = match tokio::time::timeout(wait, wizard_recv(framed)).await {
            Ok(result) => result?,
            Err(_) => continue,
        };

        match message {
            DaemonMessage::AgentWhatsAppLinkQr {
                ascii_qr,
                expires_at_ms,
            } => {
                if last_qr.as_deref() != Some(ascii_qr.as_str()) {
                    write_raw_mode_line(&mut stdout, "")?;
                    write_raw_mode_line(&mut stdout, &format!("{}", "WhatsApp QR:".bold()))?;
                    write_raw_mode_line(&mut stdout, &ascii_qr)?;
                    if let Some(expires_at) = expires_at_ms {
                        write_raw_mode_line(
                            &mut stdout,
                            &format!("QR update expires at {expires_at} ms epoch."),
                        )?;
                    }
                    last_qr = Some(ascii_qr);
                }
            }
            DaemonMessage::AgentWhatsAppLinked { phone } => {
                whatsapp_link_unsubscribe(framed).await?;
                write_raw_mode_line(
                    &mut stdout,
                    &format!("WhatsApp linked: {}", phone.as_deref().unwrap_or("device")),
                )?;
                return Ok(WhatsAppLinkAttemptOutcome::Linked(phone));
            }
            DaemonMessage::AgentWhatsAppLinkStatus {
                state,
                phone,
                last_error,
            } => {
                if state == "connected" {
                    whatsapp_link_unsubscribe(framed).await?;
                    write_raw_mode_line(
                        &mut stdout,
                        &format!("WhatsApp linked: {}", phone.as_deref().unwrap_or("device")),
                    )?;
                    return Ok(WhatsAppLinkAttemptOutcome::Linked(phone));
                }
                if last_status.as_deref() != Some(state.as_str()) {
                    match state.as_str() {
                        "starting" => {
                            write_raw_mode_line(&mut stdout, "Preparing WhatsApp link session...")?
                        }
                        "qr_ready" | "awaiting_qr" => write_raw_mode_line(
                            &mut stdout,
                            "QR is ready. Scan it in WhatsApp on your phone.",
                        )?,
                        "error" => write_raw_mode_line(
                            &mut stdout,
                            &format!(
                                "WhatsApp link error: {}",
                                last_error.as_deref().unwrap_or("unknown")
                            ),
                        )?,
                        "disconnected" => write_raw_mode_line(
                            &mut stdout,
                            &format!(
                                "WhatsApp link disconnected: {}",
                                last_error.as_deref().unwrap_or("none")
                            ),
                        )?,
                        _ => write_raw_mode_line(
                            &mut stdout,
                            &format!("WhatsApp link status: {state}"),
                        )?,
                    }
                    last_status = Some(state);
                }
            }
            DaemonMessage::AgentWhatsAppLinkError { message, .. } => {
                write_raw_mode_line(&mut stdout, &format!("WhatsApp link error: {message}"))?;
            }
            DaemonMessage::AgentWhatsAppLinkDisconnected { reason } => {
                write_raw_mode_line(
                    &mut stdout,
                    &format!(
                        "WhatsApp link disconnected: {}",
                        reason.as_deref().unwrap_or("none")
                    ),
                )?;
            }
            DaemonMessage::Error { message } => {
                write_raw_mode_line(&mut stdout, &format!("WhatsApp link error: {message}"))?
            }
            DaemonMessage::GatewayBootstrap { .. }
            | DaemonMessage::GatewaySendRequest { .. }
            | DaemonMessage::GatewayReloadCommand { .. }
            | DaemonMessage::GatewayShutdownCommand { .. } => {}
            _ => {}
        }
    }
}

pub(super) async fn run_whatsapp_link_subflow(
    framed: &mut Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, ZoraiCodec>,
) -> Result<bool> {
    loop {
        match run_whatsapp_link_attempt(framed).await? {
            WhatsAppLinkAttemptOutcome::Linked(_) => return Ok(true),
            WhatsAppLinkAttemptOutcome::CancelledByUser => return Ok(false),
            WhatsAppLinkAttemptOutcome::TimedOut => {
                println!();
                let timeout_items = whatsapp_timeout_choices();
                let choice = select_list(
                    "WhatsApp linking timed out. What would you like to do?",
                    &timeout_items,
                    false,
                    0,
                )?
                .expect("timeout choice is required");
                if !whatsapp_timeout_retry_selected(choice) {
                    println!("Skipped WhatsApp linking for now.");
                    return Ok(false);
                }
            }
        }
    }
}
