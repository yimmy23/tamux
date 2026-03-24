mod client;
mod plugins;
mod setup_wizard;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

/// Find a binary by checking the same directory as the current executable first,
/// then falling back to the bare name (which uses PATH lookup).
fn find_sibling_binary(name: &str) -> std::path::PathBuf {
    if let Ok(current) = std::env::current_exe() {
        if let Some(dir) = current.parent() {
            let sibling = dir.join(name);
            if sibling.exists() {
                return sibling;
            }
        }
    }
    // Fall back to bare name -- OS will search PATH
    std::path::PathBuf::from(name)
}

fn truncate_for_display(value: &str, max_chars: usize) -> String {
    let truncated: String = value.chars().take(max_chars).collect();
    if value.chars().count() > max_chars {
        format!("{truncated}…")
    } else {
        truncated
    }
}

#[derive(Parser)]
#[command(name = "tamux", about = "tamux terminal multiplexer CLI")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Launch the terminal UI.
    Tui,

    /// Launch the desktop GUI (Electron).
    Gui,

    /// Show agent statistics (alias for status).
    Stats,

    /// List all running sessions.
    #[command(alias = "ls")]
    List,

    /// Spawn a new terminal session.
    #[command(hide = true)]
    New {
        /// Shell to use (default: system shell).
        #[arg(short, long)]
        shell: Option<String>,
        /// Working directory.
        #[arg(short, long)]
        cwd: Option<String>,
        /// Workspace ID to associate with.
        #[arg(short, long)]
        workspace: Option<String>,
    },

    /// Attach to an existing session.
    #[command(hide = true)]
    Attach {
        /// Session ID.
        id: String,
    },

    /// Clone an existing session into a new independent session.
    Clone {
        /// Source session ID.
        #[arg(long)]
        source: String,
        /// Optional terminal columns override.
        #[arg(long)]
        cols: Option<u16>,
        /// Optional terminal rows override.
        #[arg(long)]
        rows: Option<u16>,
        /// Optional workspace override.
        #[arg(short, long)]
        workspace: Option<String>,
        /// Fallback CWD when the source session's CWD cannot be resolved.
        #[arg(long)]
        cwd: Option<String>,
    },

    /// Kill a session.
    Kill {
        /// Session ID.
        id: String,
    },

    /// Get git status for a directory.
    Git {
        /// Directory path (default: current directory).
        #[arg(default_value = ".")]
        path: String,
    },

    /// Scrub sensitive data from stdin or a string.
    #[command(hide = true)]
    Scrub {
        /// Text to scrub (reads from stdin if not provided).
        text: Option<String>,
    },

    /// View the agent's action audit trail.
    #[command(alias = "log")]
    Audit {
        /// Filter by action type(s), comma-separated (heartbeat,tool,escalation,skill,subagent).
        #[arg(long, value_delimiter = ',')]
        r#type: Option<Vec<String>>,
        /// Show actions since this time ago (e.g., "1h", "24h", "7d").
        #[arg(long)]
        since: Option<String>,
        /// Show full detail for a specific action ID.
        #[arg(long)]
        detail: Option<String>,
        /// Maximum number of entries to show.
        #[arg(long, default_value = "50")]
        limit: usize,
    },

    /// Manage discovered skills.
    Skill {
        #[command(subcommand)]
        action: SkillAction,
    },

    /// Show current agent status.
    Status,

    /// View and modify daemon configuration.
    Settings {
        #[command(subcommand)]
        action: SettingsAction,
    },

    /// Run the first-time setup wizard.
    Setup,

    /// Ping the daemon (health check).
    Ping,

    /// Start the daemon (if not already running).
    #[command(name = "daemon")]
    StartDaemon,

    /// Manage plugins (v2 manifest format).
    Plugin {
        #[command(subcommand)]
        action: PluginAction,
    },

    /// Install runtime extensions.
    Install {
        #[command(subcommand)]
        target: InstallTarget,
    },

    /// Internal agent bridge used by the Electron frontend.
    #[command(hide = true, name = "agent-bridge")]
    AgentBridge,

    /// Internal database bridge used by the Electron frontend.
    #[command(hide = true, name = "db-bridge")]
    DbBridge,

    /// Internal JSON bridge used by the Electron frontend.
    #[command(hide = true, name = "bridge")]
    Bridge {
        /// Attach to an existing session instead of spawning a new one.
        #[arg(long)]
        session: Option<String>,
        /// Shell to use when spawning a session.
        #[arg(long)]
        shell: Option<String>,
        /// Working directory when spawning a session.
        #[arg(long)]
        cwd: Option<String>,
        /// Workspace ID to associate with the spawned session.
        #[arg(long)]
        workspace: Option<String>,
        /// Initial terminal columns.
        #[arg(long, default_value_t = 80)]
        cols: u16,
        /// Initial terminal rows.
        #[arg(long, default_value_t = 24)]
        rows: u16,
    },
}

#[derive(Debug, Subcommand)]
enum InstallTarget {
    /// Install a tamux plugin from npm or a local package directory.
    Plugin {
        /// npm package spec or local package directory.
        package: String,
    },
}

#[derive(Debug, Subcommand)]
enum PluginAction {
    /// Install a plugin from npm, GitHub, or local path.
    Add {
        /// Plugin source: npm package name, GitHub URL, or local directory path.
        source: String,
    },
    /// Uninstall a plugin by name.
    Remove {
        /// Plugin name to uninstall.
        name: String,
    },
    /// List installed plugins. Per INST-05/D-08.
    #[command(alias = "list")]
    Ls,
    /// Enable a disabled plugin. Per INST-06.
    Enable {
        /// Plugin name to enable.
        name: String,
    },
    /// Disable a plugin without uninstalling. Per INST-06.
    Disable {
        /// Plugin name to disable.
        name: String,
    },
}

#[derive(Debug, Subcommand)]
enum SkillAction {
    /// List all skills with their maturity status.
    #[command(alias = "ls")]
    List {
        /// Filter by status (draft, testing, active, proven, promoted_to_canonical).
        #[arg(long)]
        status: Option<String>,
        /// Maximum number of entries to show.
        #[arg(long, default_value = "50")]
        limit: usize,
    },
    /// Show details of a specific skill.
    Inspect {
        /// Skill name or variant ID.
        name: String,
    },
    /// Reject and delete a draft skill.
    Reject {
        /// Skill name or variant ID.
        name: String,
    },
    /// Fast-promote a skill (skip to target status).
    Promote {
        /// Skill name or variant ID.
        name: String,
        /// Target status to promote to.
        #[arg(long, default_value = "active")]
        to: String,
    },
    /// Search the community skill registry.
    Search {
        /// Search query (matches skill name, description, tags).
        query: String,
    },
    /// Import a community skill by name or URL.
    Import {
        /// Skill name from registry or direct URL.
        source: String,
        /// Override security warnings.
        #[arg(long)]
        force: bool,
    },
    /// Export a local skill to a directory.
    Export {
        /// Skill name or variant ID.
        name: String,
        /// Output format: "tamux" (default) or "agentskills".
        #[arg(long, default_value = "tamux")]
        format: String,
        /// Output directory (default: current directory).
        #[arg(long, default_value = ".")]
        output: String,
    },
    /// Publish a proven skill to the community registry.
    Publish {
        /// Skill name or variant ID.
        name: String,
    },
}

#[derive(Debug, Subcommand)]
enum SettingsAction {
    /// List all configuration settings.
    #[command(alias = "ls")]
    List,
    /// Get a specific configuration value.
    Get {
        /// Configuration key (dot-notation, e.g., "heartbeat.interval", "provider", "model").
        key: String,
    },
    /// Set a configuration value.
    Set {
        /// Configuration key (dot-notation, e.g., "heartbeat.interval", "provider", "model").
        key: String,
        /// Value to set (JSON or plain string).
        value: String,
    },
}

fn init_logging(log_file_name: &str) -> Result<tracing_appender::non_blocking::WorkerGuard> {
    let log_dir = amux_protocol::ensure_amux_data_dir()?;
    let log_path = amux_protocol::log_file_path(log_file_name);
    let file_appender = tracing_appender::rolling::never(log_dir, log_file_name);
    let (writer, guard) = tracing_appender::non_blocking(file_appender);

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_env("TAMUX_LOG")
                .or_else(|_| EnvFilter::try_from_env("AMUX_LOG"))
                .unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(writer)
        .with_ansi(false)
        .init();

    std::panic::set_hook(Box::new(|panic_info| {
        tracing::error!(panic = %panic_info, "tamux-cli panicked");
    }));

    tracing::info!(path = %log_path.display(), "cli log file initialized");
    Ok(guard)
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let log_file_name = match &cli.command {
        Some(Commands::Bridge { .. }) => "tamux-bridge.log",
        _ => "tamux-cli.log",
    };
    let _log_guard = init_logging(log_file_name)?;
    tracing::info!(command = ?cli.command, "tamux-cli starting");

    // If no subcommand provided, check for first-run setup via IPC with legacy fallback
    if cli.command.is_none() {
        // Two-phase detection: try IPC first (daemon reachable), fall back to
        // legacy config.json existence check (first-ever run, daemon not started yet).
        let needs = setup_wizard::needs_setup_via_ipc().await
            || setup_wizard::needs_setup_legacy();

        if needs {
            println!("Welcome to tamux! Running first-time setup...\n");
            setup_wizard::run_setup_wizard().await?;
            // Wizard already ensured daemon is running and configured

            println!("\nLaunching TUI...");
            let status = std::process::Command::new("tamux-tui")
                .status()
                .unwrap_or_else(|e| {
                    eprintln!("Could not launch TUI: {e}");
                    eprintln!("Start manually with: tamux-tui");
                    std::process::exit(1);
                });
            std::process::exit(status.code().unwrap_or(1));
        } else {
            // Config exists, no subcommand -- show help
            Cli::parse_from(["tamux", "--help"]);
            return Ok(());
        }
    }

    let command = cli.command.unwrap(); // Safe: we handled None above

    match command {
        Commands::Tui => {
            let binary = find_sibling_binary("tamux-tui");
            let status = std::process::Command::new(&binary).status();
            match status {
                Ok(s) => std::process::exit(s.code().unwrap_or(1)),
                Err(e) => {
                    eprintln!("Error: could not launch tamux-tui: {e}");
                    eprintln!("Install it or ensure it is on your PATH.");
                    std::process::exit(1);
                }
            }
        }

        Commands::Gui => {
            // Try TAMUX_GUI_PATH env var first
            let gui_binary = std::env::var("TAMUX_GUI_PATH")
                .map(std::path::PathBuf::from)
                .ok()
                .filter(|p| p.exists())
                .unwrap_or_else(|| find_sibling_binary("tamux-desktop"));

            let result = std::process::Command::new(&gui_binary).spawn();
            match result {
                Ok(_child) => {
                    println!("Launched tamux desktop GUI.");
                }
                Err(_) => {
                    // Platform-specific fallback
                    #[cfg(target_os = "macos")]
                    {
                        let _ = std::process::Command::new("open")
                            .arg("-a")
                            .arg("tamux")
                            .status()
                            .map_err(|e| {
                                eprintln!("Error: could not launch tamux GUI: {e}");
                                eprintln!("Install the desktop app or set TAMUX_GUI_PATH.");
                                std::process::exit(1);
                            });
                        println!("Launched tamux desktop GUI.");
                    }
                    #[cfg(not(target_os = "macos"))]
                    {
                        eprintln!("Error: could not launch tamux desktop GUI.");
                        eprintln!("Install the desktop app or set TAMUX_GUI_PATH.");
                        std::process::exit(1);
                    }
                }
            }
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
                for s in sessions {
                    println!(
                        "{:<38} {:>5} {:>5}  {:>5}  {}",
                        s.id,
                        s.cols,
                        s.rows,
                        if s.is_alive { "yes" } else { "no" },
                        s.cwd.unwrap_or_default()
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
            if let Some(cmd) = active_command {
                println!("active_command:{cmd}");
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

            // If --detail specified, find entry by ID and print full detail
            if let Some(detail_id) = &detail {
                if let Some(entry) = entries.iter().find(|e| e.id == *detail_id) {
                    print_audit_detail(entry);
                } else {
                    eprintln!("No audit entry found with ID: {}", detail_id);
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

        Commands::Skill { action } => match action {
            SkillAction::List { status, limit } => {
                let variants = client::send_skill_list(status, limit).await?;
                if variants.is_empty() {
                    println!("No skills found.");
                } else {
                    println!(
                        "{:<12} {:<24} {:>5}  {:>9}  {}",
                        "STATUS", "SKILL NAME", "USES", "SUCCESS", "TAGS"
                    );
                    for v in &variants {
                        let success_str = format!("{}/{}", v.success_count, v.use_count);
                        let tags = v.context_tags.join(", ");
                        println!(
                            "{:<12} {:<24} {:>5}  {:>9}  {}",
                            v.status, v.skill_name, v.use_count, success_str, tags
                        );
                    }
                    println!("\n{} skill(s) shown.", variants.len());
                }
            }
            SkillAction::Inspect { name } => {
                let (variant, content) = client::send_skill_inspect(&name).await?;
                if let Some(v) = variant {
                    println!("Skill:       {}", v.skill_name);
                    println!("Variant:     {} ({})", v.variant_name, v.variant_id);
                    println!("Status:      {}", v.status);
                    println!("Path:        {}", v.relative_path);
                    println!(
                        "Usage:       {} uses ({} success, {} failure)",
                        v.use_count, v.success_count, v.failure_count
                    );
                    if !v.context_tags.is_empty() {
                        println!("Tags:        {}", v.context_tags.join(", "));
                    }
                    if let Some(c) = content {
                        println!("\n--- SKILL.md ---\n{}", c);
                    }
                } else {
                    eprintln!("Skill not found: {}", name);
                }
            }
            SkillAction::Reject { name } => {
                let (success, message) = client::send_skill_reject(&name).await?;
                if success {
                    println!("{}", message);
                } else {
                    eprintln!("{}", message);
                }
            }
            SkillAction::Promote { name, to } => {
                let (success, message) = client::send_skill_promote(&name, &to).await?;
                if success {
                    println!("{}", message);
                } else {
                    eprintln!("{}", message);
                }
            }
            SkillAction::Search { query } => {
                let entries = client::send_skill_search(&query).await?;
                if entries.is_empty() {
                    println!("No community skills found for '{}'.", query);
                } else {
                    println!(
                        "{:<10} {:<24} {:>6} {:>8} {:<10} {}",
                        "VERIFIED", "NAME", "USES", "SUCCESS", "PUBLISHER", "DESCRIPTION"
                    );
                    for entry in &entries {
                        let verified = if entry.publisher_verified { "✓" } else { "-" };
                        let success = format!("{:.0}%", entry.success_rate * 100.0);
                        let publisher = truncate_for_display(&entry.publisher_id, 8);
                        let description = truncate_for_display(&entry.description, 40);
                        println!(
                            "{:<10} {:<24} {:>6} {:>8} {:<10} {}",
                            verified,
                            truncate_for_display(&entry.name, 24),
                            entry.use_count,
                            success,
                            publisher,
                            description
                        );
                    }
                    println!("\n{} skill(s) found.", entries.len());
                }
            }
            SkillAction::Import { source, force } => {
                let (success, message, variant_id, scan_verdict, findings_count) =
                    client::send_skill_import(&source, force).await?;
                if success {
                    println!(
                        "Imported skill as Draft (variant: {}).",
                        variant_id.unwrap_or_default()
                    );
                    if scan_verdict.as_deref() == Some("warn") {
                        println!(
                            "Note: {} security warning(s) overridden with --force.",
                            findings_count
                        );
                    }
                } else {
                    match scan_verdict.as_deref() {
                        Some("block") => eprintln!("Import blocked: {}", message),
                        Some("warn") => eprintln!("Import requires --force: {}", message),
                        _ => eprintln!("{}", message),
                    }
                    std::process::exit(1);
                }
            }
            SkillAction::Export {
                name,
                format,
                output,
            } => {
                let (success, message, output_path) =
                    client::send_skill_export(&name, &format, &output).await?;
                if success {
                    println!("Exported to: {}", output_path.unwrap_or_default());
                } else {
                    eprintln!("Export failed: {}", message);
                    std::process::exit(1);
                }
            }
            SkillAction::Publish { name } => {
                let (success, message) = client::send_skill_publish(&name).await?;
                if success {
                    println!("{}", message);
                } else {
                    eprintln!("Publish failed: {}", message);
                    std::process::exit(1);
                }
            }
        },

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

            // Parse and display provider health
            if let Ok(providers) =
                serde_json::from_str::<serde_json::Value>(&status.provider_health_json)
            {
                if let Some(obj) = providers.as_object() {
                    if !obj.is_empty() {
                        println!("\nProviders:");
                        for (name, info) in obj {
                            let can_exec = info
                                .get("can_execute")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(true);
                            let trips = info
                                .get("trip_count")
                                .and_then(|v| v.as_u64())
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

            // Parse and display gateway statuses
            if let Ok(gateways) =
                serde_json::from_str::<serde_json::Value>(&status.gateway_statuses_json)
            {
                if let Some(obj) = gateways.as_object() {
                    if !obj.is_empty() {
                        println!("\nGateways:");
                        for (platform, info) in obj {
                            let gw_status = info
                                .get("status")
                                .and_then(|v| v.as_str())
                                .unwrap_or("unknown");
                            println!("  {} - {}", platform, gw_status);
                        }
                    }
                }
            }

            // Parse and display recent actions
            if let Ok(actions) =
                serde_json::from_str::<Vec<serde_json::Value>>(&status.recent_actions_json)
            {
                if !actions.is_empty() {
                    println!("\nRecent Actions:");
                    for a in actions.iter().take(5) {
                        let action_type =
                            a.get("action_type").and_then(|v| v.as_str()).unwrap_or("");
                        let summary =
                            a.get("summary").and_then(|v| v.as_str()).unwrap_or("");
                        let ts = a.get("timestamp").and_then(|v| v.as_i64()).unwrap_or(0);
                        println!(
                            "  {} [{}] {}",
                            format_timestamp(ts),
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
                let mut pairs: Vec<(String, String)> = Vec::new();
                flatten_json("", &config, &mut pairs);
                if pairs.is_empty() {
                    println!("No configuration found.");
                } else {
                    for (key, value) in &pairs {
                        println!("{} = {}", key, value);
                    }
                }
            }
            SettingsAction::Get { key } => {
                let config = client::send_config_get().await?;
                let value = resolve_dot_path(&config, &key);
                match value {
                    Some(v) => {
                        if is_sensitive_key(&key) {
                            println!("***");
                        } else {
                            match v.as_str() {
                                Some(s) => println!("{}", s),
                                None => println!(
                                    "{}",
                                    serde_json::to_string_pretty(v).unwrap_or_default()
                                ),
                            }
                        }
                    }
                    None => {
                        eprintln!("Key not found: {}", key);
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
                println!("{} = {}", key, value);
            }
        },

        Commands::Scrub { text } => {
            let input = if let Some(t) = text {
                t
            } else {
                use std::io::Read;
                let mut buf = String::new();
                std::io::stdin().read_to_string(&mut buf)?;
                buf
            };
            let result = client::scrub_text(input).await?;
            print!("{result}");
        }

        Commands::Setup => {
            setup_wizard::run_setup_wizard().await?;
            println!("\nSetup complete! Run 'tamux' again to start.");
        }

        Commands::Ping => {
            client::ping().await?;
            println!("Daemon is alive (pong).");
        }

        Commands::StartDaemon => {
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

            cmd.spawn()?;
            println!("Daemon started.");
        }

        Commands::Plugin { action } => match action {
            PluginAction::Add { source } => {
                println!("Installing plugin from '{}'...", source);

                // Step 1: Install files to disk
                let (dir_name, source_label) = plugins::install_plugin_v2(&source)?;
                println!("Files installed to ~/.tamux/plugins/{}/", dir_name);

                // Step 2: Register with daemon via IPC
                match client::send_plugin_install(&dir_name, &source_label).await {
                    Ok((true, message)) => {
                        println!("{}", message);
                    }
                    Ok((false, message)) => {
                        // Registration failed (e.g., conflict). Clean up files.
                        eprintln!("Registration failed: {}", message);
                        let _ = plugins::remove_plugin_files(&dir_name);
                        eprintln!("Cleaned up plugin files.");
                        std::process::exit(1);
                    }
                    Err(e) => {
                        // Daemon not reachable. Files are installed, warn user.
                        eprintln!(
                            "Warning: Could not register with daemon ({}). Plugin files installed. Start the daemon to activate.",
                            e
                        );
                    }
                }
            }
            PluginAction::Remove { name } => {
                // Step 1: Deregister from daemon via IPC (per D-06: daemon first, then files)
                match client::send_plugin_uninstall(&name).await {
                    Ok((true, message)) => {
                        println!("{}", message);
                    }
                    Ok((false, message)) => {
                        eprintln!("Warning: {}", message);
                    }
                    Err(e) => {
                        eprintln!(
                            "Warning: Could not reach daemon ({}). Removing files only.",
                            e
                        );
                    }
                }

                // Step 2: Remove files from disk (per D-06)
                plugins::remove_plugin_files(&name)?;
                println!("Plugin '{}' removed.", name);
            }
            PluginAction::Ls => {
                // Per D-08/INST-05: table format with name | version | enabled | auth | source
                match client::send_plugin_list().await {
                    Ok(plugins) => {
                        if plugins.is_empty() {
                            println!("No plugins installed.");
                        } else {
                            println!(
                                "{:<24} {:>8} {:>8} {:>6} {}",
                                "NAME", "VERSION", "ENABLED", "AUTH", "SOURCE"
                            );
                            for p in &plugins {
                                let auth_status = if p.has_auth { "yes" } else { "-" };
                                let enabled = if p.enabled { "yes" } else { "no" };
                                println!(
                                    "{:<24} {:>8} {:>8} {:>6} {}",
                                    truncate_for_display(&p.name, 24),
                                    truncate_for_display(&p.version, 8),
                                    enabled,
                                    auth_status,
                                    truncate_for_display(&p.install_source, 40),
                                );
                            }
                            println!("\n{} plugin(s) installed.", plugins.len());
                        }
                    }
                    Err(e) => {
                        eprintln!("Could not reach daemon: {}", e);
                        std::process::exit(1);
                    }
                }
            }
            PluginAction::Enable { name } => {
                let (success, message) = client::send_plugin_enable(&name).await?;
                if success {
                    println!("{}", message);
                } else {
                    eprintln!("{}", message);
                    std::process::exit(1);
                }
            }
            PluginAction::Disable { name } => {
                let (success, message) = client::send_plugin_disable(&name).await?;
                if success {
                    println!("{}", message);
                } else {
                    eprintln!("{}", message);
                    std::process::exit(1);
                }
            }
        },

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
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Audit helpers
// ---------------------------------------------------------------------------

/// Parse a human duration suffix ("1h", "24h", "7d") into an epoch-millis
/// timestamp representing `now - duration`.
fn parse_duration_ago(s: &str) -> Option<u64> {
    let s = s.trim();
    let (num_str, multiplier_ms) = if let Some(n) = s.strip_suffix('d') {
        (n, 86_400_000u64)
    } else if let Some(n) = s.strip_suffix('h') {
        (n, 3_600_000u64)
    } else if let Some(n) = s.strip_suffix('m') {
        (n, 60_000u64)
    } else {
        return None;
    };
    let num: u64 = num_str.parse().ok()?;
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_millis() as u64;
    Some(now_ms.saturating_sub(num * multiplier_ms))
}

/// Format an epoch-millis timestamp as `YYYY-MM-DD HH:MM` (UTC).
fn format_timestamp(epoch_ms: i64) -> String {
    let secs = epoch_ms / 1000;
    // Use UNIX_EPOCH + Duration to get a SystemTime, then format via
    // the standard library's time formatting capability.
    let duration = std::time::Duration::from_secs(secs.unsigned_abs());
    let system_time = if secs >= 0 {
        std::time::UNIX_EPOCH + duration
    } else {
        std::time::UNIX_EPOCH - duration
    };
    // humantime provides a human-readable format
    let formatted = humantime::format_rfc3339_seconds(system_time).to_string();
    // RFC3339 is like "2026-03-23T14:30:00Z" — extract "2026-03-23 14:30"
    if formatted.len() >= 16 {
        let date_part = &formatted[..10];
        let time_part = &formatted[11..16];
        format!("{} {}", date_part, time_part)
    } else {
        formatted
    }
}

/// Print a single audit row in the CLI table format.
fn print_audit_row(entry: &amux_protocol::AuditEntryPublic) {
    let ts = format_timestamp(entry.timestamp);
    let confidence_tag = match (&entry.confidence_band, entry.confidence) {
        (Some(band), Some(pct)) if band != "confident" => {
            format!(" [{} {}%]", band, (pct * 100.0) as u32)
        }
        _ => String::new(),
    };
    println!(
        "{} [{}] [{}]{} {}",
        entry.id, ts, entry.action_type, confidence_tag, entry.summary
    );
}

/// Print full detail for a single audit entry.
fn print_audit_detail(entry: &amux_protocol::AuditEntryPublic) {
    let ts = format_timestamp(entry.timestamp);
    println!("ID:          {}", entry.id);
    println!("Time:        {}", ts);
    println!("Type:        {}", entry.action_type);
    println!("Summary:     {}", entry.summary);
    println!(
        "Explanation: {}",
        entry.explanation.as_deref().unwrap_or("N/A")
    );
    match (&entry.confidence_band, entry.confidence) {
        (Some(band), Some(pct)) => {
            println!("Confidence:  {} ({}%)", band, (pct * 100.0) as u32);
        }
        _ => {
            println!("Confidence:  N/A");
        }
    }
    println!(
        "Trace:       {}",
        entry.causal_trace_id.as_deref().unwrap_or("N/A")
    );
    println!(
        "Thread:      {}",
        entry.thread_id.as_deref().unwrap_or("N/A")
    );
    if let Some(goal) = &entry.goal_run_id {
        println!("Goal Run:    {}", goal);
    }
    if let Some(task) = &entry.task_id {
        println!("Task:        {}", task);
    }
}

// ---------------------------------------------------------------------------
// Settings helpers
// ---------------------------------------------------------------------------

/// Flatten a JSON value into dot-notation key=value pairs.
fn flatten_json(prefix: &str, value: &serde_json::Value, out: &mut Vec<(String, String)>) {
    match value {
        serde_json::Value::Object(map) => {
            for (k, v) in map {
                let full_key = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{}.{}", prefix, k)
                };
                flatten_json(&full_key, v, out);
            }
        }
        serde_json::Value::Null => {
            // Skip null values
        }
        serde_json::Value::String(s) => {
            let display = if is_sensitive_key(prefix) {
                "***".to_string()
            } else {
                s.clone()
            };
            out.push((prefix.to_string(), display));
        }
        other => {
            let display = if is_sensitive_key(prefix) {
                "***".to_string()
            } else {
                other.to_string()
            };
            out.push((prefix.to_string(), display));
        }
    }
}

/// Resolve a dot-notation path to a JSON value reference.
fn resolve_dot_path<'a>(value: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    let mut current = value;
    for segment in path.split('.') {
        current = current.get(segment)?;
    }
    Some(current)
}

/// Check if a key name refers to a sensitive value that should be redacted.
fn is_sensitive_key(key: &str) -> bool {
    let lower = key.to_lowercase();
    lower.contains("api_key") || lower.contains("token") || lower.contains("secret")
}
