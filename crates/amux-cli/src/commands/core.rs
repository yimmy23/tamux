use anyhow::{bail, Result};
use clap::Parser;

use crate::cli::{Cli, Commands, InstallTarget, SettingsAction};
use crate::commands::common::{
    find_sibling_binary, handle_post_setup_action, launch_gui, launch_tui, resolve_dm_target,
    LaunchTarget,
};
use crate::output::audit::{
    format_timestamp, parse_duration_ago, print_audit_detail, print_audit_row,
};
use crate::output::settings::{flatten_json, is_sensitive_key, resolve_dot_path};
use crate::{client, plugins, setup_wizard, update};

pub(crate) fn should_check_for_updates(command: &Commands) -> bool {
    matches!(
        command,
        Commands::List
            | Commands::Clone { .. }
            | Commands::Kill { .. }
            | Commands::Git { .. }
            | Commands::Audit { .. }
            | Commands::Status
            | Commands::Stats
            | Commands::Settings { .. }
            | Commands::Dm { .. }
            | Commands::Setup
            | Commands::Ping
            | Commands::StartDaemon
            | Commands::Plugin { .. }
            | Commands::Install { .. }
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DefaultStartupAction {
    ShowHelp,
    RunSetup,
    StartDaemonAndRetry,
}

fn default_startup_action(probe: setup_wizard::SetupProbe) -> DefaultStartupAction {
    match probe {
        setup_wizard::SetupProbe::Ready => DefaultStartupAction::ShowHelp,
        setup_wizard::SetupProbe::NeedsSetup => DefaultStartupAction::RunSetup,
        setup_wizard::SetupProbe::DaemonUnavailable => DefaultStartupAction::StartDaemonAndRetry,
    }
}

fn format_direct_message_output(
    response: &client::DirectMessageResponse,
    json: bool,
) -> Result<String> {
    let provider_final_result = response
        .provider_final_result_json
        .as_deref()
        .map(serde_json::from_str::<serde_json::Value>)
        .transpose()?;

    if json {
        return serde_json::to_string_pretty(&serde_json::json!({
            "target": response.target,
            "thread_id": response.thread_id,
            "session_id": response.session_id,
            "response": response.response,
            "provider_final_result": provider_final_result,
        }))
        .map_err(Into::into);
    }

    let mut rendered = response.response.clone();
    rendered.push_str("\n\n");
    rendered.push_str(&format!("thread_id:{}", response.thread_id));
    if let Some(session_id) = response.session_id.as_deref() {
        rendered.push_str(&format!("\nsession_id:{session_id}"));
    }
    if let Some(value) = provider_final_result {
        rendered.push_str("\nprovider_final_result:\n");
        rendered.push_str(&serde_json::to_string_pretty(&value)?);
    }

    Ok(rendered)
}

pub(crate) async fn run_default() -> Result<()> {
    update::print_upgrade_notice_if_available(env!("CARGO_PKG_VERSION")).await;

    let mut restarted_daemon = false;
    loop {
        match default_startup_action(setup_wizard::probe_setup_via_ipc().await) {
            DefaultStartupAction::ShowHelp => {
                Cli::parse_from(["tamux", "--help"]);
                break;
            }
            DefaultStartupAction::RunSetup => {
                println!("Welcome to tamux! Running first-time setup...\n");
                let action = setup_wizard::run_setup_wizard().await?;
                match handle_post_setup_action(action) {
                    Some(LaunchTarget::Tui) => {
                        println!("\nLaunching TUI...");
                        launch_tui();
                    }
                    Some(LaunchTarget::Gui) => {
                        println!("\nLaunching desktop app...");
                        launch_gui()?;
                    }
                    None => {
                        println!("\nSetup complete. Start later with `tamux tui` or `tamux gui`.");
                    }
                }
                break;
            }
            DefaultStartupAction::StartDaemonAndRetry => {
                if restarted_daemon {
                    bail!("daemon is still unreachable after startup retry");
                }
                setup_wizard::ensure_daemon_running().await?;
                restarted_daemon = true;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        default_startup_action, format_direct_message_output, DefaultStartupAction,
    };
    use crate::client::DirectMessageResponse;
    use crate::setup_wizard::SetupProbe;

    #[test]
    fn default_startup_restarts_daemon_before_considering_setup() {
        assert_eq!(
            default_startup_action(SetupProbe::DaemonUnavailable),
            DefaultStartupAction::StartDaemonAndRetry
        );
    }

    #[test]
    fn default_startup_runs_setup_only_when_config_requires_it() {
        assert_eq!(
            default_startup_action(SetupProbe::NeedsSetup),
            DefaultStartupAction::RunSetup
        );
        assert_eq!(
            default_startup_action(SetupProbe::Ready),
            DefaultStartupAction::ShowHelp
        );
    }

    #[test]
    fn direct_message_json_output_embeds_provider_final_result() {
        let rendered = format_direct_message_output(
            &DirectMessageResponse {
                target: "main".to_string(),
                thread_id: "thread-1".to_string(),
                response: "protocol reply".to_string(),
                session_id: Some("session-1".to_string()),
                provider_final_result_json: Some(
                    r#"{"provider":"open_ai_responses","id":"resp_1"}"#.to_string(),
                ),
            },
            true,
        )
        .expect("render json output");

        let value: serde_json::Value = serde_json::from_str(&rendered).expect("parse rendered json");
        assert_eq!(value.get("target").and_then(|v| v.as_str()), Some("main"));
        assert_eq!(value.get("thread_id").and_then(|v| v.as_str()), Some("thread-1"));
        assert_eq!(value.get("session_id").and_then(|v| v.as_str()), Some("session-1"));
        assert_eq!(value.get("response").and_then(|v| v.as_str()), Some("protocol reply"));
        assert_eq!(
            value.pointer("/provider_final_result/provider").and_then(|v| v.as_str()),
            Some("open_ai_responses")
        );
        assert_eq!(
            value.pointer("/provider_final_result/id").and_then(|v| v.as_str()),
            Some("resp_1")
        );
    }

    #[test]
    fn direct_message_plain_output_prints_provider_final_result_block() {
        let rendered = format_direct_message_output(
            &DirectMessageResponse {
                target: "main".to_string(),
                thread_id: "thread-1".to_string(),
                response: "protocol reply".to_string(),
                session_id: None,
                provider_final_result_json: Some(
                    r#"{"provider":"anthropic_message","id":"msg_1"}"#.to_string(),
                ),
            },
            false,
        )
        .expect("render plain output");

        assert!(rendered.contains("protocol reply"));
        assert!(rendered.contains("thread_id:thread-1"));
        assert!(rendered.contains("provider_final_result:"));
        assert!(rendered.contains("\"provider\": \"anthropic_message\""));
        assert!(rendered.contains("\"id\": \"msg_1\""));
    }
}

pub(crate) async fn run(command: Commands) -> Result<()> {
    if should_check_for_updates(&command) {
        update::print_upgrade_notice_if_available(env!("CARGO_PKG_VERSION")).await;
    }

    match command {
        Commands::Tui => {
            launch_tui();
        }
        Commands::Gui => {
            launch_gui()?;
        }
        Commands::List => {
            let sessions = client::list_sessions().await?;
            if sessions.is_empty() {
                println!("No active sessions.");
            } else {
                println!(
                    "{:<38} {:>5} {:>5}  {:>5}  {}",
                    "ID", "COLS", "ROWS", "ALIVE", "CWD"
                );
                for session in sessions {
                    println!(
                        "{:<38} {:>5} {:>5}  {:>5}  {}",
                        session.id,
                        session.cols,
                        session.rows,
                        if session.is_alive { "yes" } else { "no" },
                        session.cwd.unwrap_or_default()
                    );
                }
            }
        }
        Commands::New {
            shell,
            cwd,
            workspace,
        } => {
            let id = client::spawn_session(shell, cwd, workspace).await?;
            println!("Session created: {id}");
        }
        Commands::Attach { id } => {
            println!("Attaching to session {id}...");
            client::attach_session(&id).await?;
        }
        Commands::Clone {
            source,
            cols,
            rows,
            workspace,
            cwd,
        } => {
            let (id, active_command) =
                client::clone_session(&source, workspace, cols, rows, cwd).await?;
            println!("{id}");
            if let Some(command) = active_command {
                println!("active_command:{command}");
            }
        }
        Commands::Kill { id } => {
            client::kill_session(&id).await?;
            println!("Session killed: {id}");
        }
        Commands::Git { path } => {
            let abs_path =
                std::fs::canonicalize(&path).unwrap_or_else(|_| std::path::PathBuf::from(&path));
            let info = client::get_git_status(abs_path.to_string_lossy().to_string()).await?;
            println!("Branch: {}", info.branch.as_deref().unwrap_or("(none)"));
            println!("Dirty:  {}", info.is_dirty);
            println!("Ahead:  {} Behind: {}", info.ahead, info.behind);
            println!(
                "Staged: {} Modified: {} Untracked: {}",
                info.staged, info.modified, info.untracked
            );
        }
        Commands::Audit {
            r#type,
            since,
            detail,
            limit,
        } => {
            let since_ts = since.as_deref().and_then(parse_duration_ago);
            let entries = client::send_audit_query(r#type, since_ts, Some(limit)).await?;

            if let Some(detail_id) = &detail {
                if let Some(entry) = entries.iter().find(|entry| entry.id == *detail_id) {
                    print_audit_detail(entry);
                } else {
                    eprintln!("No audit entry found with ID: {detail_id}");
                }
                return Ok(());
            }

            if entries.is_empty() {
                println!("No audit entries found.");
                return Ok(());
            }

            for entry in &entries {
                print_audit_row(entry);
            }
            println!(
                "\n{} entries shown. Use --detail <id> for full trace.",
                entries.len()
            );
        }
        Commands::Status | Commands::Stats => {
            let status = client::send_status_query().await?;
            println!("Agent Status");
            println!("============");
            println!("Tier:     {}", status.tier.replace('_', " "));
            println!("Activity: {}", status.activity.replace('_', " "));

            if let Some(title) = &status.active_goal_run_title {
                println!("Goal:     {}", title);
            }
            if let Some(thread) = &status.active_thread_id {
                println!("Thread:   {}", thread);
            }

            if let Ok(providers) =
                serde_json::from_str::<serde_json::Value>(&status.provider_health_json)
            {
                if let Some(obj) = providers.as_object() {
                    if !obj.is_empty() {
                        println!("\nProviders:");
                        for (name, info) in obj {
                            let can_exec = info
                                .get("can_execute")
                                .and_then(|value| value.as_bool())
                                .unwrap_or(true);
                            let trips = info
                                .get("trip_count")
                                .and_then(|value| value.as_u64())
                                .unwrap_or(0);
                            let health = if can_exec { "healthy" } else { "tripped" };
                            if trips > 0 {
                                println!("  {} - {} (trips: {})", name, health, trips);
                            } else {
                                println!("  {} - {}", name, health);
                            }
                        }
                    }
                }
            }

            if let Ok(gateways) =
                serde_json::from_str::<serde_json::Value>(&status.gateway_statuses_json)
            {
                if let Some(obj) = gateways.as_object() {
                    if !obj.is_empty() {
                        println!("\nGateways:");
                        for (platform, info) in obj {
                            let gateway_status = info
                                .get("status")
                                .and_then(|value| value.as_str())
                                .unwrap_or("unknown");
                            println!("  {} - {}", platform, gateway_status);
                        }
                    }
                }
            }

            if let Ok(actions) =
                serde_json::from_str::<Vec<serde_json::Value>>(&status.recent_actions_json)
            {
                if !actions.is_empty() {
                    println!("\nRecent Actions:");
                    for action in actions.iter().take(5) {
                        let action_type = action
                            .get("action_type")
                            .and_then(|value| value.as_str())
                            .unwrap_or("");
                        let summary = action
                            .get("summary")
                            .and_then(|value| value.as_str())
                            .unwrap_or("");
                        let timestamp = action
                            .get("timestamp")
                            .and_then(|value| value.as_i64())
                            .unwrap_or(0);
                        println!(
                            "  {} [{}] {}",
                            format_timestamp(timestamp),
                            action_type,
                            summary
                        );
                    }
                }
            }
        }
        Commands::Settings { action } => match action {
            SettingsAction::List => {
                let config = client::send_config_get().await?;
                let mut pairs = Vec::new();
                flatten_json("", &config, &mut pairs);
                if pairs.is_empty() {
                    println!("No configuration found.");
                } else {
                    for (key, value) in &pairs {
                        println!("{key} = {value}");
                    }
                }
            }
            SettingsAction::Get { key } => {
                let config = client::send_config_get().await?;
                match resolve_dot_path(&config, &key) {
                    Some(value) => {
                        if is_sensitive_key(&key) {
                            println!("***");
                        } else if let Some(string_value) = value.as_str() {
                            println!("{string_value}");
                        } else {
                            println!(
                                "{}",
                                serde_json::to_string_pretty(value).unwrap_or_default()
                            );
                        }
                    }
                    None => {
                        eprintln!("Key not found: {key}");
                        std::process::exit(1);
                    }
                }
            }
            SettingsAction::Set { key, value } => {
                let json_pointer = format!("/{}", key.replace('.', "/"));
                let value_json = if serde_json::from_str::<serde_json::Value>(&value).is_ok() {
                    value.clone()
                } else {
                    serde_json::to_string(&value).unwrap_or_else(|_| format!("\"{}\"", value))
                };
                client::send_config_set(json_pointer, value_json).await?;
                println!("{key} = {value}");
            }
        },
        Commands::Dm {
            thread,
            session,
            svarog,
            rarog,
            main_target,
            concierge,
            json,
            message,
        } => {
            let content = message.join(" ").trim().to_string();
            if content.is_empty() {
                bail!("message cannot be empty");
            }
            let target = resolve_dm_target(svarog, rarog, main_target, concierge);
            let response = client::send_direct_message(target, thread, content, session).await?;
            println!("{}", format_direct_message_output(&response, json)?);
        }
        Commands::Scrub { text } => {
            let input = if let Some(value) = text {
                value
            } else {
                use std::io::Read;
                let mut buffer = String::new();
                std::io::stdin().read_to_string(&mut buffer)?;
                buffer
            };
            let result = client::scrub_text(input).await?;
            print!("{result}");
        }
        Commands::Setup => {
            let action = setup_wizard::run_setup_wizard().await?;
            match handle_post_setup_action(action) {
                Some(LaunchTarget::Tui) => {
                    println!("\nLaunching TUI...");
                    launch_tui();
                }
                Some(LaunchTarget::Gui) => {
                    println!("\nLaunching desktop app...");
                    launch_gui()?;
                }
                None => {
                    println!("\nSetup complete. Start later with `tamux tui` or `tamux gui`.");
                }
            }
        }
        Commands::Ping => {
            client::ping().await?;
            println!("Daemon is alive (pong).");
        }
        Commands::Upgrade => {
            update::run_upgrade()?;
        }
        Commands::StartDaemon => {
            println!("Starting daemon...");
            let mut command = std::process::Command::new(find_sibling_binary("tamux-daemon"));
            command
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());

            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                command.creation_flags(0x08000000);
            }

            command.spawn()?;
            println!("Daemon started.");
        }
        Commands::Install { target } => match target {
            InstallTarget::Plugin { package } => {
                let installed = plugins::install_plugin(&package)?;
                println!(
                    "Installed plugin {}@{}\nentry: {}\nformat: {}",
                    installed.package_name,
                    installed.package_version,
                    installed.entry_path,
                    installed.format
                );
            }
        },
        Commands::AgentBridge => {
            client::run_agent_bridge().await?;
        }
        Commands::DbBridge => {
            client::run_db_bridge().await?;
        }
        Commands::Bridge {
            session,
            shell,
            cwd,
            workspace,
            cols,
            rows,
        } => {
            client::run_bridge(session, shell, cwd, workspace, cols, rows).await?;
        }
        Commands::Skill { .. } | Commands::Plugin { .. } => unreachable!(),
    }

    Ok(())
}
