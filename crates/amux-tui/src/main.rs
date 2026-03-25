#![recursion_limit = "256"]

mod app;
mod auth;
mod client;
mod projection;
mod providers;
mod state;
mod theme;
mod widgets;
mod wire;

use std::io;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::{
    event::{
        self, DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event, KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
    ExecutableCommand,
};
use ratatui::prelude::*;
use tokio::sync::mpsc as tokio_mpsc;

use crate::app::TuiModel;
use crate::client::DaemonClient;
use crate::state::DaemonCommand;
use crate::wire::FetchedModel;

fn main() -> Result<()> {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| "/tmp".to_string());
    let log_dir = format!("{}/.tamux", home);
    let _ = std::fs::create_dir_all(&log_dir);
    let log_file = std::fs::File::create(format!("{}/tamux-tui.log", log_dir))?;
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_writer(log_file)
        .init();

    // Setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    stdout.execute(EnterAlternateScreen)?;
    stdout.execute(EnableMouseCapture)?;
    stdout.execute(EnableBracketedPaste)?;
    let supports_keyboard_enhancement = matches!(
        crossterm::terminal::supports_keyboard_enhancement(),
        Ok(true)
    );
    if supports_keyboard_enhancement {
        let _ = stdout.execute(PushKeyboardEnhancementFlags(
            KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
                | KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
                | KeyboardEnhancementFlags::REPORT_ALL_KEYS_AS_ESCAPE_CODES,
        ));
    }
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    // Setup daemon bridge
    let (daemon_event_tx, daemon_event_rx) = mpsc::channel();
    let (daemon_cmd_tx, daemon_cmd_rx) = tokio_mpsc::unbounded_channel();
    start_daemon_bridge(daemon_event_tx, daemon_cmd_rx);

    // Create model
    let mut model = TuiModel::new(daemon_event_rx, daemon_cmd_tx);
    model.load_saved_settings();

    // Main loop
    let tick_rate = Duration::from_millis(50);
    let result = run_loop(&mut terminal, &mut model, tick_rate);

    // Restore terminal
    disable_raw_mode()?;
    terminal.backend_mut().execute(DisableBracketedPaste)?;
    terminal.backend_mut().execute(DisableMouseCapture)?;
    if supports_keyboard_enhancement {
        let _ = terminal.backend_mut().execute(PopKeyboardEnhancementFlags);
    }
    terminal.backend_mut().execute(LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    model: &mut TuiModel,
    tick_rate: Duration,
) -> Result<()> {
    let mut next_tick = Instant::now() + tick_rate;

    loop {
        let now = Instant::now();
        if now >= next_tick {
            model.on_tick();
            next_tick = now + tick_rate;
        }

        model.pump_daemon_events();

        terminal.draw(|frame| {
            model.render(frame);
        })?;

        let now = Instant::now();
        let until_tick = next_tick.saturating_duration_since(now);
        let wait_for = until_tick.min(Duration::from_millis(10));

        if event::poll(wait_for)? {
            match event::read()? {
                Event::Key(key)
                    if matches!(
                        key.kind,
                        crossterm::event::KeyEventKind::Press
                            | crossterm::event::KeyEventKind::Repeat
                    ) =>
                {
                    if model.handle_key(key.code, key.modifiers) {
                        return Ok(());
                    }
                }
                Event::Key(key) if key.kind == crossterm::event::KeyEventKind::Release => {
                    model.handle_key_release(key.code, key.modifiers);
                }
                Event::Paste(text) => {
                    model.handle_paste(text);
                }
                Event::Resize(w, h) => model.handle_resize(w, h),
                Event::Mouse(mouse) => model.handle_mouse(mouse),
                _ => {}
            }
        }
    }
}

fn start_daemon_bridge(
    daemon_event_tx: mpsc::Sender<client::ClientEvent>,
    mut daemon_cmd_rx: tokio_mpsc::UnboundedReceiver<DaemonCommand>,
) {
    thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build();

        let Ok(runtime) = runtime else {
            let _ = daemon_event_tx.send(client::ClientEvent::Error(
                "failed to create tokio runtime".to_string(),
            ));
            return;
        };

        runtime.block_on(async move {
            let (client_event_tx, mut client_event_rx) = tokio_mpsc::channel(512);
            let client = DaemonClient::new(client_event_tx);

            if let Err(err) = client.connect().await {
                let _ = daemon_event_tx.send(client::ClientEvent::Error(format!(
                    "daemon connection init failed: {}",
                    err
                )));
            }

            loop {
                tokio::select! {
                    incoming = client_event_rx.recv() => {
                        match incoming {
                            Some(event) => {
                                let _ = daemon_event_tx.send(event);
                            }
                            None => break,
                        }
                    }
                    command = daemon_cmd_rx.recv() => {
                        let Some(command) = command else {
                            break;
                        };

                        match command {
                            DaemonCommand::Refresh => {
                                let _ = client.refresh();
                            }
                            DaemonCommand::RefreshServices => {
                                let _ = client.refresh_services();
                            }
                            DaemonCommand::RequestThread(thread_id) => {
                                let _ = client.request_thread(thread_id);
                            }
                            DaemonCommand::RequestThreadTodos(thread_id) => {
                                let _ = client.request_todos(thread_id);
                            }
                            DaemonCommand::RequestThreadWorkContext(thread_id) => {
                                let _ = client.request_work_context(thread_id);
                            }
                            DaemonCommand::RequestGoalRunDetail(goal_run_id) => {
                                let _ = client.request_goal_run(goal_run_id);
                            }
                            DaemonCommand::RequestGoalRunCheckpoints(goal_run_id) => {
                                let _ = client.list_checkpoints(goal_run_id);
                            }
                            DaemonCommand::StartGoalRun {
                                goal,
                                thread_id,
                                session_id,
                            } => {
                                let _ = client.start_goal_run(goal, thread_id, session_id);
                            }
                            DaemonCommand::RequestGitDiff { repo_path, file_path } => {
                                let _ = client.request_git_diff(repo_path, file_path);
                            }
                            DaemonCommand::RequestFilePreview { path, max_bytes } => {
                                let _ = client.request_file_preview(path, max_bytes);
                            }
                            DaemonCommand::SendMessage { thread_id, content, session_id } => {
                                let _ = client.send_message(thread_id, content, session_id);
                            }
                            DaemonCommand::StopStream { thread_id } => {
                                let _ = client.stop_stream(thread_id);
                            }
                            DaemonCommand::DeleteMessages { thread_id, message_ids } => {
                                let _ = client.delete_messages(thread_id, message_ids);
                            }
                            DaemonCommand::FetchModels { provider_id: _, base_url, api_key } => {
                                // Fetch models directly from provider API
                                let tx = daemon_event_tx.clone();
                                tokio::spawn(async move {
                                    match fetch_models_http(&base_url, &api_key).await {
                                        Ok(models) => {
                                            let _ = tx.send(client::ClientEvent::ModelsFetched(models));
                                        }
                                        Err(err) => {
                                            tracing::warn!("Model fetch failed: {}", err);
                                            // Don't send error — hardcoded fallback will be used
                                        }
                                    }
                                });
                            }
                            DaemonCommand::SetConfigItem { key_path, value_json } => {
                                let _ = client.set_config_item_json(key_path, value_json);
                            }
                            DaemonCommand::ControlGoalRun { goal_run_id, action } => {
                                let _ = client.control_goal_run(goal_run_id, action);
                            }
                            DaemonCommand::ResolveTaskApproval { approval_id, decision } => {
                                let _ = client.resolve_task_approval(approval_id, decision);
                            }
                            DaemonCommand::SpawnSession { shell, cwd, cols, rows } => {
                                let _ = client.spawn_session(shell, cwd, cols, rows);
                            }
                            DaemonCommand::GetProviderAuthStates => {
                                let _ = client.get_provider_auth_states();
                            }
                            DaemonCommand::ValidateProvider { provider_id, base_url, api_key, auth_source } => {
                                let _ = client.validate_provider(provider_id, base_url, api_key, auth_source);
                            }
                            DaemonCommand::SetSubAgent(sub_agent_json) => {
                                let _ = client.set_sub_agent(sub_agent_json);
                            }
                            DaemonCommand::RemoveSubAgent(sub_agent_id) => {
                                let _ = client.remove_sub_agent(sub_agent_id);
                            }
                            DaemonCommand::ListSubAgents => {
                                let _ = client.list_sub_agents();
                            }
                            DaemonCommand::GetConciergeConfig => {
                                let _ = client.get_concierge_config();
                            }
                            DaemonCommand::SetConciergeConfig(config_json) => {
                                let _ = client.set_concierge_config(config_json);
                            }
                            DaemonCommand::RequestConciergeWelcome => {
                                let _ = client.request_concierge_welcome();
                            }
                            DaemonCommand::DismissConciergeWelcome => {
                                let _ = client.dismiss_concierge_welcome();
                            }
                            DaemonCommand::RecordAttention {
                                surface,
                                thread_id,
                                goal_run_id,
                            } => {
                                let _ = client.record_attention(surface, thread_id, goal_run_id);
                            }
                            DaemonCommand::AuditDismiss { entry_id } => {
                                let _ = client.dismiss_audit_entry(entry_id);
                            }
                            // Plugin commands (Plan 16-03)
                            DaemonCommand::PluginList => {
                                let _ = client.plugin_list();
                            }
                            DaemonCommand::PluginGet(name) => {
                                let _ = client.plugin_get(name);
                            }
                            DaemonCommand::PluginEnable(name) => {
                                let _ = client.plugin_enable(name);
                            }
                            DaemonCommand::PluginDisable(name) => {
                                let _ = client.plugin_disable(name);
                            }
                            DaemonCommand::PluginGetSettings(name) => {
                                let _ = client.plugin_get_settings(name);
                            }
                            DaemonCommand::PluginUpdateSetting {
                                plugin_name,
                                key,
                                value,
                                is_secret,
                            } => {
                                let _ = client.plugin_update_setting(
                                    plugin_name,
                                    key,
                                    value,
                                    is_secret,
                                );
                            }
                            DaemonCommand::PluginTestConnection(name) => {
                                let _ = client.plugin_test_connection(name);
                            }
                            DaemonCommand::PluginListCommands => {
                                let _ = client.plugin_list_commands();
                            }
                            DaemonCommand::PluginOAuthStart(name) => {
                                let _ = client.plugin_oauth_start(name);
                            }
                        }
                    }
                }
            }
        });
    });
}

/// Fetch models from a provider's /models API endpoint.
/// Most OpenAI-compatible providers expose GET /models (or /v1/models).
async fn fetch_models_http(base_url: &str, api_key: &str) -> Result<Vec<FetchedModel>> {
    // Normalize URL: ensure it ends with /models
    let url = if base_url.ends_with("/models") {
        base_url.to_string()
    } else {
        let base = base_url.trim_end_matches('/');
        if base.ends_with("/v1") {
            format!("{}/models", base)
        } else {
            format!("{}/v1/models", base)
        }
    };

    let api_key = api_key.to_string();
    let mut last_error: Option<anyhow::Error> = None;

    for attempt in 0..3 {
        let url = url.clone();
        let api_key = api_key.clone();
        let result = tokio::task::spawn_blocking(move || -> Result<Vec<FetchedModel>> {
            let mut resp = ureq::get(&url)
                .header("Authorization", &format!("Bearer {}", api_key))
                .header("Accept", "application/json")
                .call()
                .map_err(|e| anyhow::anyhow!("HTTP request failed: {}", e))?;

            let body: serde_json::Value = resp
                .body_mut()
                .read_json()
                .map_err(|e| anyhow::anyhow!("JSON parse failed: {}", e))?;

            let mut models = Vec::new();
            if let Some(data) = body.get("data").and_then(|d| d.as_array()) {
                for item in data {
                    let id = item
                        .get("id")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    if id.is_empty() {
                        continue;
                    }
                    if id.contains("embedding")
                        || id.contains("tts")
                        || id.contains("whisper")
                        || id.contains("dall-e")
                        || id.contains("davinci")
                        || id.contains("babbage")
                        || id.contains("moderation")
                    {
                        continue;
                    }
                    let name = item.get("name").and_then(|v| v.as_str()).map(String::from);
                    let context_window = item
                        .get("context_window")
                        .and_then(|v| v.as_u64())
                        .map(|v| v as u32);
                    models.push(FetchedModel {
                        id,
                        name,
                        context_window,
                    });
                }
            }

            models.sort_by(|a, b| {
                b.context_window
                    .unwrap_or(0)
                    .cmp(&a.context_window.unwrap_or(0))
                    .then(a.id.cmp(&b.id))
            });

            Ok(models)
        })
        .await?;

        match result {
            Ok(models) => return Ok(models),
            Err(err) => {
                let message = err.to_string().to_ascii_lowercase();
                let retryable = message.contains("connection")
                    || message.contains("timed out")
                    || message.contains("error sending request")
                    || message.contains("transport")
                    || message.contains("io");
                last_error = Some(err);
                if retryable && attempt < 2 {
                    tokio::time::sleep(std::time::Duration::from_millis(1_000)).await;
                    continue;
                }
                break;
            }
        }
    }

    Err(last_error.unwrap_or_else(|| anyhow::anyhow!("model fetch failed")))
}
