mod client;
mod plugins;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "tamux", about = "tamux terminal multiplexer CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// List all running sessions.
    #[command(alias = "ls")]
    List,

    /// Spawn a new terminal session.
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

    /// Ping the daemon (health check).
    Ping,

    /// Start the daemon (if not already running).
    #[command(name = "daemon")]
    StartDaemon,

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
        Commands::Bridge { .. } => "tamux-bridge.log",
        _ => "tamux-cli.log",
    };
    let _log_guard = init_logging(log_file_name)?;
    tracing::info!(command = ?cli.command, "tamux-cli starting");

    match cli.command {
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
        "[{}] [{}]{} {}",
        ts, entry.action_type, confidence_tag, entry.summary
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
