use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "tamux", about = "tamux terminal multiplexer CLI")]
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

    /// Show the assembled system prompt for an agent.
    Prompt {
        /// Inspect a specific agent id or name.
        #[arg(long)]
        agent: Option<String>,
        /// Inspect the WELES prompt.
        #[arg(long, conflicts_with_all = ["agent", "concierge", "rarog"])]
        weles: bool,
        /// Inspect the Rarog concierge prompt.
        #[arg(long, conflicts_with_all = ["agent", "weles", "rarog"])]
        concierge: bool,
        /// Inspect the Rarog concierge prompt.
        #[arg(long, conflicts_with_all = ["agent", "weles", "concierge"])]
        rarog: bool,
        /// Emit the raw daemon payload as JSON.
        #[arg(long)]
        json: bool,
    },

    /// View and modify daemon configuration.
    Settings {
        #[command(subcommand)]
        action: SettingsAction,
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

#[cfg(test)]
mod tests {
    use super::{Cli, Commands, SkillAction};
    use clap::Parser;

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
}
