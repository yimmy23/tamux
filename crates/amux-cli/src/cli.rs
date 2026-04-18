use clap::{Parser, Subcommand};

fn parse_page(value: &str) -> Result<usize, String> {
    let page = value
        .parse::<usize>()
        .map_err(|_| "page must be a positive integer".to_string())?;
    if page == 0 {
        return Err("page must be at least 1".to_string());
    }
    Ok(page)
}

fn parse_list_limit(value: &str) -> Result<usize, String> {
    let limit = value
        .parse::<usize>()
        .map_err(|_| "limit must be a positive integer".to_string())?;
    if limit == 0 {
        return Err("limit must be at least 1".to_string());
    }
    if limit > 20 {
        return Err("limit must be 20 or less".to_string());
    }
    Ok(limit)
}

#[derive(Parser)]
#[command(
    name = "tamux",
    about = "Tamux Agent and Terminal Agentic Multiplexer CLI"
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Option<Commands>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Commands {
    /// Launch the terminal UI.
    Tui,

    /// Launch the desktop GUI (Electron).
    Gui,

    /// Show agent statistics (alias for status).
    Stats,

    /// List all running daemon-owned terminal sessions.
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

    /// Kill a daemon-owned terminal session.
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

    /// Inspect the tools currently available to the daemon agent.
    Tool {
        #[command(subcommand)]
        action: ToolAction,
    },

    /// Show current agent status.
    Status,

    /// Show the assembled system prompt for an agent.
    Prompt {
        /// Inspect a specific agent id or name.
        #[arg(long)]
        agent: Option<String>,
        /// Inspect the WELES prompt.
        #[arg(long, conflicts_with_all = ["agent", "concierge", "rarog"])]
        weles: bool,
        /// Inspect the concierge agent prompt.
        #[arg(long, conflicts_with_all = ["agent", "weles", "rarog"])]
        concierge: bool,
        /// Inspect the Rarog agent prompt.
        #[arg(long, conflicts_with_all = ["agent", "weles", "concierge"])]
        rarog: bool,
        /// Emit the raw daemon payload as JSON.
        #[arg(long)]
        json: bool,
    },

    /// Query the status of an asynchronous daemon operation.
    Operation {
        /// Operation ID returned by the daemon.
        id: String,
        /// Emit the raw daemon payload as JSON.
        #[arg(long)]
        json: bool,
    },

    /// View and modify daemon configuration.
    Settings {
        #[command(subcommand)]
        action: SettingsAction,
    },

    /// Inspect and manage agent threads.
    Thread {
        #[command(subcommand)]
        action: ThreadAction,
    },

    /// Inspect and manage agent goals.
    Goal {
        #[command(subcommand)]
        action: GoalAction,
    },

    /// Send a direct message to svarog or Rarog from the CLI.
    Dm {
        /// Continue a specific thread.
        #[arg(long)]
        thread: Option<String>,
        /// Preferred terminal session hint.
        #[arg(long)]
        session: Option<String>,
        /// Route the message to svarog explicitly.
        #[arg(long = "svarog", alias = "swarog", conflicts_with_all = ["rarog", "main_target", "concierge"])]
        svarog: bool,
        /// Route the message to Rarog explicitly.
        #[arg(long, conflicts_with_all = ["svarog", "main_target", "concierge"])]
        rarog: bool,
        /// Route the message to svarog using the old alias.
        #[arg(long = "main", conflicts_with_all = ["svarog", "rarog", "concierge"])]
        main_target: bool,
        /// Route the message to Rarog instead of svarog.
        #[arg(long, conflicts_with_all = ["svarog", "rarog", "main_target"])]
        concierge: bool,
        /// Emit a JSON object with response and ids.
        #[arg(long)]
        json: bool,
        /// Message content.
        #[arg(required = true)]
        message: Vec<String>,
    },

    /// Run the first-time setup wizard.
    Setup,

    /// Ping the daemon (health check).
    Ping,

    /// Stop tamux background processes.
    Stop,

    /// Restart tamux by stopping background processes and starting the daemon again.
    Restart,

    /// Upgrade tamux to the npm registry's current @latest release.
    Upgrade,

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
pub(crate) enum InstallTarget {
    /// Install a tamux plugin from npm or a local package directory.
    Plugin {
        /// npm package spec or local package directory.
        package: String,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum PluginAction {
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
    /// List available plugin commands. Per PSKL-05.
    Commands,
}

#[derive(Debug, Subcommand)]
pub(crate) enum SkillAction {
    /// List all skills with their maturity status.
    #[command(alias = "ls")]
    List {
        /// Filter by status (draft, testing, active, proven, promoted_to_canonical).
        #[arg(long)]
        status: Option<String>,
        /// Maximum number of entries to show.
        #[arg(long, default_value = "50")]
        limit: usize,
        /// Continue from an earlier page cursor.
        #[arg(long)]
        cursor: Option<String>,
        /// Fetch every page by following cursors until exhaustion.
        #[arg(long)]
        all: bool,
    },
    /// Rank installed skills for a task and suggest the next action.
    Discover {
        /// Task or problem description to match against installed skills.
        query: String,
        /// Optional terminal session UUID for workspace-aware ranking.
        #[arg(long)]
        session: Option<String>,
        /// Maximum number of ranked candidates to show.
        #[arg(long, default_value = "3")]
        limit: usize,
        /// Continue from an earlier discovery page cursor.
        #[arg(long)]
        cursor: Option<String>,
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
pub(crate) enum ToolAction {
    /// List the tools currently available to the daemon agent.
    #[command(alias = "ls")]
    List {
        /// Maximum number of tools to show.
        #[arg(long, default_value = "50")]
        limit: usize,
        /// Zero-based pagination offset.
        #[arg(long, default_value = "0")]
        offset: usize,
        /// Emit raw JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },
    /// Search the currently available tool catalog.
    Search {
        /// What capability or action to look for.
        query: String,
        /// Maximum number of matches to show.
        #[arg(long, default_value = "20")]
        limit: usize,
        /// Zero-based pagination offset.
        #[arg(long, default_value = "0")]
        offset: usize,
        /// Emit raw JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum SettingsAction {
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

#[derive(Debug, Subcommand)]
pub(crate) enum ThreadAction {
    /// List recent agent threads.
    #[command(alias = "ls")]
    List {
        /// 1-based page number.
        #[arg(long, default_value_t = 1, value_parser = parse_page)]
        page: usize,
        /// Maximum items per page (capped at 20).
        #[arg(long, default_value_t = 20, value_parser = parse_list_limit)]
        limit: usize,
        /// Emit raw JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },
    /// Show one agent thread and its messages.
    Get {
        /// Thread ID to fetch.
        thread_id: String,
        /// Emit raw JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },
    /// Stop the active stream for one agent thread.
    Stop {
        /// Thread ID to stop.
        thread_id: String,
    },
    /// Resume one agent thread by retrying execution now.
    Resume {
        /// Thread ID to resume.
        thread_id: String,
    },
    /// Delete one agent thread.
    Delete {
        /// Thread ID to delete.
        thread_id: String,
        /// Skip the confirmation prompt.
        #[arg(long)]
        yes: bool,
    },
}

#[derive(Debug, Subcommand)]
pub(crate) enum GoalAction {
    /// List recent goal runs.
    #[command(alias = "ls")]
    List {
        /// 1-based page number.
        #[arg(long, default_value_t = 1, value_parser = parse_page)]
        page: usize,
        /// Maximum items per page (capped at 20).
        #[arg(long, default_value_t = 20, value_parser = parse_list_limit)]
        limit: usize,
        /// Emit raw JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },
    /// Show one goal run and its current state.
    Get {
        /// Goal run ID to fetch.
        goal_run_id: String,
        /// Emit raw JSON instead of human-readable output.
        #[arg(long)]
        json: bool,
    },
    /// Stop one goal run by pausing it.
    Stop {
        /// Goal run ID to stop.
        goal_run_id: String,
    },
    /// Resume one paused goal run.
    Resume {
        /// Goal run ID to resume.
        goal_run_id: String,
    },
    /// Delete one goal run.
    Delete {
        /// Goal run ID to delete.
        goal_run_id: String,
        /// Skip the confirmation prompt.
        #[arg(long)]
        yes: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::{Cli, Commands, GoalAction, SkillAction, ThreadAction, ToolAction};
    use clap::{CommandFactory, Parser};

    #[test]
    fn skill_discover_subcommand_parses_query() {
        let cli = Cli::try_parse_from(["tamux", "skill", "discover", "debug panic"])
            .expect("skill discover subcommand should parse");
        match cli.command {
            Some(Commands::Skill {
                action:
                    SkillAction::Discover {
                        query,
                        session,
                        limit,
                        ..
                    },
            }) => {
                assert_eq!(query, "debug panic");
                assert_eq!(session, None);
                assert_eq!(limit, 3);
            }
            other => panic!("parsed unexpected command: {other:?}"),
        }
    }

    #[test]
    fn thread_list_subcommand_parses() {
        let cli = Cli::try_parse_from([
            "tamux", "thread", "list", "--page", "2", "--limit", "15", "--json",
        ])
        .expect("thread list subcommand should parse");
        match cli.command {
            Some(Commands::Thread {
                action: ThreadAction::List { page, limit, json },
            }) => {
                assert_eq!(page, 2);
                assert_eq!(limit, 15);
                assert!(json);
            }
            other => panic!("expected thread list command, got {other:?}"),
        }
    }

    #[test]
    fn tool_list_subcommand_parses() {
        let cli = Cli::try_parse_from(["tamux", "tool", "list", "--limit", "25", "--offset", "5"])
            .expect("tool list subcommand should parse");
        match cli.command {
            Some(Commands::Tool {
                action:
                    ToolAction::List {
                        limit,
                        offset,
                        json,
                    },
            }) => {
                assert_eq!(limit, 25);
                assert_eq!(offset, 5);
                assert!(!json);
            }
            other => panic!("expected tool list command, got {other:?}"),
        }
    }

    #[test]
    fn tool_search_subcommand_parses() {
        let cli = Cli::try_parse_from(["tamux", "tool", "search", "discover skill", "--json"])
            .expect("tool search subcommand should parse");
        match cli.command {
            Some(Commands::Tool {
                action:
                    ToolAction::Search {
                        query,
                        limit,
                        offset,
                        json,
                    },
            }) => {
                assert_eq!(query, "discover skill");
                assert_eq!(limit, 20);
                assert_eq!(offset, 0);
                assert!(json);
            }
            other => panic!("expected tool search command, got {other:?}"),
        }
    }

    #[test]
    fn thread_get_subcommand_parses() {
        let cli = Cli::try_parse_from(["tamux", "thread", "get", "thread-123"])
            .expect("thread get subcommand should parse");
        match cli.command {
            Some(Commands::Thread {
                action: ThreadAction::Get { thread_id, json },
            }) => {
                assert_eq!(thread_id, "thread-123");
                assert!(!json);
            }
            other => panic!("expected thread get command, got {other:?}"),
        }
    }

    #[test]
    fn thread_delete_subcommand_parses() {
        let cli = Cli::try_parse_from(["tamux", "thread", "delete", "thread-123", "--yes"])
            .expect("thread delete subcommand should parse");
        match cli.command {
            Some(Commands::Thread {
                action: ThreadAction::Delete { thread_id, yes },
            }) => {
                assert_eq!(thread_id, "thread-123");
                assert!(yes);
            }
            other => panic!("expected thread delete command, got {other:?}"),
        }
    }

    #[test]
    fn thread_stop_subcommand_parses() {
        let cli = Cli::try_parse_from(["tamux", "thread", "stop", "thread-123"])
            .expect("thread stop subcommand should parse");
        match cli.command {
            Some(Commands::Thread {
                action: ThreadAction::Stop { thread_id },
            }) => {
                assert_eq!(thread_id, "thread-123");
            }
            other => panic!("expected thread stop command, got {other:?}"),
        }
    }

    #[test]
    fn thread_resume_subcommand_parses() {
        let cli = Cli::try_parse_from(["tamux", "thread", "resume", "thread-123"])
            .expect("thread resume subcommand should parse");
        match cli.command {
            Some(Commands::Thread {
                action: ThreadAction::Resume { thread_id },
            }) => {
                assert_eq!(thread_id, "thread-123");
            }
            other => panic!("expected thread resume command, got {other:?}"),
        }
    }

    #[test]
    fn goal_list_subcommand_parses() {
        let cli = Cli::try_parse_from([
            "tamux", "goal", "list", "--page", "3", "--limit", "10", "--json",
        ])
        .expect("goal list subcommand should parse");
        match cli.command {
            Some(Commands::Goal {
                action: GoalAction::List { page, limit, json },
            }) => {
                assert_eq!(page, 3);
                assert_eq!(limit, 10);
                assert!(json);
            }
            other => panic!("expected goal list command, got {other:?}"),
        }
    }

    #[test]
    fn list_subcommands_default_to_first_page_with_limit_20() {
        let thread_cli =
            Cli::try_parse_from(["tamux", "thread", "list"]).expect("thread list should parse");
        match thread_cli.command {
            Some(Commands::Thread {
                action: ThreadAction::List { page, limit, json },
            }) => {
                assert_eq!(page, 1);
                assert_eq!(limit, 20);
                assert!(!json);
            }
            other => panic!("expected thread list command, got {other:?}"),
        }

        let goal_cli =
            Cli::try_parse_from(["tamux", "goal", "list"]).expect("goal list should parse");
        match goal_cli.command {
            Some(Commands::Goal {
                action: GoalAction::List { page, limit, json },
            }) => {
                assert_eq!(page, 1);
                assert_eq!(limit, 20);
                assert!(!json);
            }
            other => panic!("expected goal list command, got {other:?}"),
        }
    }

    #[test]
    fn goal_get_subcommand_parses() {
        let cli = Cli::try_parse_from(["tamux", "goal", "get", "goal-123"])
            .expect("goal get subcommand should parse");
        match cli.command {
            Some(Commands::Goal {
                action: GoalAction::Get { goal_run_id, json },
            }) => {
                assert_eq!(goal_run_id, "goal-123");
                assert!(!json);
            }
            other => panic!("expected goal get command, got {other:?}"),
        }
    }

    #[test]
    fn goal_stop_subcommand_parses() {
        let cli = Cli::try_parse_from(["tamux", "goal", "stop", "goal-123"])
            .expect("goal stop subcommand should parse");
        match cli.command {
            Some(Commands::Goal {
                action: GoalAction::Stop { goal_run_id },
            }) => {
                assert_eq!(goal_run_id, "goal-123");
            }
            other => panic!("expected goal stop command, got {other:?}"),
        }
    }

    #[test]
    fn goal_resume_subcommand_parses() {
        let cli = Cli::try_parse_from(["tamux", "goal", "resume", "goal-123"])
            .expect("goal resume subcommand should parse");
        match cli.command {
            Some(Commands::Goal {
                action: GoalAction::Resume { goal_run_id },
            }) => {
                assert_eq!(goal_run_id, "goal-123");
            }
            other => panic!("expected goal resume command, got {other:?}"),
        }
    }

    #[test]
    fn goal_delete_subcommand_parses() {
        let cli = Cli::try_parse_from(["tamux", "goal", "delete", "goal-123", "--yes"])
            .expect("goal delete subcommand should parse");
        match cli.command {
            Some(Commands::Goal {
                action: GoalAction::Delete { goal_run_id, yes },
            }) => {
                assert_eq!(goal_run_id, "goal-123");
                assert!(yes);
            }
            other => panic!("expected goal delete command, got {other:?}"),
        }
    }

    #[test]
    fn stop_subcommand_parses() {
        let cli = Cli::try_parse_from(["tamux", "stop"]).expect("stop subcommand should parse");
        match cli.command {
            Some(Commands::Stop) => {}
            other => panic!("expected stop command, got {other:?}"),
        }
    }

    #[test]
    fn restart_subcommand_parses() {
        let cli =
            Cli::try_parse_from(["tamux", "restart"]).expect("restart subcommand should parse");
        match cli.command {
            Some(Commands::Restart) => {}
            other => panic!("expected restart command, got {other:?}"),
        }
    }

    #[test]
    fn skill_discover_subcommand_accepts_explicit_session() {
        let cli = Cli::try_parse_from([
            "tamux",
            "skill",
            "discover",
            "--session",
            "550e8400-e29b-41d4-a716-446655440000",
            "debug panic",
        ])
        .expect("skill discover subcommand should parse explicit session");

        match cli.command {
            Some(Commands::Skill {
                action:
                    SkillAction::Discover {
                        query,
                        session,
                        limit,
                        ..
                    },
            }) => {
                assert_eq!(query, "debug panic");
                assert_eq!(
                    session.as_deref(),
                    Some("550e8400-e29b-41d4-a716-446655440000")
                );
                assert_eq!(limit, 3);
            }
            other => panic!("parsed unexpected command: {other:?}"),
        }
    }

    #[test]
    fn prompt_help_text_describes_concierge_and_rarog_distinctly() {
        let command = Cli::command();
        let prompt = command
            .find_subcommand("prompt")
            .expect("prompt subcommand should exist");
        let concierge_help = prompt
            .get_arguments()
            .find(|arg| arg.get_id().as_str() == "concierge")
            .and_then(|arg| arg.get_help())
            .map(|help| help.to_string())
            .expect("concierge help should exist");
        let rarog_help = prompt
            .get_arguments()
            .find(|arg| arg.get_id().as_str() == "rarog")
            .and_then(|arg| arg.get_help())
            .map(|help| help.to_string())
            .expect("rarog help should exist");

        assert_eq!(concierge_help, "Inspect the concierge agent prompt");
        assert_eq!(rarog_help, "Inspect the Rarog agent prompt");
    }
}
