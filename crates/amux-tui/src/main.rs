#![recursion_limit = "256"]

mod app;
mod auth;
mod client;
mod projection;
mod providers;
mod state;
mod theme;
mod update;
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
use tracing_subscriber::EnvFilter;

use crate::app::TuiModel;
use crate::client::DaemonClient;
use crate::state::DaemonCommand;

fn build_log_filter(
    tui_log: Option<&str>,
    tamux_log: Option<&str>,
    amux_log: Option<&str>,
) -> EnvFilter {
    tui_log
        .and_then(parse_log_filter)
        .or_else(|| tamux_log.and_then(parse_log_filter))
        .or_else(|| amux_log.and_then(parse_log_filter))
        .unwrap_or_else(|| EnvFilter::new("error"))
}

fn parse_log_filter(value: &str) -> Option<EnvFilter> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }

    EnvFilter::try_new(trimmed).ok()
}

fn main() -> Result<()> {
    let log_writer = amux_protocol::DailyLogWriter::new("tamux-tui.log")?;
    let log_path = log_writer.current_path()?;
    let (log_writer, _log_guard) = tracing_appender::non_blocking(log_writer);
    let tui_log = std::env::var("TAMUX_TUI_LOG").ok();
    let tamux_log = std::env::var("TAMUX_LOG").ok();
    let amux_log = std::env::var("AMUX_LOG").ok();
    tracing_subscriber::fmt()
        .with_env_filter(build_log_filter(
            tui_log.as_deref(),
            tamux_log.as_deref(),
            amux_log.as_deref(),
        ))
        .with_writer(log_writer)
        .with_ansi(false)
        .init();
    tracing::info!(path = %log_path.display(), "tui log file initialized");

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
    update::spawn_update_check(daemon_cmd_tx.clone());

    // Create model
    let mut model = TuiModel::new(daemon_event_rx, daemon_cmd_tx);
    model.load_saved_settings();

    // Main loop
    let tick_rate = Duration::from_millis(crate::app::TUI_TICK_RATE_MS);
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
                            DaemonCommand::RequestThread {
                                thread_id,
                                message_limit,
                                message_offset,
                            } => {
                                let _ = client.request_thread(
                                    thread_id,
                                    message_limit,
                                    message_offset,
                                );
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
                            DaemonCommand::ExplainAction {
                                action_id,
                                step_index,
                            } => {
                                let _ = client.explain_action(action_id, step_index);
                            }
                            DaemonCommand::StartDivergentSession {
                                problem_statement,
                                thread_id,
                                goal_run_id,
                            } => {
                                let _ = client.start_divergent_session(
                                    problem_statement,
                                    thread_id,
                                    goal_run_id,
                                );
                            }
                            DaemonCommand::GetDivergentSession { session_id } => {
                                let _ = client.get_divergent_session(session_id);
                            }
                            DaemonCommand::RequestGitDiff { repo_path, file_path } => {
                                let _ = client.request_git_diff(repo_path, file_path);
                            }
                            DaemonCommand::RequestFilePreview { path, max_bytes } => {
                                let _ = client.request_file_preview(path, max_bytes);
                            }
                            DaemonCommand::RequestAgentStatus => {
                                let _ = client.request_agent_status();
                            }
                            DaemonCommand::RequestAgentStatistics { window } => {
                                let _ = client.request_agent_statistics(window);
                            }
                            DaemonCommand::RequestPromptInspection { agent_id } => {
                                let _ = client.request_prompt_inspection(agent_id);
                            }
                            DaemonCommand::SendMessage {
                                thread_id,
                                content,
                                session_id,
                                target_agent_id,
                            } => {
                                let _ = client.send_message(
                                    thread_id,
                                    content,
                                    session_id,
                                    target_agent_id,
                                );
                            }
                            DaemonCommand::InternalDelegate {
                                thread_id,
                                target_agent_id,
                                content,
                                session_id,
                            } => {
                                let _ = client.send_internal_delegate(
                                    thread_id,
                                    target_agent_id,
                                    content,
                                    session_id,
                                );
                            }
                            DaemonCommand::ThreadParticipantCommand {
                                thread_id,
                                target_agent_id,
                                action,
                                instruction,
                                session_id,
                            } => {
                                let _ = client.send_thread_participant_command(
                                    thread_id,
                                    target_agent_id,
                                    action,
                                    instruction,
                                    session_id,
                                );
                            }
                            DaemonCommand::SendParticipantSuggestion {
                                thread_id,
                                suggestion_id,
                            } => {
                                let _ = client.send_participant_suggestion(thread_id, suggestion_id);
                            }
                            DaemonCommand::DismissParticipantSuggestion {
                                thread_id,
                                suggestion_id,
                            } => {
                                let _ = client.dismiss_participant_suggestion(thread_id, suggestion_id);
                            }
                            DaemonCommand::StopStream { thread_id } => {
                                let _ = client.stop_stream(thread_id);
                            }
                            DaemonCommand::ForceCompact { thread_id } => {
                                let _ = client.force_compact(thread_id);
                            }
                            DaemonCommand::RetryStreamNow { thread_id } => {
                                let _ = client.retry_stream_now(thread_id);
                            }
                            DaemonCommand::DeleteMessages { thread_id, message_ids } => {
                                let _ = client.delete_messages(thread_id, message_ids);
                            }
                            DaemonCommand::FetchModels {
                                provider_id,
                                base_url,
                                api_key,
                            } => {
                                let _ = client.fetch_models(provider_id, base_url, api_key);
                            }
                            DaemonCommand::SetConfigItem { key_path, value_json } => {
                                let _ = client.set_config_item_json(key_path, value_json);
                            }
                            DaemonCommand::SetProviderModel { provider_id, model } => {
                                let _ = client.set_provider_model(provider_id, model);
                            }
                            DaemonCommand::SetTargetAgentProviderModel {
                                target_agent_id,
                                provider_id,
                                model,
                            } => {
                                let _ = client.set_target_agent_provider_model(
                                    target_agent_id,
                                    provider_id,
                                    model,
                                );
                            }
                            DaemonCommand::ControlGoalRun { goal_run_id, action } => {
                                let _ = client.control_goal_run(goal_run_id, action);
                            }
                            DaemonCommand::ListTaskApprovalRules => {
                                let _ = client.list_task_approval_rules();
                            }
                            DaemonCommand::CreateTaskApprovalRule { approval_id } => {
                                let _ = client.create_task_approval_rule(approval_id);
                            }
                            DaemonCommand::RevokeTaskApprovalRule { rule_id } => {
                                let _ = client.revoke_task_approval_rule(rule_id);
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
                            DaemonCommand::GetOpenAICodexAuthStatus => {
                                let _ = client.get_openai_codex_auth_status();
                            }
                            DaemonCommand::LoginOpenAICodex => {
                                let _ = client.login_openai_codex();
                            }
                            DaemonCommand::LogoutOpenAICodex => {
                                let _ = client.logout_openai_codex();
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
                            DaemonCommand::RetryOperatorProfile => {
                                let _ = client.request_concierge_welcome();
                            }
                            DaemonCommand::StartOperatorProfileSession { kind } => {
                                let _ = client.start_operator_profile_session(kind);
                            }
                            DaemonCommand::NextOperatorProfileQuestion { session_id } => {
                                let _ = client.next_operator_profile_question(session_id);
                            }
                            DaemonCommand::SubmitOperatorProfileAnswer {
                                session_id,
                                question_id,
                                answer_json,
                            } => {
                                let _ = client.submit_operator_profile_answer(
                                    session_id,
                                    question_id,
                                    answer_json,
                                );
                            }
                            DaemonCommand::SkipOperatorProfileQuestion {
                                session_id,
                                question_id,
                                reason,
                            } => {
                                let _ = client.skip_operator_profile_question(
                                    session_id,
                                    question_id,
                                    reason,
                                );
                            }
                            DaemonCommand::DeferOperatorProfileQuestion {
                                session_id,
                                question_id,
                                defer_until_unix_ms,
                            } => {
                                let _ = client.defer_operator_profile_question(
                                    session_id,
                                    question_id,
                                    defer_until_unix_ms,
                                );
                            }
                            DaemonCommand::AnswerOperatorQuestion { question_id, answer } => {
                                let _ = client.answer_operator_question(question_id, answer);
                            }
                            DaemonCommand::GetOperatorProfileSummary => {
                                let _ = client.get_operator_profile_summary();
                            }
                            DaemonCommand::GetOperatorModel => {
                                let _ = client.get_operator_model();
                            }
                            DaemonCommand::ResetOperatorModel => {
                                let _ = client.reset_operator_model();
                            }
                            DaemonCommand::GetCollaborationSessions => {
                                let _ = client.get_collaboration_sessions();
                            }
                            DaemonCommand::VoteOnCollaborationDisagreement {
                                parent_task_id,
                                disagreement_id,
                                task_id,
                                position,
                                confidence,
                            } => {
                                let _ = client.vote_on_collaboration_disagreement(
                                    parent_task_id,
                                    disagreement_id,
                                    task_id,
                                    position,
                                    confidence,
                                );
                            }
                            DaemonCommand::GetGeneratedTools => {
                                let _ = client.get_generated_tools();
                            }
                            DaemonCommand::SetOperatorProfileConsent {
                                consent_key,
                                granted,
                            } => {
                                let _ = client.set_operator_profile_consent(consent_key, granted);
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
                            DaemonCommand::ListNotifications => {
                                let _ = client.list_notifications();
                            }
                            DaemonCommand::UpsertNotification(notification) => {
                                let _ = client.upsert_notification(notification);
                            }
                            DaemonCommand::WhatsAppLinkStart => {
                                let _ = client.whatsapp_link_start();
                            }
                            DaemonCommand::WhatsAppLinkStop => {
                                let _ = client.whatsapp_link_stop();
                            }
                            DaemonCommand::WhatsAppLinkStatus => {
                                let _ = client.whatsapp_link_status();
                            }
                            DaemonCommand::WhatsAppLinkSubscribe => {
                                let _ = client.whatsapp_link_subscribe();
                            }
                            DaemonCommand::WhatsAppLinkUnsubscribe => {
                                let _ = client.whatsapp_link_unsubscribe();
                            }
                            DaemonCommand::WhatsAppLinkReset => {
                                let _ = client.whatsapp_link_reset();
                            }
                        }
                    }
                }
            }
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    };

    #[test]
    fn fetch_models_command_does_not_use_local_http_bridge() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind test http server");
        listener
            .set_nonblocking(true)
            .expect("set nonblocking listener");
        let addr = listener.local_addr().expect("listener addr");
        let hit = Arc::new(AtomicBool::new(false));
        let hit_for_server = Arc::clone(&hit);

        let server = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_millis(700);
            while Instant::now() < deadline {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        hit_for_server.store(true, Ordering::SeqCst);
                        let mut buffer = [0_u8; 1024];
                        let _ = stream.read(&mut buffer);
                        let body = r#"{"data":[{"id":"sdk-model"}]}"#;
                        let response = format!(
                            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                            body.len(),
                            body
                        );
                        let _ = stream.write_all(response.as_bytes());
                        return;
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => return,
                }
            }
        });

        let (daemon_event_tx, _daemon_event_rx) = mpsc::channel();
        let (daemon_cmd_tx, daemon_cmd_rx) = tokio_mpsc::unbounded_channel();
        start_daemon_bridge(daemon_event_tx, daemon_cmd_rx);

        daemon_cmd_tx
            .send(DaemonCommand::FetchModels {
                provider_id: amux_shared::providers::PROVIDER_ID_GITHUB_COPILOT.to_string(),
                base_url: format!("http://{addr}"),
                api_key: "test-key".to_string(),
            })
            .expect("queue fetch models command");

        thread::sleep(Duration::from_millis(350));

        assert!(
            !hit.load(Ordering::SeqCst),
            "fetch models should be delegated to the daemon, not fetched directly over HTTP"
        );

        server.join().expect("join test http server");
    }

    #[test]
    fn tui_log_filter_defaults_to_error_only() {
        let filter = build_log_filter(None, None, None);

        assert_eq!(filter.to_string(), "error");
    }
}
