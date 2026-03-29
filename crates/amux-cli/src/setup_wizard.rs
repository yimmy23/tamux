//! First-run setup wizard for tamux.
//!
//! Connects to the daemon via IPC socket and configures the agent through
//! protocol messages. All config writes go through daemon IPC -- config.json
//! is never written or referenced as a daemon config source.
//!
//! Navigation uses crossterm arrow-key selection (not number input).
//! Provider list is queried from the daemon at runtime (no hardcoded list).

use amux_protocol::{
    AmuxCodec, ClientMessage, DaemonMessage, parse_whatsapp_allowed_contacts,
};
use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::style::{self, Stylize};
use crossterm::terminal;
use futures::{SinkExt, StreamExt};
use std::io::{self, Write};
use tokio_util::codec::Framed;

// ---------------------------------------------------------------------------
// Local mirror of ProviderAuthState (daemon-side struct)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Deserialize)]
struct ProviderAuthState {
    provider_id: String,
    provider_name: String,
    #[allow(dead_code)]
    authenticated: bool,
    auth_source: String,
    model: String,
    base_url: String,
}

// ---------------------------------------------------------------------------
// IPC connection helpers (private to wizard)
// ---------------------------------------------------------------------------

#[cfg(unix)]
async fn wizard_connect(
) -> Result<Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, AmuxCodec>> {
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
    let path = std::path::PathBuf::from(runtime_dir).join("tamux-daemon.sock");
    let stream = tokio::net::UnixStream::connect(&path)
        .await
        .with_context(|| format!("cannot connect to daemon at {}", path.display()))?;
    Ok(Framed::new(stream, AmuxCodec))
}

#[cfg(windows)]
async fn wizard_connect(
) -> Result<Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, AmuxCodec>> {
    let addr = amux_protocol::default_tcp_addr();
    let stream = tokio::net::TcpStream::connect(&addr)
        .await
        .with_context(|| format!("cannot connect to daemon on {addr}"))?;
    Ok(Framed::new(stream, AmuxCodec))
}

async fn wizard_send(
    framed: &mut Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, AmuxCodec>,
    msg: ClientMessage,
) -> Result<()> {
    framed.send(msg).await.map_err(Into::into)
}

async fn wizard_recv(
    framed: &mut Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, AmuxCodec>,
) -> Result<DaemonMessage> {
    framed
        .next()
        .await
        .ok_or_else(|| anyhow::anyhow!("daemon closed connection"))?
        .map_err(Into::into)
}

// ---------------------------------------------------------------------------
// Validate provider using a fresh IPC connection (avoids state issues on wizard stream)
// ---------------------------------------------------------------------------

/// Validate provider via daemon IPC AgentValidateProvider on the given framed connection.
/// Loops to skip any interleaved DaemonMessage::Error (from prior fire-and-forget config sets).
async fn validate_provider_on_stream(
    framed: &mut Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, AmuxCodec>,
    provider_id: &str,
    auth_source: &str,
) -> Result<bool> {
    wizard_send(
        framed,
        ClientMessage::AgentValidateProvider {
            provider_id: provider_id.to_string(),
            base_url: String::new(),
            api_key: String::new(),
            auth_source: auth_source.to_string(),
        },
    )
    .await?;

    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        match tokio::time::timeout_at(deadline, wizard_recv(framed)).await {
            Ok(Ok(DaemonMessage::AgentProviderValidation { valid, error, .. })) => {
                if !valid {
                    if let Some(err) = error {
                        println!("Validation error: {err}");
                    }
                }
                return Ok(valid);
            }
            Ok(Ok(DaemonMessage::Error { message })) => {
                // Skip errors from prior fire-and-forget config sets
                tracing::debug!("skipping daemon error during validate: {message}");
                continue;
            }
            Ok(Ok(
                DaemonMessage::GatewayBootstrap { .. }
                | DaemonMessage::GatewaySendRequest { .. }
                | DaemonMessage::GatewayReloadCommand { .. }
                | DaemonMessage::GatewayShutdownCommand { .. },
            )) => continue,
            Ok(Ok(DaemonMessage::AgentEvent { .. })) => continue,
            Ok(Ok(DaemonMessage::AgentConfigResponse { .. })) => continue,
            Ok(Ok(other)) => {
                tracing::debug!("unexpected message during validate: {other:?}");
                continue;
            }
            Ok(Err(e)) => anyhow::bail!("Connection error: {e}"),
            Err(_) => anyhow::bail!("Timed out (30s)"),
        }
    }
}

// ---------------------------------------------------------------------------
// Read a config value from daemon (fresh connection to avoid stream state issues)
// ---------------------------------------------------------------------------

async fn read_config_key(key: &str) -> Option<String> {
    let mut conn = wizard_connect().await.ok()?;
    wizard_send(&mut conn, ClientMessage::AgentGetConfig)
        .await
        .ok()?;
    match wizard_recv(&mut conn).await.ok()? {
        DaemonMessage::AgentConfigResponse { config_json } => {
            let val: serde_json::Value = serde_json::from_str(&config_json).ok()?;
            // Support dot notation: "gateway.slack_token" -> val["gateway"]["slack_token"]
            let mut current = &val;
            for part in key.split('.') {
                current = current.get(part)?;
            }
            match current {
                serde_json::Value::String(s) if !s.is_empty() => Some(s.clone()),
                serde_json::Value::Bool(b) => Some(b.to_string()),
                serde_json::Value::Number(n) => Some(n.to_string()),
                _ => None,
            }
        }
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// Ensure daemon is running (Step 0 per D-02)
// ---------------------------------------------------------------------------

async fn ensure_daemon_running() -> Result<()> {
    // Try to connect first
    if wizard_connect().await.is_ok() {
        return Ok(());
    }

    // Daemon not reachable -- try to start it
    println!("Starting daemon...");
    let mut cmd = std::process::Command::new("tamux-daemon");
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }

    if let Err(e) = cmd.spawn() {
        anyhow::bail!("Could not start daemon: {e}\nPlease start it manually with: tamux-daemon");
    }

    // Poll for daemon socket up to 5 seconds
    for _ in 0..10 {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        if wizard_connect().await.is_ok() {
            println!("Daemon started.");
            return Ok(());
        }
    }

    anyhow::bail!(
        "Daemon did not become reachable within 5 seconds.\n\
         Please start it manually with: tamux-daemon"
    )
}

// ---------------------------------------------------------------------------
// Crossterm arrow-key select list (per D-04)
// ---------------------------------------------------------------------------

/// Interactive select list with arrow-key navigation.
/// Returns `Some(index)` on Enter, `None` on Esc (only if `allow_esc` is true).
/// `default_index` sets the initially highlighted item.
fn select_list(
    title: &str,
    items: &[(&str, &str)],
    allow_esc: bool,
    default_index: usize,
) -> Result<Option<usize>> {
    use crossterm::{cursor, execute, queue};

    let mut stdout = io::stdout();
    let mut selected: usize = default_index.min(items.len().saturating_sub(1));

    terminal::enable_raw_mode().context("Failed to enable raw mode")?;

    // Helper to clean up raw mode on any exit path
    let result = (|| -> Result<Option<usize>> {
        loop {
            // Clear from cursor down and render using execute!/queue! for Windows compat
            queue!(
                stdout,
                style::SetForegroundColor(style::Color::White),
                style::SetAttribute(style::Attribute::Bold),
                style::Print(title),
                style::SetAttribute(style::Attribute::Reset),
                style::SetForegroundColor(style::Color::Reset),
                style::Print("\r\n\r\n"),
            )?;

            for (i, (label, desc)) in items.iter().enumerate() {
                if i == selected {
                    let mut line = format!("  > {label}");
                    if !desc.is_empty() {
                        line.push_str(&format!(" ({desc})"));
                    }
                    queue!(
                        stdout,
                        style::SetForegroundColor(style::Color::Green),
                        style::SetAttribute(style::Attribute::Bold),
                        style::Print(&line),
                        style::SetAttribute(style::Attribute::Reset),
                        style::SetForegroundColor(style::Color::Reset),
                        style::Print("\r\n"),
                    )?;
                } else {
                    let mut line = format!("    {label}");
                    if !desc.is_empty() {
                        line.push_str(&format!(" ({desc})"));
                    }
                    queue!(
                        stdout,
                        style::SetForegroundColor(style::Color::Grey),
                        style::Print(&line),
                        style::SetForegroundColor(style::Color::Reset),
                        style::Print("\r\n"),
                    )?;
                }
            }

            stdout.flush()?;

            // Read key (blocking — runs on tokio blocking thread via spawn_blocking caller)
            if let Event::Key(KeyEvent {
                code, modifiers, ..
            }) = event::read()?
            {
                match code {
                    KeyCode::Up => {
                        if selected == 0 {
                            selected = items.len().saturating_sub(1);
                        } else {
                            selected -= 1;
                        }
                    }
                    KeyCode::Down => {
                        selected += 1;
                        if selected >= items.len() {
                            selected = 0;
                        }
                    }
                    KeyCode::Enter => {
                        // Print final selection and exit
                        execute!(stdout, style::SetForegroundColor(style::Color::Reset),)?;
                        return Ok(Some(selected));
                    }
                    KeyCode::Esc if allow_esc => {
                        execute!(stdout, style::SetForegroundColor(style::Color::Reset),)?;
                        return Ok(None);
                    }
                    KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                        anyhow::bail!("Setup cancelled by user");
                    }
                    _ => {}
                }
            }

            // Move cursor up to redraw (title + blank line + items) using execute! for Windows
            let lines_to_clear = items.len() + 2;
            execute!(
                stdout,
                cursor::MoveUp(lines_to_clear as u16),
                terminal::Clear(terminal::ClearType::FromCursorDown),
            )?;
        }
    })();

    terminal::disable_raw_mode().context("Failed to disable raw mode")?;

    result
}

// ---------------------------------------------------------------------------
// Crossterm text input (for API key etc.)
// ---------------------------------------------------------------------------

/// Interactive text input with optional masking.
/// Returns `Some(text)` on Enter, `None` on Esc.
fn text_input(prompt_text: &str, default: &str, masked: bool) -> Result<Option<String>> {
    use crossterm::execute;

    let mut stdout = io::stdout();
    if !default.is_empty() {
        execute!(stdout, style::Print(format!("{prompt_text} [{default}]: ")))?;
    } else {
        execute!(stdout, style::Print(format!("{prompt_text}: ")))?;
    }

    terminal::enable_raw_mode().context("Failed to enable raw mode for input")?;

    let result = (|| -> Result<Option<String>> {
        let mut input = String::new();
        loop {
            if let Event::Key(KeyEvent {
                code, modifiers, ..
            }) = event::read()?
            {
                match code {
                    KeyCode::Enter => {
                        execute!(stdout, style::Print("\r\n"))?;
                        let value = if input.is_empty() && !default.is_empty() {
                            default.to_string()
                        } else {
                            input
                        };
                        return Ok(Some(value));
                    }
                    KeyCode::Esc => {
                        execute!(stdout, style::Print("\r\n"))?;
                        return Ok(None);
                    }
                    KeyCode::Backspace => {
                        if input.pop().is_some() {
                            execute!(stdout, style::Print("\x08 \x08"))?;
                        }
                    }
                    KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => {
                        anyhow::bail!("Setup cancelled by user");
                    }
                    KeyCode::Char(c) => {
                        input.push(c);
                        if masked {
                            execute!(stdout, style::Print("*"))?;
                        } else {
                            execute!(stdout, style::Print(format!("{c}")))?;
                        }
                    }
                    _ => {}
                }
            }
        }
    })();

    terminal::disable_raw_mode().context("Failed to disable raw mode")?;

    result
}

fn parse_whatsapp_setup_allowlist(raw: &str) -> Option<Vec<String>> {
    let parsed = parse_whatsapp_allowed_contacts(raw);
    if parsed.is_empty() {
        None
    } else {
        Some(parsed)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum WhatsAppAllowlistPromptOutcome {
    Submitted(String),
    Cancelled,
    EndOfInput,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum WhatsAppAllowlistPromptResolution<'a> {
    Accept(String),
    Retry(&'a str),
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ConfigWrite {
    key_path: String,
    value_json: String,
}

fn resolve_whatsapp_allowlist_prompt(
    outcome: WhatsAppAllowlistPromptOutcome,
) -> WhatsAppAllowlistPromptResolution<'static> {
    match outcome {
        WhatsAppAllowlistPromptOutcome::Submitted(raw)
            if parse_whatsapp_setup_allowlist(&raw).is_some() =>
        {
            WhatsAppAllowlistPromptResolution::Accept(raw)
        }
        WhatsAppAllowlistPromptOutcome::Submitted(_) => {
            WhatsAppAllowlistPromptResolution::Retry(
                "Enter at least one valid WhatsApp phone number before linking.",
            )
        }
        WhatsAppAllowlistPromptOutcome::Cancelled | WhatsAppAllowlistPromptOutcome::EndOfInput => {
            WhatsAppAllowlistPromptResolution::Cancel
        }
    }
}

fn whatsapp_gateway_config_writes(raw_allowlist: &str) -> Result<Vec<ConfigWrite>> {
    parse_whatsapp_setup_allowlist(raw_allowlist)
        .ok_or_else(|| anyhow::anyhow!("Enter at least one valid WhatsApp phone number before linking."))?;

    Ok(vec![
        ConfigWrite {
            key_path: "/gateway/whatsapp_allowed_contacts".to_string(),
            value_json: serde_json::to_string(raw_allowlist)
                .context("Failed to encode WhatsApp allowlist")?,
        },
        ConfigWrite {
            key_path: "/gateway/enabled".to_string(),
            value_json: "true".to_string(),
        },
    ])
}

fn collect_whatsapp_allowlist_input() -> Result<WhatsAppAllowlistPromptOutcome> {
    let mut stdout = io::stdout();
    let mut lines = Vec::new();

    println!(
        "Before linking WhatsApp, tamux requires an allowlist to avoid replying in every chat."
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

const WHATSAPP_LINK_TIMEOUT_SECS: u64 = 120;

#[derive(Debug, Clone, PartialEq, Eq)]
enum WhatsAppLinkAttemptOutcome {
    Linked(Option<String>),
    TimedOut,
    CancelledByUser,
}

struct RawModeGuard;

impl RawModeGuard {
    fn new() -> Result<Self> {
        terminal::enable_raw_mode().context("Failed to enable raw mode")?;
        Ok(Self)
    }
}

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
    }
}

fn gateway_choice_items() -> [(&'static str, &'static str); 5] {
    [
        ("Slack", "slack"),
        ("Discord", "discord"),
        ("Telegram", "telegram"),
        ("WhatsApp", "whatsapp"),
        ("Skip", ""),
    ]
}

fn whatsapp_timeout_choices() -> [(&'static str, &'static str); 2] {
    [
        ("Retry WhatsApp linking", ""),
        ("Skip for now", "continue setup"),
    ]
}

fn whatsapp_timeout_retry_selected(index: usize) -> bool {
    index == 0
}

fn poll_for_setup_cancel_key() -> Result<bool> {
    if event::poll(std::time::Duration::from_millis(0)).context("Failed to poll keyboard input")? {
        if let Event::Key(KeyEvent {
            code, modifiers, ..
        }) = event::read().context("Failed to read keyboard input")?
        {
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

async fn whatsapp_link_unsubscribe(
    framed: &mut Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, AmuxCodec>,
) -> Result<()> {
    wizard_send(framed, ClientMessage::AgentWhatsAppLinkUnsubscribe)
        .await
        .context("Failed to unsubscribe from WhatsApp link updates")
}

async fn whatsapp_link_stop_and_unsubscribe(
    framed: &mut Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, AmuxCodec>,
) -> Result<()> {
    wizard_send(framed, ClientMessage::AgentWhatsAppLinkStop)
        .await
        .context("Failed to stop WhatsApp link workflow")?;
    wizard_send(framed, ClientMessage::AgentWhatsAppLinkUnsubscribe)
        .await
        .context("Failed to unsubscribe from WhatsApp link updates")
}

async fn run_whatsapp_link_attempt(
    framed: &mut Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, AmuxCodec>,
) -> Result<WhatsAppLinkAttemptOutcome> {
    println!();
    println!("{}", "Starting WhatsApp linking...".bold());
    println!("Scan the QR code when it appears. Press Esc to skip.");

    wizard_send(framed, ClientMessage::AgentWhatsAppLinkSubscribe)
        .await
        .context("Failed to subscribe to WhatsApp link updates")?;
    wizard_send(framed, ClientMessage::AgentWhatsAppLinkStart)
        .await
        .context("Failed to start WhatsApp link workflow")?;

    let _raw_mode = RawModeGuard::new()?;
    let deadline =
        tokio::time::Instant::now() + std::time::Duration::from_secs(WHATSAPP_LINK_TIMEOUT_SECS);
    let mut last_qr: Option<String> = None;
    let mut last_status: Option<String> = None;

    loop {
        if poll_for_setup_cancel_key()? {
            whatsapp_link_stop_and_unsubscribe(framed).await?;
            println!();
            println!("Skipped WhatsApp linking.");
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
                    println!();
                    println!("{}", "WhatsApp QR:".bold());
                    println!("{ascii_qr}");
                    if let Some(expires_at) = expires_at_ms {
                        println!("QR update expires at {expires_at} ms epoch.");
                    }
                    last_qr = Some(ascii_qr);
                }
            }
            DaemonMessage::AgentWhatsAppLinked { phone } => {
                whatsapp_link_unsubscribe(framed).await?;
                println!("WhatsApp linked: {}", phone.as_deref().unwrap_or("device"));
                return Ok(WhatsAppLinkAttemptOutcome::Linked(phone));
            }
            DaemonMessage::AgentWhatsAppLinkStatus {
                state,
                phone,
                last_error,
            } => {
                if state == "connected" {
                    whatsapp_link_unsubscribe(framed).await?;
                    println!("WhatsApp linked: {}", phone.as_deref().unwrap_or("device"));
                    return Ok(WhatsAppLinkAttemptOutcome::Linked(phone));
                }
                if last_status.as_deref() != Some(state.as_str()) {
                    match state.as_str() {
                        "starting" => println!("Preparing WhatsApp link session..."),
                        "qr_ready" | "awaiting_qr" => {
                            println!("QR is ready. Scan it in WhatsApp on your phone.")
                        }
                        "error" => println!(
                            "WhatsApp link error: {}",
                            last_error.as_deref().unwrap_or("unknown")
                        ),
                        "disconnected" => println!(
                            "WhatsApp link disconnected: {}",
                            last_error.as_deref().unwrap_or("none")
                        ),
                        _ => println!("WhatsApp link status: {state}"),
                    }
                    last_status = Some(state);
                }
            }
            DaemonMessage::AgentWhatsAppLinkError { message, .. } => {
                println!("WhatsApp link error: {message}");
            }
            DaemonMessage::AgentWhatsAppLinkDisconnected { reason } => {
                println!(
                    "WhatsApp link disconnected: {}",
                    reason.as_deref().unwrap_or("none")
                );
            }
            DaemonMessage::Error { message } => {
                println!("WhatsApp link error: {message}");
            }
            DaemonMessage::GatewayBootstrap { .. }
            | DaemonMessage::GatewaySendRequest { .. }
            | DaemonMessage::GatewayReloadCommand { .. }
            | DaemonMessage::GatewayShutdownCommand { .. } => {}
            _ => {}
        }
    }
}

async fn run_whatsapp_link_subflow(
    framed: &mut Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, AmuxCodec>,
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

// ---------------------------------------------------------------------------
// Helper: is local provider (no API key required)
// ---------------------------------------------------------------------------

/// Returns true if a provider is local (no API key required).
fn is_local_provider(id: &str) -> bool {
    matches!(id, "ollama" | "lmstudio")
}

// ---------------------------------------------------------------------------
// Tier-gated helper functions (pure, testable)
// ---------------------------------------------------------------------------

/// Returns the default security level selection index for a given tier.
/// - newcomer  -> 0 (highest: approve risky actions)
/// - familiar  -> 1 (moderate: approve risky actions)
/// - power_user -> 2 (lowest: approve destructive only)
/// - expert    -> 2 (lowest: approve destructive only)
fn default_security_index(tier: &str) -> usize {
    match tier {
        "newcomer" => 0,
        "familiar" => 1,
        "power_user" | "expert" => 2,
        _ => 1, // safe default
    }
}

/// Returns whether a given tier should see a specific optional step.
/// Steps: "model", "web_search", "gateway", "data_dir"
/// Whether a first-time setup step auto-shows the advanced picker for this tier.
/// Only used for model selection on first-time setup (Familiar+ get the picker,
/// newcomers get the default silently). Web search and gateway are shown to all tiers.
fn tier_shows_step(tier: &str, step: &str) -> bool {
    match step {
        "model" | "data_dir" => matches!(tier, "familiar" | "power_user" | "expert"),
        _ => false,
    }
}

/// Maps a security level selection index to its kebab-case string and label.
fn security_level_from_index(index: usize) -> (&'static str, &'static str) {
    match index {
        0 => ("highest", "Approve risky actions"),
        1 => ("moderate", "Approve risky actions"),
        2 => ("lowest", "Approve destructive only"),
        3 => ("yolo", "Minimize interruptions"),
        _ => ("moderate", "Approve risky actions"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostSetupAction {
    LaunchTui,
    LaunchElectron,
    NotNow,
}

fn post_setup_choices() -> [(&'static str, &'static str); 3] {
    [
        ("TUI", "Terminal interface"),
        ("Electron", "Desktop app"),
        ("Not now", "Finish setup without launching"),
    ]
}

fn post_setup_action_from_index(index: usize) -> PostSetupAction {
    match index {
        0 => PostSetupAction::LaunchTui,
        1 => PostSetupAction::LaunchElectron,
        2 => PostSetupAction::NotNow,
        _ => PostSetupAction::NotNow,
    }
}

// ---------------------------------------------------------------------------
// Setup detection
// ---------------------------------------------------------------------------

/// Check whether setup is needed by querying daemon config via IPC.
/// Returns `true` if daemon is unreachable or has no provider set.
pub async fn needs_setup_via_ipc() -> bool {
    let mut framed = match wizard_connect().await {
        Ok(f) => f,
        Err(_) => return true, // Can't reach daemon = needs setup
    };
    if wizard_send(&mut framed, ClientMessage::AgentGetConfig)
        .await
        .is_err()
    {
        return true;
    }
    match wizard_recv(&mut framed).await {
        Ok(DaemonMessage::AgentConfigResponse { config_json }) => {
            let value: serde_json::Value = match serde_json::from_str(&config_json) {
                Ok(v) => v,
                Err(_) => return true,
            };
            match value.get("provider").and_then(|v| v.as_str()) {
                Some(s) if !s.is_empty() => false,
                _ => true,
            }
        }
        _ => true,
    }
}

// ---------------------------------------------------------------------------
// Main wizard entry point
// ---------------------------------------------------------------------------

/// Run the setup wizard. Connects to the daemon via IPC, queries provider list,
/// and configures the agent through IPC messages. Never writes config.json.
pub async fn run_setup_wizard() -> Result<PostSetupAction> {
    // Step 0: Ensure daemon is running
    ensure_daemon_running().await?;

    // Open a long-lived IPC connection for the wizard
    let mut framed = wizard_connect()
        .await
        .context("Failed to connect to daemon for setup")?;

    // Step 1: Welcome banner
    println!();
    println!("{}", "tamux -- The Agent That Lives".bold());
    println!("First-time setup");
    println!();

    // Step 2: Tier self-assessment (per D-06)
    let tier_items: Vec<(&str, &str)> = vec![
        ("Just getting started", "newcomer"),
        ("I've used chatbots and assistants", "familiar"),
        ("I run automations and scripting", "power_user"),
        ("I build agent systems", "expert"),
    ];

    let tier_idx = select_list(
        "How familiar are you with AI agents?",
        &tier_items,
        false,
        0,
    )?
    .expect("tier selection is required");

    let tier_string = tier_items[tier_idx].1.to_string();

    // Send tier override via IPC
    wizard_send(
        &mut framed,
        ClientMessage::AgentSetTierOverride {
            tier: Some(tier_string.clone()),
        },
    )
    .await
    .context("Failed to set tier override")?;

    // AgentSetTierOverride is fire-and-forget (no response expected per server.rs)
    // Brief pause to let daemon process it
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    println!();

    // Step 3: Provider selection (per D-07)
    // Query daemon for provider list
    wizard_send(&mut framed, ClientMessage::AgentGetProviderAuthStates)
        .await
        .context("Failed to query provider list")?;

    let providers: Vec<ProviderAuthState> = match wizard_recv(&mut framed).await? {
        DaemonMessage::AgentProviderAuthStates { states_json } => {
            serde_json::from_str(&states_json)
                .context("Failed to parse provider auth states from daemon")?
        }
        other => {
            anyhow::bail!("Unexpected daemon response: {other:?}");
        }
    };

    if providers.is_empty() {
        anyhow::bail!("Daemon returned empty provider list. Is the daemon running correctly?");
    }

    let provider_items: Vec<(&str, &str)> = providers
        .iter()
        .map(|p| (p.provider_name.as_str(), p.provider_id.as_str()))
        .collect();

    let provider_idx = select_list("Select your LLM provider:", &provider_items, false, 0)?
        .expect("provider selection is required");

    let selected_provider = &providers[provider_idx];
    let provider_id = selected_provider.provider_id.clone();
    let provider_name = selected_provider.provider_name.clone();
    let base_url = selected_provider.base_url.clone();
    let default_model = selected_provider.model.clone();
    let auth_source = selected_provider.auth_source.clone();

    println!();

    // Step 4: API key (per D-05)
    let mut api_key_saved = String::new();
    if is_local_provider(&provider_id) {
        println!("Local provider -- no API key needed.");
    } else if selected_provider.authenticated {
        // Provider already has a key — ask whether to replace
        println!("{} is already authenticated.", provider_name.clone().bold());
        let replace_idx = select_list(
            "Replace API key?",
            &[("No, keep existing key", ""), ("Yes, enter a new key", "")],
            false,
            0,
        )?
        .unwrap_or(0);
        if replace_idx == 0 {
            // Keep existing — skip to connectivity test
            println!("Keeping existing API key.");
            api_key_saved = "(existing)".to_string();
        } else {
            let api_key = text_input(&format!("Enter new API key for {provider_name}"), "", true)?
                .unwrap_or_default();
            if !api_key.is_empty() {
                api_key_saved = api_key.clone();
                for (key, val) in [
                    (
                        "provider",
                        serde_json::to_string(&provider_id).unwrap_or_default(),
                    ),
                    (
                        "api_key",
                        serde_json::to_string(&api_key).unwrap_or_default(),
                    ),
                ] {
                    wizard_send(
                        &mut framed,
                        ClientMessage::AgentSetConfigItem {
                            key_path: format!("/{key}"),
                            value_json: val,
                        },
                    )
                    .await
                    .with_context(|| format!("Failed to set {key}"))?;
                }
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                println!("API key updated.");
            }
        }
    } else {
        let api_key = text_input(&format!("Enter API key for {provider_name}"), "", true)?
            .unwrap_or_default();

        if api_key.is_empty() {
            println!("No API key entered. You can set it later with `tamux setup`.");
        } else {
            api_key_saved = api_key.clone();

            // Send provider config via AgentSetConfigItem (fire-and-forget — daemon
            // sends NO response on success, only DaemonMessage::Error on failure).
            for (key, val) in [
                (
                    "provider",
                    serde_json::to_string(&provider_id).unwrap_or_default(),
                ),
                (
                    "api_key",
                    serde_json::to_string(&api_key).unwrap_or_default(),
                ),
                (
                    "base_url",
                    serde_json::to_string(&base_url).unwrap_or_default(),
                ),
                (
                    "model",
                    serde_json::to_string(&default_model).unwrap_or_default(),
                ),
            ] {
                wizard_send(
                    &mut framed,
                    ClientMessage::AgentSetConfigItem {
                        key_path: format!("/{key}"),
                        value_json: val,
                    },
                )
                .await
                .with_context(|| format!("Failed to set {key}"))?;
            }
            // Brief pause to let daemon process all config items
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;

            println!("API key saved.");
        }
    }

    println!();

    // Set as active provider
    wizard_send(
        &mut framed,
        ClientMessage::AgentSetConfigItem {
            key_path: "/provider".to_string(),
            value_json: format!("\"{}\"", provider_id),
        },
    )
    .await
    .context("Failed to set active provider")?;

    // Track summary info for final message
    let mut summary_model: Option<String> = None;
    let mut summary_web_search: Option<String> = None;
    let mut summary_gateway: Option<String> = None;
    let mut summary_whatsapp_linked = false;

    // ----- Step 5: Model selection (all tiers) -----
    println!();

    // Check if a model is already configured
    let existing_model = read_config_key("model").await;
    let user_wants_replace = if let Some(ref model) = existing_model {
        println!("Model is already configured ({}).", model.clone().bold());
        match select_list(
            "Replace model?",
            &[
                ("No, keep existing model", ""),
                ("Yes, choose a new model", ""),
            ],
            false,
            0,
        )? {
            Some(1) => true,
            _ => {
                println!("Keeping existing model.");
                summary_model = Some(model.clone());
                false
            }
        }
    } else {
        false
    };

    // Show model picker when: user explicitly chose to replace, OR first-time setup for Familiar+ tier.
    let show_model_picker =
        user_wants_replace || (existing_model.is_none() && tier_shows_step(&tier_string, "model"));

    if show_model_picker {
        println!("Fetching available models...");

        wizard_send(
            &mut framed,
            ClientMessage::AgentFetchModels {
                provider_id: provider_id.clone(),
                base_url: base_url.clone(),
                api_key: api_key_saved.clone(),
            },
        )
        .await
        .context("Failed to fetch models")?;

        match wizard_recv(&mut framed).await? {
            DaemonMessage::AgentModelsResponse { models_json } => {
                let models: Vec<String> = serde_json::from_str(&models_json).unwrap_or_default();

                if !models.is_empty() {
                    let model_items: Vec<(&str, &str)> =
                        models.iter().map(|m| (m.as_str(), "")).collect();

                    match select_list("Select default model:", &model_items, true, 0)? {
                        Some(idx) => {
                            let chosen_model = &models[idx];
                            wizard_send(
                                &mut framed,
                                ClientMessage::AgentSetConfigItem {
                                    key_path: "/model".to_string(),
                                    value_json: format!("\"{}\"", chosen_model),
                                },
                            )
                            .await
                            .context("Failed to set model")?;
                            summary_model = Some(chosen_model.clone());
                        }
                        None => {
                            println!("Skipped -- using default model.");
                        }
                    }
                } else {
                    let fallback_default = if default_model.is_empty() {
                        ""
                    } else {
                        &default_model
                    };
                    match text_input("Enter model name (or Esc to skip)", fallback_default, false)?
                    {
                        Some(m) if !m.is_empty() => {
                            wizard_send(
                                &mut framed,
                                ClientMessage::AgentSetConfigItem {
                                    key_path: "/model".to_string(),
                                    value_json: format!("\"{}\"", m),
                                },
                            )
                            .await
                            .context("Failed to set model")?;
                            summary_model = Some(m);
                        }
                        _ => {
                            println!("Skipped -- using default model.");
                        }
                    }
                }
            }
            DaemonMessage::AgentError { message } => {
                println!("Could not fetch models: {message}");
                let fallback_default = if default_model.is_empty() {
                    ""
                } else {
                    &default_model
                };
                match text_input("Enter model name (or Esc to skip)", fallback_default, false)? {
                    Some(m) if !m.is_empty() => {
                        wizard_send(
                            &mut framed,
                            ClientMessage::AgentSetConfigItem {
                                key_path: "/model".to_string(),
                                value_json: format!("\"{}\"", m),
                            },
                        )
                        .await
                        .context("Failed to set model")?;
                        summary_model = Some(m);
                    }
                    _ => {
                        println!("Skipped -- using default model.");
                    }
                }
            }
            _ => {
                println!("Unexpected response fetching models.");
            }
        }

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    } else if existing_model.is_none() && !default_model.is_empty() {
        // First-time newcomer: set default model silently
        wizard_send(
            &mut framed,
            ClientMessage::AgentSetConfigItem {
                key_path: "/model".to_string(),
                value_json: format!("\"{}\"", default_model),
            },
        )
        .await
        .context("Failed to set default model")?;
        summary_model = Some(default_model.clone());
    }

    // Brief pause for daemon to process
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // Connectivity test — runs after provider, API key, and model are all configured.
    if !is_local_provider(&provider_id) && !api_key_saved.is_empty() {
        println!();
        println!("Testing connection to {provider_name}...");
        match validate_provider_on_stream(&mut framed, &provider_id, &auth_source).await {
            Ok(true) => {
                println!("{}", "Connection successful!".with(style::Color::Green));
            }
            Ok(false) => {
                println!("Provider validation returned invalid. You can retry with `tamux setup`.");
            }
            Err(e) => {
                println!("Could not test connection: {e}");
                println!("You can retry later with `tamux setup`.");
            }
        }
    }

    // ----- Security preference step (per D-10, D-11) -- all tiers -----
    println!();
    let security_items: Vec<(&str, &str)> = vec![
        ("Ask for risky actions only (strict)", "highest"),
        ("Ask for risky actions only", "moderate"),
        ("Ask for destructive actions only", "lowest"),
        ("I trust it, minimize interruptions", "yolo"),
    ];

    let security_default = default_security_index(&tier_string);
    let security_idx = select_list(
        "How cautious should tamux be?",
        &security_items,
        false,
        security_default,
    )?
    .expect("security selection is required");

    let (security_level_str, security_level_label) = security_level_from_index(security_idx);

    wizard_send(
        &mut framed,
        ClientMessage::AgentSetConfigItem {
            key_path: "/managed_execution/security_level".to_string(),
            value_json: format!("\"{}\"", security_level_str),
        },
    )
    .await
    .context("Failed to set security level")?;

    // Brief pause for daemon to process
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    // ----- Optional step: Web search API key -----
    {
        println!();

        // Check if any web search key is already configured
        let existing_ws = if read_config_key("firecrawl_api_key").await.is_some() {
            Some("Firecrawl")
        } else if read_config_key("exa_api_key").await.is_some() {
            Some("Exa")
        } else if read_config_key("tavily_api_key").await.is_some() {
            Some("Tavily")
        } else {
            None
        };

        let should_configure = if let Some(provider) = existing_ws {
            println!("Web search is already configured ({provider}).");
            match select_list(
                "Replace web search configuration?",
                &[("No, keep existing", ""), ("Yes, reconfigure", "")],
                true,
                0,
            )? {
                Some(1) => true,
                _ => {
                    summary_web_search = Some(provider.to_string());
                    false
                }
            }
        } else {
            true
        };

        if should_configure {
            let web_search_items: Vec<(&str, &str)> = vec![
                ("Firecrawl", "firecrawl_api_key"),
                ("Exa", "exa_api_key"),
                ("Tavily", "tavily_api_key"),
                ("Skip", ""),
            ];

            match select_list(
                "Configure web search? (enables agent web browsing)",
                &web_search_items,
                true,
                0,
            )? {
                Some(idx) if idx < 3 => {
                    let (provider_label, key_name) = web_search_items[idx];
                    match text_input(&format!("Enter {provider_label} API key"), "", true)? {
                        Some(key) if !key.is_empty() => {
                            // Enable web_search tool
                            wizard_send(
                                &mut framed,
                                ClientMessage::AgentSetConfigItem {
                                    key_path: "/tools/web_search".to_string(),
                                    value_json: "true".to_string(),
                                },
                            )
                            .await
                            .context("Failed to enable web search")?;

                            // Set the API key
                            wizard_send(
                                &mut framed,
                                ClientMessage::AgentSetConfigItem {
                                    key_path: format!("/{key_name}"),
                                    value_json: format!("\"{}\"", key),
                                },
                            )
                            .await
                            .context("Failed to set web search API key")?;

                            summary_web_search = Some(provider_label.to_string());
                            println!("Web search configured with {provider_label}.");
                        }
                        _ => {
                            println!("Skipped -- you can add web search later with `tamux setup`.");
                        }
                    }
                }
                _ => {
                    println!("Skipped -- you can add web search later with `tamux setup`.");
                }
            }
        } // end if should_configure

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    // ----- Optional step: Gateway setup -----
    {
        println!();

        // Check if any gateway is already configured
        let existing_gw = if read_config_key("gateway.slack_token").await.is_some() {
            Some("Slack")
        } else if read_config_key("gateway.discord_token").await.is_some() {
            Some("Discord")
        } else if read_config_key("gateway.telegram_token").await.is_some() {
            Some("Telegram")
        } else {
            None
        };

        let should_configure_gw = if let Some(platform) = existing_gw {
            println!("Gateway is already configured ({platform}).");
            match select_list(
                "Replace gateway configuration?",
                &[("No, keep existing", ""), ("Yes, reconfigure", "")],
                true,
                0,
            )? {
                Some(1) => true,
                _ => {
                    summary_gateway = Some(platform.to_string());
                    false
                }
            }
        } else {
            true
        };

        if should_configure_gw {
            let gateway_items = gateway_choice_items();

            match select_list(
                "Configure a chat gateway? (Slack, Discord, Telegram, WhatsApp)",
                &gateway_items,
                true,
                0,
            )? {
                Some(idx) if idx + 1 < gateway_items.len() => {
                    let (platform_label, platform) = gateway_items[idx];
                    if platform == "whatsapp" {
                        let whatsapp_allowlist_raw = loop {
                            println!();
                            match resolve_whatsapp_allowlist_prompt(
                                collect_whatsapp_allowlist_input()?,
                            ) {
                                WhatsAppAllowlistPromptResolution::Accept(raw) => break Some(raw),
                                WhatsAppAllowlistPromptResolution::Retry(message) => {
                                    println!("{message}");
                                }
                                WhatsAppAllowlistPromptResolution::Cancel => {
                                    println!(
                                        "Skipped -- you can configure WhatsApp later with `tamux setup`."
                                    );
                                    break None;
                                }
                            }
                        };

                        if let Some(whatsapp_allowlist_raw) = whatsapp_allowlist_raw {
                            for write in whatsapp_gateway_config_writes(&whatsapp_allowlist_raw)? {
                                wizard_send(
                                    &mut framed,
                                    ClientMessage::AgentSetConfigItem {
                                        key_path: write.key_path,
                                        value_json: write.value_json,
                                    },
                                )
                                .await
                                .context("Failed to save WhatsApp gateway setup")?;
                            }

                            let linked = run_whatsapp_link_subflow(&mut framed)
                                .await
                                .context("WhatsApp link flow failed during setup")?;
                            summary_gateway = Some(platform_label.to_string());
                            summary_whatsapp_linked = linked;
                            if linked {
                                println!("WhatsApp gateway linked.");
                            } else {
                                println!("WhatsApp gateway selected (link skipped).");
                            }
                        }
                    } else {
                        // Enable gateway for token-based platforms only after token entry succeeds.
                        match text_input(&format!("Enter {platform_label} token"), "", true)? {
                            Some(token) if !token.is_empty() => {
                                wizard_send(
                                    &mut framed,
                                    ClientMessage::AgentSetConfigItem {
                                        key_path: "/gateway/enabled".to_string(),
                                        value_json: "true".to_string(),
                                    },
                                )
                                .await
                                .context("Failed to enable gateway")?;

                                let token_key = format!("/gateway/{}_token", platform);
                                wizard_send(
                                    &mut framed,
                                    ClientMessage::AgentSetConfigItem {
                                        key_path: token_key,
                                        value_json: format!("\"{}\"", token),
                                    },
                                )
                                .await
                                .context("Failed to set gateway token")?;

                                summary_gateway = Some(platform_label.to_string());
                                println!("{platform_label} gateway configured.");
                            }
                            _ => {
                                println!(
                                    "Skipped -- you can configure gateways later with `tamux setup`."
                                );
                            }
                        }
                    }
                }
                _ => {
                    println!("Skipped -- you can configure gateways later with `tamux setup`.");
                }
            }
        } // end if should_configure_gw

        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    // ----- Optional step: Data directory info (Familiar+ per D-08 item 4) -----
    if tier_shows_step(&tier_string, "data_dir") {
        println!();
        let data_dir = amux_protocol::amux_data_dir();
        println!("Data stored at: {}", data_dir.display());
    }

    // ----- Final completion message with summary -----
    println!();
    println!("{}", "Setup complete!".bold());
    println!();
    println!("  Provider:  {provider_name}");
    println!("  Security:  {security_level_label}");
    if let Some(ref model) = summary_model {
        println!("  Model:     {model}");
    }
    if let Some(ref ws) = summary_web_search {
        println!("  Web Search: Enabled ({ws})");
    }
    if let Some(ref gw) = summary_gateway {
        if gw == "WhatsApp" {
            if summary_whatsapp_linked {
                println!("  Gateway:   WhatsApp linked");
            } else {
                println!("  Gateway:   WhatsApp selected (link skipped)");
            }
        } else {
            println!("  Gateway:   {gw} configured");
        }
    }
    println!();
    let launch_items = post_setup_choices();
    let launch_idx = select_list("What would you like to run now?", &launch_items, false, 0)?
        .expect("post-setup selection is required");
    let post_setup_action = post_setup_action_from_index(launch_idx);

    Ok(post_setup_action)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_select_list_wraps_index() {
        // Test the wrapping logic used in select_list.
        // We can't run the full interactive function in tests, but we can verify
        // the index arithmetic.
        let len = 4usize;

        // Wrap down from 0 -> last
        let mut idx = 0usize;
        if idx == 0 {
            idx = len.saturating_sub(1);
        } else {
            idx -= 1;
        }
        assert_eq!(idx, 3);

        // Wrap up from last -> 0
        idx = 3;
        idx += 1;
        if idx >= len {
            idx = 0;
        }
        assert_eq!(idx, 0);

        // Normal move down
        idx = 1;
        idx += 1;
        if idx >= len {
            idx = 0;
        }
        assert_eq!(idx, 2);
    }

    #[test]
    fn test_is_local_provider() {
        assert!(is_local_provider("ollama"));
        assert!(is_local_provider("lmstudio"));
        assert!(!is_local_provider("anthropic"));
        assert!(!is_local_provider("openai"));
    }

    #[test]
    fn test_security_default_for_tier() {
        // newcomer -> 0 (highest: approve risky actions)
        assert_eq!(default_security_index("newcomer"), 0);
        // familiar -> 1 (moderate: approve risky actions)
        assert_eq!(default_security_index("familiar"), 1);
        // power_user -> 2 (lowest: approve destructive only)
        assert_eq!(default_security_index("power_user"), 2);
        // expert -> 2 (lowest: approve destructive only)
        assert_eq!(default_security_index("expert"), 2);
        // unknown tier falls back to 1 (moderate)
        assert_eq!(default_security_index("unknown"), 1);
    }

    #[test]
    fn test_tier_shows_optional_steps() {
        // Newcomer: no auto-picker for model on first-time setup
        assert!(!tier_shows_step("newcomer", "model"));
        assert!(!tier_shows_step("newcomer", "data_dir"));

        // Familiar+: auto-show model picker on first-time setup
        assert!(tier_shows_step("familiar", "model"));
        assert!(tier_shows_step("familiar", "data_dir"));
        assert!(tier_shows_step("power_user", "model"));
        assert!(tier_shows_step("expert", "model"));
    }

    #[test]
    fn test_security_level_from_index() {
        assert_eq!(
            security_level_from_index(0),
            ("highest", "Approve risky actions")
        );
        assert_eq!(
            security_level_from_index(1),
            ("moderate", "Approve risky actions")
        );
        assert_eq!(
            security_level_from_index(2),
            ("lowest", "Approve destructive only")
        );
        assert_eq!(
            security_level_from_index(3),
            ("yolo", "Minimize interruptions")
        );
        // Out of range falls back to moderate
        assert_eq!(
            security_level_from_index(99),
            ("moderate", "Approve risky actions")
        );
    }

    #[test]
    fn test_post_setup_action_from_index() {
        assert_eq!(post_setup_action_from_index(0), PostSetupAction::LaunchTui);
        assert_eq!(
            post_setup_action_from_index(1),
            PostSetupAction::LaunchElectron
        );
        assert_eq!(post_setup_action_from_index(2), PostSetupAction::NotNow);
    }

    #[test]
    fn test_post_setup_choices_include_not_now() {
        let choices = post_setup_choices();
        assert_eq!(choices.len(), 3);
        assert_eq!(choices[0].0, "TUI");
        assert_eq!(choices[1].0, "Electron");
        assert_eq!(choices[2].0, "Not now");
    }

    #[test]
    fn test_gateway_choice_items_include_whatsapp_and_skip() {
        let items = gateway_choice_items();
        assert_eq!(items.len(), 5);
        assert_eq!(items[3], ("WhatsApp", "whatsapp"));
        assert_eq!(items[4], ("Skip", ""));
    }

    #[test]
    fn test_whatsapp_timeout_choice_mapping() {
        let choices = whatsapp_timeout_choices();
        assert_eq!(choices.len(), 2);
        assert!(whatsapp_timeout_retry_selected(0));
        assert!(!whatsapp_timeout_retry_selected(1));
    }

    #[test]
    fn whatsapp_setup_accepts_multiline_or_csv_contacts() {
        let parsed = parse_whatsapp_setup_allowlist("+48 123 456 789, 15551230000\n+44 20 7946 0958");

        assert_eq!(
            parsed,
            Some(vec![
                "48123456789".to_string(),
                "15551230000".to_string(),
                "442079460958".to_string(),
            ])
        );
    }

    #[test]
    fn whatsapp_setup_rejects_empty_allowlist() {
        assert_eq!(parse_whatsapp_setup_allowlist("\n , invalid , device "), None);
        assert_eq!(parse_whatsapp_setup_allowlist("   \n  "), None);
    }

    #[test]
    fn whatsapp_setup_cancellation_paths_stop_without_retry() {
        assert_eq!(
            resolve_whatsapp_allowlist_prompt(WhatsAppAllowlistPromptOutcome::Cancelled),
            WhatsAppAllowlistPromptResolution::Cancel
        );
        assert_eq!(
            resolve_whatsapp_allowlist_prompt(WhatsAppAllowlistPromptOutcome::EndOfInput),
            WhatsAppAllowlistPromptResolution::Cancel
        );
    }

    #[test]
    fn whatsapp_setup_invalid_submission_requests_retry() {
        assert_eq!(
            resolve_whatsapp_allowlist_prompt(WhatsAppAllowlistPromptOutcome::Submitted(
                "\n , invalid , device ".to_string()
            )),
            WhatsAppAllowlistPromptResolution::Retry(
                "Enter at least one valid WhatsApp phone number before linking."
            )
        );
    }

    #[test]
    fn whatsapp_setup_persists_allowlist_before_gateway_enable() {
        let writes = whatsapp_gateway_config_writes("+48 123 456 789").expect("valid writes");

        assert_eq!(writes.len(), 2);
        assert_eq!(writes[0].key_path, "/gateway/whatsapp_allowed_contacts");
        assert_eq!(writes[0].value_json, "\"+48 123 456 789\"");
        assert_eq!(writes[1].key_path, "/gateway/enabled");
        assert_eq!(writes[1].value_json, "true");
    }
}
