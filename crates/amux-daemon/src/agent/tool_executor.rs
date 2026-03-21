//! Tool execution for the agent engine.
//!
//! Maps tool calls to daemon infrastructure. Tools that require frontend
//! state (workspace/pane/browser) are not available in daemon mode — only
//! tools that can execute headlessly are included here.

use std::sync::Arc;

use amux_protocol::{
    DaemonMessage, ManagedCommandRequest, ManagedCommandSource, SecurityLevel, SessionId,
};
use anyhow::{Context, Result};
use base64::Engine;
use tokio::sync::broadcast;

use crate::session_manager::SessionManager;

use super::types::{
    AgentConfig, AgentEvent, NotificationSeverity, TodoItem, TodoStatus, ToolCall, ToolDefinition,
    ToolFunctionDef, ToolPendingApproval, ToolResult,
};
use super::AgentEngine;

const ONECONTEXT_TOOL_QUERY_MAX_CHARS: usize = 300;
const ONECONTEXT_TOOL_OUTPUT_MAX_CHARS: usize = 12_000;
const ONECONTEXT_TOOL_TIMEOUT_SECS: u64 = 8;

// ---------------------------------------------------------------------------
// Tool definitions (OpenAI function calling schema)
// ---------------------------------------------------------------------------

pub fn get_available_tools(config: &AgentConfig) -> Vec<ToolDefinition> {
    let mut tools = Vec::new();

    if config.tools.bash {
        tools.push(tool_def(
            "bash_command",
            "Execute a shell command through a tamux-managed terminal session. This does not run as a daemon-native headless subprocess.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "Shell command to execute in a managed terminal session" },
                    "rationale": { "type": "string", "description": "Why this command should run" },
                    "session": { "type": "string", "description": "Optional terminal session ID or unique substring" },
                    "cwd": { "type": "string", "description": "Optional working directory" },
                    "allow_network": { "type": "boolean", "description": "Whether network access is expected" },
                    "sandbox_enabled": { "type": "boolean", "description": "Whether sandboxing should be requested" },
                    "security_level": { "type": "string", "enum": ["highest", "moderate", "lowest", "yolo"], "description": "Approval strictness level" },
                    "language_hint": { "type": "string", "description": "Optional language hint for validation" },
                    "wait_for_completion": { "type": "boolean", "description": "Wait for completion and return exit status/output summary (default: true)" },
                    "timeout_seconds": { "type": "integer", "description": "Wait timeout (default: 30, max: 600). If you set a value above 600, the command auto-runs in background with a monitor that notifies you when it completes." }
                },
                "required": ["command"]
            }),
        ));
    }

    if config.tools.file_operations {
        tools.push(tool_def(
            "list_files",
            "List files and directories at a given path through a tamux-managed terminal session.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Directory path to list" },
                    "session": { "type": "string", "description": "Optional terminal session ID or unique substring" },
                    "timeout_seconds": { "type": "integer", "description": "Max time to wait for completion (default: 30, max: 600)" }
                },
                "required": ["path"]
            }),
        ));

        tools.push(tool_def(
            "read_file",
            "Read the contents of a file.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path to read" },
                    "max_lines": { "type": "integer", "description": "Max lines to read (default: 200)" }
                },
                "required": ["path"]
            }),
        ));

        tools.push(tool_def(
            "write_file",
            "Write content to a file through an existing terminal session managed by tamux. This runs in the terminal's environment, not in a daemon-native filesystem context.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path to write" },
                    "content": { "type": "string", "description": "File content to write" },
                    "session": { "type": "string", "description": "Optional terminal session ID or unique substring" },
                    "timeout_seconds": { "type": "integer", "description": "Max time to wait for completion (default: 30, max: 600)" }
                },
                "required": ["path", "content"]
            }),
        ));

        tools.push(tool_def(
            "create_file",
            "Create a new file directly from the daemon filesystem context. Fails if the file already exists unless overwrite=true.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path to create" },
                    "content": { "type": "string", "description": "Initial file content" },
                    "overwrite": { "type": "boolean", "description": "Allow replacing an existing file (default: false)" }
                },
                "required": ["path", "content"]
            }),
        ));

        tools.push(tool_def(
            "append_to_file",
            "Append text to the end of an existing file without rewriting the whole file.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path to append to" },
                    "content": { "type": "string", "description": "Text to append" },
                    "create_if_missing": { "type": "boolean", "description": "Create the file if it does not exist (default: false)" }
                },
                "required": ["path", "content"]
            }),
        ));

        tools.push(tool_def(
            "replace_in_file",
            "Replace a specific fragment inside a file. Use this for targeted edits instead of rewriting the full file.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path to edit" },
                    "old_text": { "type": "string", "description": "Exact text to replace" },
                    "new_text": { "type": "string", "description": "Replacement text" },
                    "replace_all": { "type": "boolean", "description": "Replace every occurrence instead of exactly one (default: false)" }
                },
                "required": ["path", "old_text", "new_text"]
            }),
        ));

        tools.push(tool_def(
            "apply_file_patch",
            "Apply one or more exact text replacements to a file in order. Use this for multi-hunk targeted edits.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path to patch" },
                    "edits": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "old_text": { "type": "string", "description": "Exact existing text to replace" },
                                "new_text": { "type": "string", "description": "Replacement text" },
                                "replace_all": { "type": "boolean", "description": "Replace all occurrences for this edit (default: false)" }
                            },
                            "required": ["old_text", "new_text"]
                        }
                    }
                },
                "required": ["path", "edits"]
            }),
        ));

        tools.push(tool_def(
            "search_files",
            "Search for a pattern in files using grep. Returns matching lines with file paths and line numbers.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "Regex pattern to search for" },
                    "path": { "type": "string", "description": "Directory to search in (default: current directory)" },
                    "file_pattern": { "type": "string", "description": "Glob pattern to filter files (e.g. '*.rs', '*.ts')" },
                    "max_results": { "type": "integer", "description": "Max results to return (default: 50)" }
                },
                "required": ["pattern"]
            }),
        ));
    }

    if config.tools.system_info {
        tools.push(tool_def(
            "get_system_info",
            "Get system information: CPU, memory, disk, load average, hostname.",
            serde_json::json!({ "type": "object", "properties": {} }),
        ));

        tools.push(tool_def(
            "list_processes",
            "List running processes sorted by CPU usage.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "limit": { "type": "integer", "description": "Max processes to show (default: 20)" }
                }
            }),
        ));
    }

    // History search (daemon has SQLite FTS)
    tools.push(tool_def(
        "search_history",
        "Search command execution history. Returns matching commands with timestamps and exit codes.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search query" },
                "limit": { "type": "integer", "description": "Max results (default: 20)" }
            },
            "required": ["query"]
        }),
    ));
    tools.push(tool_def(
        "onecontext_search",
        "Search Aline OneContext history for related prior sessions/events/turns.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search query" },
                "scope": { "type": "string", "enum": ["session", "event", "turn"], "description": "Search scope (default: session)" },
                "no_regex": { "type": "boolean", "description": "Treat query as plain text (default: true)" }
            },
            "required": ["query"]
        }),
    ));

    // Daemon session management
    tools.push(tool_def(
        "list_sessions",
        "List all active terminal sessions managed by the daemon.",
        serde_json::json!({ "type": "object", "properties": {} }),
    ));

    // Always available: notify and memory tools
    tools.push(tool_def(
        "notify_user",
        "Send a proactive notification to the user via configured channels.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "title": { "type": "string", "description": "Notification title" },
                "message": { "type": "string", "description": "Notification body" },
                "severity": { "type": "string", "enum": ["info", "warning", "alert", "error"] }
            },
            "required": ["title", "message"]
        }),
    ));

    tools.push(tool_def(
        "update_todo",
        "Replace the current todo list for this conversation. Use this to enter plan mode for non-trivial work and keep the list current as execution progresses.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "items": {
                    "type": "array",
                    "description": "Ordered todo items representing the current plan",
                    "items": {
                        "type": "object",
                        "properties": {
                            "content": { "type": "string", "description": "Short todo item text" },
                            "status": { "type": "string", "enum": ["pending", "in_progress", "completed", "blocked"], "description": "Current execution state" },
                            "step_index": { "type": "integer", "description": "Optional goal-run step index for this todo item" }
                        },
                        "required": ["content", "status"]
                    }
                }
            },
            "required": ["items"]
        }),
    ));

    tools.push(tool_def(
        "update_memory",
        "Update your persistent memory with new knowledge. Persists across sessions and restarts.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "content": { "type": "string", "description": "Updated memory content (markdown)" }
            },
            "required": ["content"]
        }),
    ));

    tools.push(tool_def(
        "list_skills",
        "List reusable local skills available to the tamux agent from ~/.tamux/skills (platform dependent).",
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Optional name/path filter for relevant skills" },
                "limit": { "type": "integer", "description": "Max skills to return (default: 20)" }
            }
        }),
    ));

    tools.push(tool_def(
        "read_skill",
        "Read a local skill document before acting. Accepts a skill name, relative path, or generated skill filename.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "skill": { "type": "string", "description": "Skill name, file stem, or relative path under the tamux skills directory" },
                "max_lines": { "type": "integer", "description": "Max lines to read (default: 200)" }
            },
            "required": ["skill"]
        }),
    ));

    if config.tools.web_search {
        tools.push(tool_def(
            "web_search",
            "Search the web and return results. Requires a search API key in config.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query" },
                    "max_results": { "type": "integer", "description": "Max results (default: 5)" }
                },
                "required": ["query"]
            }),
        ));
    }

    if config.tools.web_browse {
        tools.push(tool_def(
            "fetch_url",
            "Fetch a URL and return its text content (HTML stripped to text).",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "URL to fetch" },
                    "max_length": { "type": "integer", "description": "Max characters to return (default: 10000)" }
                },
                "required": ["url"]
            }),
        ));
    }

    if config.tools.gateway_messaging {
        for (name, desc, params) in [
            ("send_slack_message", "Send a message to a Slack channel. If channel is omitted, sends to the default channel from settings (slackChannelFilter).", serde_json::json!({
                "type": "object",
                "properties": {
                    "channel": { "type": "string", "description": "Slack channel name or ID. Optional — uses default from config if omitted." },
                    "message": { "type": "string", "description": "Message text" }
                },
                "required": ["message"]
            })),
            ("send_discord_message", "Send a message to a Discord channel or user. If channel_id and user_id are both omitted, sends to the default channel (discordChannelFilter) or default user DM (discordAllowedUsers) from settings.", serde_json::json!({
                "type": "object",
                "properties": {
                    "channel_id": { "type": "string", "description": "Discord channel ID. Optional — uses default from config if omitted." },
                    "user_id": { "type": "string", "description": "Discord user ID for DM. Optional." },
                    "message": { "type": "string", "description": "Message text" }
                },
                "required": ["message"]
            })),
            ("send_telegram_message", "Send a message to a Telegram chat. If chat_id is omitted, sends to the default chat from settings (telegramAllowedChats).", serde_json::json!({
                "type": "object",
                "properties": {
                    "chat_id": { "type": "string", "description": "Telegram chat ID. Optional — uses default from config if omitted." },
                    "message": { "type": "string", "description": "Message text" }
                },
                "required": ["message"]
            })),
            ("send_whatsapp_message", "Send a message to a WhatsApp contact. If phone is omitted, sends to the default contact from settings (whatsappAllowedContacts).", serde_json::json!({
                "type": "object",
                "properties": {
                    "phone": { "type": "string", "description": "Phone in E.164 format or WhatsApp JID. Optional — uses default from config if omitted." },
                    "message": { "type": "string", "description": "Message text" }
                },
                "required": ["message"]
            })),
        ] {
            tools.push(tool_def(name, desc, params));
        }
    }

    // Terminal pane tools
    tools.push(tool_def(
        "list_terminals",
        "List all open terminal panes with their IDs and names.",
        serde_json::json!({"type":"object","properties":{}}),
    ));
    tools.push(tool_def("read_active_terminal_content", "Read the current terminal buffer content from a pane, or browser panel info. For browser panels, returns URL and title; use include_dom to get page text content.", serde_json::json!({
        "type": "object",
        "properties": {
            "pane": { "type": "string", "description": "Pane ID or name (optional, defaults to active)" },
            "include_dom": { "type": "boolean", "description": "For browser panels: include page DOM text content. Ignored for terminal panes." }
        }
    })));
    tools.push(tool_def("run_terminal_command", "Execute a shell command through a tamux-managed terminal session. This runs in the app's terminal context (not a daemon-native subprocess).", serde_json::json!({
        "type": "object",
        "properties": {
            "command": { "type": "string", "description": "Shell command to execute in a managed terminal session" },
            "rationale": { "type": "string", "description": "Why this command should run" },
            "session": { "type": "string", "description": "Optional terminal session ID or unique substring" },
            "cwd": { "type": "string", "description": "Optional working directory" },
            "allow_network": { "type": "boolean", "description": "Whether network access is expected" },
            "sandbox_enabled": { "type": "boolean", "description": "Whether sandboxing should be requested" },
            "security_level": { "type": "string", "enum": ["highest", "moderate", "lowest", "yolo"], "description": "Approval strictness level" },
            "language_hint": { "type": "string", "description": "Optional language hint for validation" },
            "wait_for_completion": { "type": "boolean", "description": "Wait for completion and return exit status/output summary (default: true)" },
            "timeout_seconds": { "type": "integer", "description": "Wait timeout when wait_for_completion=true (default: 30, max: 600)" }
        },
        "required": ["command"]
    })));
    tools.push(tool_def("execute_managed_command", "Queue a command in a daemon-managed terminal lane. By default this tool waits for completion and returns final status/output tail. If session is omitted, uses the first active terminal session.", serde_json::json!({
        "type": "object",
        "properties": {
            "command": { "type": "string", "description": "Shell command to run in the managed terminal session" },
            "rationale": { "type": "string", "description": "Why this command should run" },
            "session": { "type": "string", "description": "Optional session ID or unique substring" },
            "cwd": { "type": "string", "description": "Optional working directory" },
            "allow_network": { "type": "boolean", "description": "Whether network access is expected" },
            "sandbox_enabled": { "type": "boolean", "description": "Whether sandboxing should be requested" },
            "security_level": { "type": "string", "enum": ["highest", "moderate", "lowest", "yolo"], "description": "Approval strictness level" },
            "language_hint": { "type": "string", "description": "Optional language hint for validation" },
            "wait_for_completion": { "type": "boolean", "description": "Wait for completion and return exit status/output summary (default: true)" },
            "timeout_seconds": { "type": "integer", "description": "Wait timeout when wait_for_completion=true (default: 30, max: 600)" }
        },
        "required": ["command", "rationale"]
    })));
    tools.push(tool_def("allocate_terminal", "Allocate another daemon-managed terminal lane in the same workspace as the current session. Use this when your chosen session is occupied by a blocking or long-running command and you need another terminal to continue working.", serde_json::json!({
        "type": "object",
        "properties": {
            "session": { "type": "string", "description": "Optional source session ID or unique substring. Defaults to the preferred/current session." },
            "pane_name": { "type": "string", "description": "Optional name for the new terminal pane" },
            "cwd": { "type": "string", "description": "Optional working directory hint to show in the workspace metadata" }
        }
    })));
    tools.push(tool_def("spawn_subagent", "Spawn a bounded child task under the current task or thread. Use this to split a large task into parallel subagents with dedicated runtime/session metadata. Keep each child narrowly scoped and monitor it with list_subagents.", serde_json::json!({
        "type": "object",
        "properties": {
            "title": { "type": "string", "description": "Short subagent title" },
            "description": { "type": "string", "description": "Detailed instructions for the child task" },
            "runtime": { "type": "string", "enum": ["daemon", "hermes", "openclaw"], "description": "Preferred runtime for the child agent (default: daemon)" },
            "priority": { "type": "string", "enum": ["low", "normal", "high", "urgent"], "description": "Child task priority" },
            "command": { "type": "string", "description": "Optional preferred entrypoint or command" },
            "session": { "type": "string", "description": "Optional explicit session ID or unique substring. If omitted, tamux allocates a fresh lane in the same workspace when possible." },
            "cwd": { "type": "string", "description": "Optional working directory hint for any newly allocated lane" },
            "dependencies": { "type": "array", "items": { "type": "string" }, "description": "Optional additional task dependencies" }
        },
        "required": ["title", "description"]
    })));
    tools.push(tool_def("list_subagents", "List child tasks spawned under the current parent task or thread, including runtime, status, thread, and session metadata.", serde_json::json!({
        "type": "object",
        "properties": {
            "status": { "type": "string", "enum": ["queued", "in_progress", "awaiting_approval", "blocked", "failed_analyzing", "completed", "failed", "cancelled"], "description": "Optional status filter" },
            "parent_task_id": { "type": "string", "description": "Override parent task scope" },
            "parent_thread_id": { "type": "string", "description": "Override parent thread scope" },
            "limit": { "type": "integer", "description": "Maximum subagents to return (default: 20)" }
        }
    })));
    tools.push(tool_def("enqueue_task", "Create a daemon-managed background task. Use this for work that should run later, survive disconnects, wait on dependencies, or schedule follow-up actions like reminders and gateway messages.", serde_json::json!({
        "type": "object",
        "properties": {
            "title": { "type": "string", "description": "Short task title" },
            "description": { "type": "string", "description": "Detailed task instructions for the daemon agent" },
            "priority": { "type": "string", "enum": ["low", "normal", "high", "urgent"], "description": "Task priority" },
            "command": { "type": "string", "description": "Optional preferred command or entrypoint" },
            "session": { "type": "string", "description": "Optional preferred terminal session ID or substring" },
            "dependencies": { "type": "array", "items": { "type": "string" }, "description": "Task IDs that must complete first" },
            "scheduled_at": { "type": "integer", "description": "Optional Unix timestamp in milliseconds for when the task may start" },
            "schedule_at": { "type": "string", "description": "Optional RFC3339 timestamp for when the task may start" },
            "delay_seconds": { "type": "integer", "description": "Optional relative delay before the task may start" }
        },
        "required": ["description"]
    })));
    tools.push(tool_def("list_tasks", "List daemon-managed background tasks and their status, dependencies, schedule, and recent execution metadata.", serde_json::json!({
        "type": "object",
        "properties": {
            "status": { "type": "string", "enum": ["queued", "in_progress", "awaiting_approval", "blocked", "failed_analyzing", "completed", "failed", "cancelled"] },
            "limit": { "type": "integer", "description": "Maximum number of tasks to return" }
        }
    })));
    tools.push(tool_def(
        "cancel_task",
        "Cancel a queued, blocked, running, approval-pending, or retrying background task by ID.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "task_id": { "type": "string", "description": "Task ID to cancel" }
            },
            "required": ["task_id"]
        }),
    ));
    tools.push(tool_def("type_in_terminal", "Type text into an existing terminal session as raw keyboard input. Use this for: interactive TUI programs (codex, vim, htop), REPLs (python, node), typing commands in running shells, or any program that needs a real TTY. Text and Enter are sent with a small delay between them so TUIs process correctly. You can also send special keys like ctrl+c, escape, tab, arrow keys, etc.", serde_json::json!({
        "type": "object",
        "properties": {
            "text": { "type": "string", "description": "Text to type into the terminal" },
            "press_enter": { "type": "boolean", "description": "Whether to press Enter after typing (default: true)" },
            "key": { "type": "string", "description": "Send a special key instead of text. Options: enter, ctrl+c, ctrl+d, ctrl+z, ctrl+l, ctrl+a, ctrl+e, ctrl+u, ctrl+k, escape, tab, up, down, left, right, backspace, delete, home, end, page_up, page_down. When 'key' is set, 'text' is ignored." },
            "pane": { "type": "string", "description": "Pane ID or name (optional, defaults to first active session)" }
        },
        "required": []
    })));

    // Workspace tools — executed via WorkspaceCommand event on the frontend
    tools.push(tool_def(
        "list_workspaces",
        "List workspaces, surfaces, and panes (with names and IDs).",
        serde_json::json!({"type":"object","properties":{}}),
    ));
    tools.push(tool_def(
        "create_workspace",
        "Create a new workspace and make it active.",
        serde_json::json!({
            "type": "object",
            "properties": { "name": { "type": "string", "description": "Optional workspace name" } }
        }),
    ));
    tools.push(tool_def("set_active_workspace", "Set the active workspace by ID or name.", serde_json::json!({
        "type": "object",
        "properties": { "workspace": { "type": "string", "description": "Workspace ID or name" } },
        "required": ["workspace"]
    })));
    tools.push(tool_def(
        "create_surface",
        "Create a new surface (tab) in a workspace.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "workspace": { "type": "string", "description": "Optional workspace ID or name" },
                "name": { "type": "string", "description": "Optional surface name" }
            }
        }),
    ));
    tools.push(tool_def(
        "set_active_surface",
        "Set active surface by ID or name.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "surface": { "type": "string", "description": "Surface ID or name" },
                "workspace": { "type": "string", "description": "Optional workspace scope" }
            },
            "required": ["surface"]
        }),
    ));
    tools.push(tool_def("split_pane", "Split a pane horizontally or vertically. Works in BSP layout mode. In canvas mode, creates a new panel instead.", serde_json::json!({
        "type": "object",
        "properties": {
            "direction": { "type": "string", "enum": ["horizontal", "vertical"] },
            "pane": { "type": "string", "description": "Optional pane ID or name" },
            "new_pane_name": { "type": "string", "description": "Optional name for new pane" }
        },
        "required": ["direction"]
    })));
    tools.push(tool_def(
        "rename_pane",
        "Rename a pane.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "pane": { "type": "string", "description": "Optional pane ID or name" },
                "name": { "type": "string", "description": "New pane name" }
            },
            "required": ["name"]
        }),
    ));
    tools.push(tool_def("set_layout_preset", "Apply a layout preset to a surface.", serde_json::json!({
        "type": "object",
        "properties": {
            "preset": { "type": "string", "enum": ["single", "2-columns", "3-columns", "grid-2x2", "main-stack"] },
            "surface": { "type": "string", "description": "Optional surface ID or name" },
            "workspace": { "type": "string", "description": "Optional workspace scope" }
        },
        "required": ["preset"]
    })));
    tools.push(tool_def(
        "equalize_layout",
        "Equalize all split ratios in a surface.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "surface": { "type": "string", "description": "Optional surface ID or name" },
                "workspace": { "type": "string", "description": "Optional workspace scope" }
            }
        }),
    ));
    tools.push(tool_def(
        "list_snippets",
        "List saved snippets with names and content previews.",
        serde_json::json!({
            "type": "object",
            "properties": { "owner": { "type": "string", "enum": ["user", "assistant", "both"] } }
        }),
    ));
    tools.push(tool_def(
        "create_snippet",
        "Create a new snippet.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "content": { "type": "string" },
                "category": { "type": "string" },
                "description": { "type": "string" },
                "tags": { "type": "array", "items": { "type": "string" } }
            },
            "required": ["name", "content"]
        }),
    ));
    tools.push(tool_def("run_snippet", "Execute a snippet by ID or name in a pane.", serde_json::json!({
        "type": "object",
        "properties": {
            "snippet": { "type": "string", "description": "Snippet ID or name" },
            "pane": { "type": "string", "description": "Optional pane ID or name" },
            "params": { "type": "object", "additionalProperties": { "type": "string" } },
            "execute": { "type": "boolean", "description": "Append Enter after inserting (default: true)" }
        },
        "required": ["snippet"]
    })));

    tools
}

fn tool_def(name: &str, description: &str, parameters: serde_json::Value) -> ToolDefinition {
    ToolDefinition {
        tool_type: "function".into(),
        function: ToolFunctionDef {
            name: name.into(),
            description: description.into(),
            parameters,
        },
    }
}

// ---------------------------------------------------------------------------
// Tool execution
// ---------------------------------------------------------------------------

pub async fn execute_tool(
    tool_call: &ToolCall,
    agent: &AgentEngine,
    thread_id: &str,
    task_id: Option<&str>,
    session_manager: &Arc<SessionManager>,
    session_id: Option<SessionId>,
    event_tx: &broadcast::Sender<AgentEvent>,
    agent_data_dir: &std::path::Path,
    http_client: &reqwest::Client,
) -> ToolResult {
    let args: serde_json::Value =
        serde_json::from_str(&tool_call.function.arguments).unwrap_or(serde_json::json!({}));

    tracing::info!(
        tool = %tool_call.function.name,
        args = %tool_call.function.arguments,
        "agent tool call"
    );

    if !thread_id.trim().is_empty()
        && matches!(
            tool_call.function.name.as_str(),
            "bash_command" | "execute_managed_command" | "enqueue_task" | "spawn_subagent"
        )
        && agent.get_todos(thread_id).await.is_empty()
        && (task_id.is_some() || agent.planner_required_for_thread(thread_id).await)
    {
        return ToolResult {
            tool_call_id: tool_call.id.clone(),
            name: tool_call.function.name.clone(),
            content: "Plan required: call update_todo first so tamux can track the live execution plan before running commands or spawning tasks.".to_string(),
            is_error: true,
            pending_approval: None,
        };
    }

    let mut pending_approval = None;

    let result = match tool_call.function.name.as_str() {
        // Terminal/session tools (daemon owns sessions directly)
        "list_terminals" | "list_sessions" => execute_list_sessions(session_manager).await,
        "read_active_terminal_content" => execute_read_terminal(&args, session_manager).await,
        "run_terminal_command" => {
            match execute_run_terminal_command(
                &args,
                session_manager,
                session_id,
                event_tx,
                thread_id,
            )
            .await
            {
                Ok((content, approval)) => {
                    pending_approval = approval;
                    Ok(content)
                }
                Err(error) => Err(error),
            }
        }
        "execute_managed_command" => {
            match execute_managed_command(&args, session_manager, session_id, event_tx, thread_id)
                .await
            {
                Ok((content, approval)) => {
                    pending_approval = approval;
                    Ok(content)
                }
                Err(error) => Err(error),
            }
        }
        "allocate_terminal" => {
            execute_allocate_terminal(&args, session_manager, session_id, event_tx).await
        }
        "spawn_subagent" => {
            execute_spawn_subagent(
                &args,
                agent,
                thread_id,
                task_id,
                session_manager,
                session_id,
                event_tx,
            )
            .await
        }
        "list_subagents" => execute_list_subagents(&args, agent, thread_id, task_id).await,
        "enqueue_task" => execute_enqueue_task(&args, agent).await,
        "list_tasks" => execute_list_tasks(&args, agent).await,
        "cancel_task" => execute_cancel_task(&args, agent).await,
        "type_in_terminal" => execute_type_in_terminal(&args, session_manager).await,
        // Gateway messaging (execute via CLI)
        "send_slack_message"
        | "send_discord_message"
        | "send_telegram_message"
        | "send_whatsapp_message" => {
            execute_gateway_message(
                tool_call.function.name.as_str(),
                &args,
                http_client,
                agent_data_dir,
            )
            .await
        }
        // Workspace/snippet tools (read/write persistence files directly)
        "list_workspaces"
        | "create_workspace"
        | "set_active_workspace"
        | "create_surface"
        | "set_active_surface"
        | "split_pane"
        | "rename_pane"
        | "set_layout_preset"
        | "equalize_layout"
        | "list_snippets"
        | "create_snippet"
        | "run_snippet" => {
            execute_workspace_tool(tool_call.function.name.as_str(), &args, event_tx).await
        }
        // Daemon-native tools
        "bash_command" => {
            match execute_bash_command(&args, session_manager, session_id, event_tx, thread_id)
                .await
            {
                Ok((content, approval)) => {
                    pending_approval = approval;
                    Ok(content)
                }
                Err(error) => Err(error),
            }
        }
        "list_files" => execute_list_files(&args, session_manager, session_id).await,
        "read_file" => execute_read_file(&args).await,
        "write_file" => execute_write_file(&args, session_manager, session_id).await,
        "create_file" => execute_create_file(&args).await,
        "append_to_file" => execute_append_to_file(&args).await,
        "replace_in_file" => execute_replace_in_file(&args).await,
        "apply_file_patch" => execute_apply_file_patch(&args).await,
        "search_files" => execute_search_files(&args).await,
        "get_system_info" => execute_system_info().await,
        "list_processes" => execute_list_processes(&args).await,
        "search_history" => execute_search_history(&args, session_manager).await,
        "onecontext_search" => execute_onecontext_search(&args).await,
        "notify_user" => execute_notify(&args, event_tx).await,
        "update_todo" => execute_update_todo(&args, agent, thread_id, task_id).await,
        "update_memory" => execute_update_memory(&args, agent_data_dir).await,
        "list_skills" => execute_list_skills(&args, agent_data_dir).await,
        "read_skill" => execute_read_skill(&args, agent_data_dir).await,
        "web_search" => execute_web_search(&args, http_client).await,
        "fetch_url" => execute_fetch_url(&args, http_client).await,
        other => Err(anyhow::anyhow!("Unknown tool: {other}")),
    };

    match result {
        Ok(content) => {
            emit_workflow_notice_for_tool(
                event_tx,
                thread_id,
                tool_call.function.name.as_str(),
                &args,
            );
            tracing::info!(tool = %tool_call.function.name, result_len = content.len(), "agent tool result: ok");
            ToolResult {
                tool_call_id: tool_call.id.clone(),
                name: tool_call.function.name.clone(),
                content,
                is_error: false,
                pending_approval,
            }
        }
        Err(e) => {
            tracing::warn!(tool = %tool_call.function.name, error = %e, "agent tool result: error");
            ToolResult {
                tool_call_id: tool_call.id.clone(),
                name: tool_call.function.name.clone(),
                content: format!("Error: {e}"),
                is_error: true,
                pending_approval: None,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tool implementations
// ---------------------------------------------------------------------------

async fn execute_list_files(
    args: &serde_json::Value,
    _session_manager: &Arc<SessionManager>,
    _preferred_session_id: Option<SessionId>,
) -> Result<String> {
    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");
    validate_read_path(path)?;
    let mut rows = Vec::new();
    let mut read_dir = tokio::fs::read_dir(path).await?;
    while let Some(entry) = read_dir.next_entry().await? {
        let metadata = entry.metadata().await?;
        let kind = if metadata.is_dir() { "dir" } else { "file" };
        let size = metadata.len();
        let name = entry.file_name().to_string_lossy().to_string();
        rows.push(format!("{kind}\t{size}\t{name}"));
    }

    rows.sort();
    if rows.is_empty() {
        Ok("(empty directory)".to_string())
    } else {
        Ok(rows.join("\n"))
    }
}

async fn execute_read_file(args: &serde_json::Value) -> Result<String> {
    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'path' argument"))?;

    let max_lines = args
        .get("max_lines")
        .and_then(|v| v.as_u64())
        .unwrap_or(200) as usize;

    let content = tokio::fs::read_to_string(path).await?;
    let total_lines = content.lines().count();
    let lines: Vec<&str> = content.lines().take(max_lines).collect();

    let mut result = lines.join("\n");
    if total_lines > max_lines {
        result.push_str(&format!(
            "\n\n... (truncated, showing {max_lines} of {total_lines} lines)"
        ));
    }

    Ok(result)
}

async fn execute_create_file(args: &serde_json::Value) -> Result<String> {
    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'path' argument"))?;
    validate_write_path(path)?;
    let content = args
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'content' argument"))?;
    let overwrite = args
        .get("overwrite")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let target = std::path::Path::new(path);
    if target.exists() && !overwrite {
        anyhow::bail!("file already exists: {path}");
    }

    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(target, content).await?;
    Ok(format!("Created file {path} ({} bytes)", content.len()))
}

async fn execute_append_to_file(args: &serde_json::Value) -> Result<String> {
    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'path' argument"))?;
    validate_write_path(path)?;
    let content = args
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'content' argument"))?;
    let create_if_missing = args
        .get("create_if_missing")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let target = std::path::Path::new(path);
    if !target.exists() && !create_if_missing {
        anyhow::bail!("file does not exist: {path}");
    }
    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    let mut existing = if target.exists() {
        tokio::fs::read_to_string(target).await?
    } else {
        String::new()
    };
    existing.push_str(content);
    tokio::fs::write(target, existing).await?;
    Ok(format!("Appended {} bytes to {path}", content.len()))
}

async fn execute_replace_in_file(args: &serde_json::Value) -> Result<String> {
    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'path' argument"))?;
    validate_write_path(path)?;
    let old_text = args
        .get("old_text")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'old_text' argument"))?;
    let new_text = args
        .get("new_text")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'new_text' argument"))?;
    let replace_all = args
        .get("replace_all")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    apply_exact_replacements(
        path,
        vec![(old_text.to_string(), new_text.to_string(), replace_all)],
    )
    .await
}

async fn execute_apply_file_patch(args: &serde_json::Value) -> Result<String> {
    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'path' argument"))?;
    validate_write_path(path)?;
    let edits = args
        .get("edits")
        .and_then(|v| v.as_array())
        .ok_or_else(|| anyhow::anyhow!("missing 'edits' argument"))?;
    if edits.is_empty() {
        anyhow::bail!("'edits' must contain at least one edit");
    }

    let replacements = edits
        .iter()
        .enumerate()
        .map(|(index, edit)| {
            let old_text = edit
                .get("old_text")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("edit {} is missing 'old_text'", index + 1))?;
            let new_text = edit
                .get("new_text")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow::anyhow!("edit {} is missing 'new_text'", index + 1))?;
            let replace_all = edit
                .get("replace_all")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            Ok((old_text.to_string(), new_text.to_string(), replace_all))
        })
        .collect::<Result<Vec<_>>>()?;

    apply_exact_replacements(path, replacements).await
}

async fn apply_exact_replacements(
    path: &str,
    replacements: Vec<(String, String, bool)>,
) -> Result<String> {
    let target = std::path::Path::new(path);
    let mut content = tokio::fs::read_to_string(target).await?;
    let mut summary = Vec::with_capacity(replacements.len());

    for (index, (old_text, new_text, replace_all)) in replacements.into_iter().enumerate() {
        if old_text.is_empty() {
            anyhow::bail!("edit {} has empty 'old_text'", index + 1);
        }

        let match_count = content.matches(&old_text).count();
        if match_count == 0 {
            anyhow::bail!("edit {} target text was not found in {}", index + 1, path);
        }
        if !replace_all && match_count != 1 {
            anyhow::bail!(
                "edit {} matched {} locations in {}; set replace_all=true or provide a more specific old_text",
                index + 1,
                match_count,
                path
            );
        }

        content = if replace_all {
            content.replace(&old_text, &new_text)
        } else {
            content.replacen(&old_text, &new_text, 1)
        };
        summary.push(format!(
            "edit {} replaced {} occurrence(s)",
            index + 1,
            if replace_all { match_count } else { 1 }
        ));
    }

    tokio::fs::write(target, content).await?;
    Ok(format!("Patched {} with {}.", path, summary.join(", ")))
}

async fn execute_write_file(
    args: &serde_json::Value,
    session_manager: &Arc<SessionManager>,
    preferred_session_id: Option<SessionId>,
) -> Result<String> {
    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'path' argument"))?;
    validate_write_path(path)?;

    let content = args
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'content' argument"))?;

    let timeout_secs = args
        .get("timeout_seconds")
        .and_then(|v| v.as_u64())
        .unwrap_or(30)
        .min(600);

    let sessions = session_manager.list().await;
    if sessions.is_empty() {
        anyhow::bail!("No active terminal sessions are available for write_file");
    }

    let resolved_session_id =
        if let Some(session_ref) = args.get("session").and_then(|v| v.as_str()) {
            sessions
                .iter()
                .find(|session| {
                    session.id.to_string() == session_ref
                        || session.id.to_string().contains(session_ref)
                })
                .map(|session| session.id)
                .ok_or_else(|| anyhow::anyhow!("session not found: {session_ref}"))?
        } else {
            preferred_session_id.unwrap_or(sessions[0].id)
        };

    let (mut rx, _) = session_manager.subscribe(resolved_session_id).await?;
    let command = build_write_file_command(path, content);
    let request = ManagedCommandRequest {
        command,
        rationale: format!(
            "Write {} bytes to file path requested by agent tool call",
            content.len()
        ),
        allow_network: false,
        sandbox_enabled: false,
        security_level: SecurityLevel::Lowest,
        cwd: None,
        language_hint: Some("shell".to_string()),
        source: ManagedCommandSource::Agent,
    };

    let queued = session_manager
        .execute_managed_command(resolved_session_id, request)
        .await?;

    let execution_id = match queued {
        DaemonMessage::ManagedCommandQueued { execution_id, .. } => execution_id,
        DaemonMessage::ApprovalRequired { approval, .. } => {
            return Err(anyhow::anyhow!(
                "write_file requires approval before execution (approval_id: {})",
                approval.approval_id
            ));
        }
        DaemonMessage::ManagedCommandRejected { message, .. } => {
            return Err(anyhow::anyhow!("write_file command rejected: {message}"));
        }
        other => {
            return Err(anyhow::anyhow!(
                "unexpected managed command response for write_file: {}",
                daemon_message_kind(&other)
            ));
        }
    };

    let wait_deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
    loop {
        let remaining = wait_deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return Err(anyhow::anyhow!(
                "write_file timed out waiting for terminal command completion (execution_id: {execution_id})"
            ));
        }

        let event = tokio::time::timeout(remaining, rx.recv())
            .await
            .map_err(|_| {
                anyhow::anyhow!(
                    "write_file timed out waiting for terminal command completion (execution_id: {execution_id})"
                )
            })?;

        let msg = match event {
            Ok(message) => message,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                return Err(anyhow::anyhow!(
                    "terminal session event stream closed while waiting for write_file result"
                ));
            }
        };

        match msg {
            DaemonMessage::ManagedCommandFinished {
                execution_id: finished_id,
                exit_code,
                ..
            } if finished_id == execution_id => {
                if exit_code == Some(0) {
                    return Ok(format!(
                        "Written {} bytes to {path} via session {resolved_session_id} (execution_id: {execution_id})",
                        content.len()
                    ));
                }
                return Err(anyhow::anyhow!(
                    "write_file terminal command failed (execution_id: {execution_id}, exit_code: {:?})",
                    exit_code
                ));
            }
            DaemonMessage::ManagedCommandRejected {
                execution_id: rejected_id,
                message,
                ..
            } => {
                if rejected_id.as_deref() == Some(execution_id.as_str()) || rejected_id.is_none() {
                    return Err(anyhow::anyhow!(
                        "write_file terminal command rejected (execution_id: {execution_id}): {message}"
                    ));
                }
            }
            _ => {}
        }
    }
}

fn validate_write_path(path: &str) -> Result<()> {
    if path.is_empty() {
        return Err(anyhow::anyhow!("'path' must not be empty"));
    }
    if path.trim().is_empty() {
        return Err(anyhow::anyhow!("'path' must not be blank"));
    }
    if path.trim() != path {
        return Err(anyhow::anyhow!(
            "invalid 'path': leading/trailing whitespace is not allowed"
        ));
    }
    if path.chars().any(|ch| ch.is_control()) {
        return Err(anyhow::anyhow!(
            "invalid 'path': control characters are not allowed"
        ));
    }

    Ok(())
}

fn validate_read_path(path: &str) -> Result<()> {
    if path.is_empty() {
        return Err(anyhow::anyhow!("'path' must not be empty"));
    }
    if path.trim().is_empty() {
        return Err(anyhow::anyhow!("'path' must not be blank"));
    }
    if path.trim() != path {
        return Err(anyhow::anyhow!(
            "invalid 'path': leading/trailing whitespace is not allowed"
        ));
    }
    if path.chars().any(|ch| ch.is_control()) {
        return Err(anyhow::anyhow!(
            "invalid 'path': control characters are not allowed"
        ));
    }

    Ok(())
}

fn build_write_file_command(path: &str, content: &str) -> String {
    let path_b64 = base64::engine::general_purpose::STANDARD.encode(path.as_bytes());
    let content_b64 = base64::engine::general_purpose::STANDARD.encode(content.as_bytes());
    let script = build_write_file_script(&path_b64, &content_b64);

    let script_b64 = base64::engine::general_purpose::STANDARD.encode(script.as_bytes());
    format!(
        "if command -v python3 >/dev/null 2>&1; then \
             python3 -c \"import base64;exec(base64.b64decode('{script_b64}').decode('utf-8'))\"; \
         else \
             python -c \"import base64;exec(base64.b64decode('{script_b64}').decode('utf-8'))\"; \
         fi"
    )
}

fn build_write_file_script(path_b64: &str, content_b64: &str) -> String {
    let mut script = vec![
        "import base64, pathlib".to_string(),
        format!("p = pathlib.Path(base64.b64decode('{path_b64}').decode('utf-8'))"),
        format!("data = base64.b64decode('{content_b64}')"),
        "p.parent.mkdir(parents=True, exist_ok=True)".to_string(),
        "p.write_bytes(data)".to_string(),
        "actual = p.stat().st_size".to_string(),
        "expected = len(data)".to_string(),
        "if actual != expected:".to_string(),
        "    raise SystemExit(f'size mismatch: expected {expected}, got {actual}')".to_string(),
        "print(f'written {actual} bytes to {p}')".to_string(),
    ]
    .join("\n");
    script.push('\n');
    script
}

fn build_list_files_script(path_b64: &str, token: &str) -> String {
    let mut script = vec![
        "import base64, pathlib, sys".to_string(),
        format!("p = pathlib.Path(base64.b64decode('{path_b64}').decode('utf-8'))"),
        "try:".to_string(),
        "    rows = []".to_string(),
        "    for entry in sorted(p.iterdir(), key=lambda item: item.name):".to_string(),
        "        kind = 'dir' if entry.is_dir() else 'file'".to_string(),
        "        size = entry.stat().st_size".to_string(),
        "        rows.append(f'{kind}\\t{size}\\t{entry.name}')".to_string(),
        "    payload = '\\n'.join(rows) if rows else '(empty directory)'".to_string(),
        "    status = 0".to_string(),
        "except Exception as exc:".to_string(),
        "    payload = f'Error: {exc}'".to_string(),
        "    status = 1".to_string(),
        "encoded = base64.b64encode(payload.encode('utf-8')).decode('ascii')".to_string(),
        format!("print('__AMUX_CAPTURE_BEGIN_{token}__')"),
        "print(encoded)".to_string(),
        format!("print(f'__AMUX_CAPTURE_END_{token}__:{{status}}')"),
        "sys.exit(status)".to_string(),
    ]
    .join("\n");
    script.push('\n');
    script
}

fn daemon_message_kind(msg: &DaemonMessage) -> &'static str {
    match msg {
        DaemonMessage::ManagedCommandQueued { .. } => "managed_command_queued",
        DaemonMessage::ApprovalRequired { .. } => "approval_required",
        DaemonMessage::ManagedCommandRejected { .. } => "managed_command_rejected",
        DaemonMessage::ManagedCommandStarted { .. } => "managed_command_started",
        DaemonMessage::ManagedCommandFinished { .. } => "managed_command_finished",
        _ => "other",
    }
}

enum ManagedCommandWaitOutcome {
    Finished {
        exit_code: Option<i32>,
        duration_ms: Option<u64>,
        output_tail: String,
    },
    Rejected {
        message: String,
    },
    Timeout {
        output_tail: String,
    },
}

fn terminal_output_tail(raw: &[u8], max_lines: usize) -> String {
    if raw.is_empty() {
        return String::new();
    }
    let stripped = strip_ansi_escapes::strip(raw);
    let text = String::from_utf8_lossy(&stripped);
    let lines: Vec<&str> = text.lines().collect();
    if lines.is_empty() {
        return String::new();
    }
    let start = lines.len().saturating_sub(max_lines);
    let mut result = String::new();
    if start > 0 {
        result.push_str(&format!("... ({} earlier lines omitted)\n", start));
    }
    result.push_str(&lines[start..].join("\n"));
    result
}

async fn wait_for_managed_command_outcome(
    rx: &mut tokio::sync::broadcast::Receiver<DaemonMessage>,
    session_id: SessionId,
    execution_id: &str,
    timeout_secs: u64,
) -> Result<ManagedCommandWaitOutcome> {
    const MAX_CAPTURE_BYTES: usize = 512_000;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
    let mut output_buf = Vec::new();

    loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return Ok(ManagedCommandWaitOutcome::Timeout {
                output_tail: terminal_output_tail(&output_buf, 80),
            });
        }

        let event = tokio::time::timeout(remaining, rx.recv())
            .await
            .map_err(|_| anyhow::anyhow!("timed out waiting for managed command result"))?;

        let msg = match event {
            Ok(message) => message,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                return Err(anyhow::anyhow!(
                    "terminal session event stream closed while waiting for managed command result"
                ));
            }
        };

        match msg {
            DaemonMessage::Output { id, data } if id == session_id => {
                output_buf.extend_from_slice(&data);
                if output_buf.len() > MAX_CAPTURE_BYTES {
                    let overflow = output_buf.len() - MAX_CAPTURE_BYTES;
                    output_buf.drain(..overflow);
                }
            }
            DaemonMessage::ManagedCommandFinished {
                id,
                execution_id: finished_id,
                exit_code,
                duration_ms,
                ..
            } if id == session_id && finished_id == execution_id => {
                return Ok(ManagedCommandWaitOutcome::Finished {
                    exit_code,
                    duration_ms,
                    output_tail: terminal_output_tail(&output_buf, 80),
                });
            }
            DaemonMessage::ManagedCommandRejected {
                id,
                execution_id: rejected_id,
                message,
            } if id == session_id
                && (rejected_id.as_deref() == Some(execution_id) || rejected_id.is_none()) =>
            {
                return Ok(ManagedCommandWaitOutcome::Rejected { message });
            }
            _ => {}
        }
    }
}

async fn execute_terminal_python_capture(
    session_manager: &Arc<SessionManager>,
    preferred_session_id: Option<SessionId>,
    requested_session: Option<&str>,
    script: &str,
    token: &str,
    rationale: &str,
    timeout_secs: u64,
) -> Result<String> {
    const MAX_CAPTURE_BYTES: usize = 512_000;
    let sessions = session_manager.list().await;
    if sessions.is_empty() {
        anyhow::bail!("No active terminal sessions are available");
    }

    let resolved_session_id = if let Some(session_ref) = requested_session {
        sessions
            .iter()
            .find(|session| {
                session.id.to_string() == session_ref
                    || session.id.to_string().contains(session_ref)
            })
            .map(|session| session.id)
            .ok_or_else(|| anyhow::anyhow!("session not found: {session_ref}"))?
    } else {
        preferred_session_id.unwrap_or(sessions[0].id)
    };

    let (mut rx, _) = session_manager.subscribe(resolved_session_id).await?;
    let script_b64 = base64::engine::general_purpose::STANDARD.encode(script.as_bytes());
    let command = format!(
        "if command -v python3 >/dev/null 2>&1; then \
             python3 -c \"import base64;exec(base64.b64decode('{script_b64}').decode('utf-8'))\"; \
         else \
             python -c \"import base64;exec(base64.b64decode('{script_b64}').decode('utf-8'))\"; \
         fi"
    );
    let request = ManagedCommandRequest {
        command,
        rationale: rationale.to_string(),
        allow_network: false,
        sandbox_enabled: false,
        security_level: SecurityLevel::Lowest,
        cwd: None,
        language_hint: Some("python".to_string()),
        source: ManagedCommandSource::Agent,
    };

    let queued = session_manager
        .execute_managed_command(resolved_session_id, request)
        .await?;
    let execution_id = match queued {
        DaemonMessage::ManagedCommandQueued { execution_id, .. } => execution_id,
        DaemonMessage::ApprovalRequired { approval, .. } => {
            return Err(anyhow::anyhow!(
                "terminal capture command requires approval before execution (approval_id: {})",
                approval.approval_id
            ));
        }
        DaemonMessage::ManagedCommandRejected { message, .. } => {
            return Err(anyhow::anyhow!(
                "terminal capture command rejected: {message}"
            ));
        }
        other => {
            return Err(anyhow::anyhow!(
                "unexpected managed command response: {}",
                daemon_message_kind(&other)
            ));
        }
    };

    let wait_deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);
    let mut output_buf: Vec<u8> = Vec::new();
    loop {
        let remaining = wait_deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            return Err(anyhow::anyhow!(
                "timed out waiting for terminal capture command completion (execution_id: {execution_id})"
            ));
        }

        let event = tokio::time::timeout(remaining, rx.recv())
            .await
            .map_err(|_| {
                anyhow::anyhow!(
                    "timed out waiting for terminal capture command completion (execution_id: {execution_id})"
                )
            })?;

        let msg = match event {
            Ok(message) => message,
            Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
            Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                return Err(anyhow::anyhow!(
                    "terminal session event stream closed while waiting for command output"
                ));
            }
        };

        match msg {
            DaemonMessage::Output { id, data } if id == resolved_session_id => {
                output_buf.extend_from_slice(&data);
                if output_buf.len() > MAX_CAPTURE_BYTES {
                    let overflow = output_buf.len() - MAX_CAPTURE_BYTES;
                    output_buf.drain(..overflow);
                }
            }
            DaemonMessage::ManagedCommandFinished {
                id,
                execution_id: finished_id,
                exit_code,
                ..
            } if id == resolved_session_id && finished_id == execution_id => {
                let (captured_status, captured_output) = parse_capture_output(&output_buf, token)
                    .ok_or_else(|| {
                    anyhow::anyhow!(
                        "failed to parse captured command output (execution_id: {execution_id})"
                    )
                })?;

                if captured_status == 0 && exit_code == Some(0) {
                    return Ok(captured_output);
                }

                return Err(anyhow::anyhow!(
                    "terminal capture command failed (execution_id: {execution_id}, exit_code: {:?}): {}",
                    exit_code,
                    captured_output
                ));
            }
            DaemonMessage::ManagedCommandRejected {
                id,
                execution_id: rejected_id,
                message,
            } if id == resolved_session_id
                && (rejected_id.as_deref() == Some(execution_id.as_str())
                    || rejected_id.is_none()) =>
            {
                return Err(anyhow::anyhow!(
                    "terminal capture command rejected (execution_id: {execution_id}): {message}"
                ));
            }
            _ => {}
        }
    }
}

fn parse_capture_output(output: &[u8], token: &str) -> Option<(i32, String)> {
    let stripped = strip_ansi_escapes::strip(output);
    let text = String::from_utf8_lossy(&stripped);

    let begin_marker = format!("__AMUX_CAPTURE_BEGIN_{token}__");
    let end_prefix = format!("__AMUX_CAPTURE_END_{token}__:");

    let begin_idx = text.rfind(&begin_marker)?;
    let after_begin = &text[begin_idx + begin_marker.len()..];
    let after_begin = after_begin.trim_start_matches(['\r', '\n']);

    let end_idx = after_begin.find(&end_prefix)?;
    let encoded_payload = after_begin[..end_idx]
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>();
    if encoded_payload.is_empty() {
        return Some((0, String::new()));
    }

    let after_end = &after_begin[end_idx + end_prefix.len()..];
    let status_raw = after_end
        .chars()
        .take_while(|ch| ch.is_ascii_digit() || *ch == '-')
        .collect::<String>();
    let status = status_raw.parse::<i32>().ok()?;

    let decoded = base64::engine::general_purpose::STANDARD
        .decode(encoded_payload)
        .ok()?;
    let payload = String::from_utf8_lossy(&decoded).into_owned();
    Some((status, payload))
}

async fn execute_search_files(args: &serde_json::Value) -> Result<String> {
    let pattern = args
        .get("pattern")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'pattern' argument"))?;

    let path = args.get("path").and_then(|v| v.as_str()).unwrap_or(".");

    let max_results = args
        .get("max_results")
        .and_then(|v| v.as_u64())
        .unwrap_or(50);

    let file_pattern = args.get("file_pattern").and_then(|v| v.as_str());

    // Use grep for search
    let mut cmd_args = vec!["-rn".to_string(), "--color=never".to_string()];
    if let Some(fp) = file_pattern {
        cmd_args.push(format!("--include={fp}"));
    }
    cmd_args.push(pattern.to_string());
    cmd_args.push(path.to_string());

    let output = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        tokio::process::Command::new("grep")
            .args(&cmd_args)
            .output(),
    )
    .await
    .map_err(|_| anyhow::anyhow!("search timed out"))??;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout.lines().take(max_results as usize).collect();
    let total = stdout.lines().count();

    if lines.is_empty() {
        Ok("No matches found.".into())
    } else {
        let mut result = lines.join("\n");
        if total > lines.len() {
            result.push_str(&format!("\n\n... ({} more matches)", total - lines.len()));
        }
        Ok(result)
    }
}

async fn execute_system_info() -> Result<String> {
    use sysinfo::System;

    let mut sys = System::new_all();
    sys.refresh_all();

    let total_mem = sys.total_memory();
    let used_mem = sys.used_memory();
    let cpu_count = sys.cpus().len();
    let load_avg = System::load_average();

    Ok(format!(
        "CPU cores: {cpu_count}\n\
         Load average: {:.2} {:.2} {:.2}\n\
         Memory: {:.1} GB / {:.1} GB ({:.0}% used)\n\
         OS: {} {}\n\
         Hostname: {}",
        load_avg.one,
        load_avg.five,
        load_avg.fifteen,
        used_mem as f64 / 1_073_741_824.0,
        total_mem as f64 / 1_073_741_824.0,
        (used_mem as f64 / total_mem as f64) * 100.0,
        System::name().unwrap_or_default(),
        System::os_version().unwrap_or_default(),
        System::host_name().unwrap_or_default(),
    ))
}

async fn execute_list_processes(args: &serde_json::Value) -> Result<String> {
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize;

    use sysinfo::System;
    let mut sys = System::new_all();
    sys.refresh_all();

    let mut procs: Vec<(u32, String, f32, u64)> = sys
        .processes()
        .values()
        .map(|p| {
            (
                p.pid().as_u32(),
                p.name().to_string(),
                p.cpu_usage(),
                p.memory(),
            )
        })
        .collect();

    procs.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

    let header = format!(
        "{:<8} {:<30} {:>8} {:>12}",
        "PID", "NAME", "CPU%", "MEM(MB)"
    );
    let rows: Vec<String> = procs
        .iter()
        .take(limit)
        .map(|(pid, name, cpu, mem)| {
            format!(
                "{:<8} {:<30} {:>7.1}% {:>12.1}",
                pid,
                if name.len() > 30 { &name[..30] } else { name },
                cpu,
                *mem as f64 / 1_048_576.0
            )
        })
        .collect();

    Ok(format!("{header}\n{}", rows.join("\n")))
}

async fn execute_search_history(
    args: &serde_json::Value,
    session_manager: &Arc<SessionManager>,
) -> Result<String> {
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'query' argument"))?;

    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as usize;

    let (summary, hits) = session_manager.search_history(query, limit)?;

    if hits.is_empty() {
        Ok("No matching history entries.".into())
    } else {
        let lines: Vec<String> = hits
            .iter()
            .map(|h| {
                format!(
                    "[{:.1}] {} — {}",
                    h.score,
                    h.title,
                    h.excerpt.chars().take(120).collect::<String>(),
                )
            })
            .collect();
        Ok(format!("{summary}\n\n{}", lines.join("\n")))
    }
}

async fn execute_onecontext_search(args: &serde_json::Value) -> Result<String> {
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'query' argument"))?
        .trim();

    if query.is_empty() {
        return Err(anyhow::anyhow!("'query' must not be empty"));
    }

    let scope = args
        .get("scope")
        .and_then(|v| v.as_str())
        .unwrap_or("session");
    if !matches!(scope, "session" | "event" | "turn") {
        return Err(anyhow::anyhow!(
            "invalid 'scope': {scope} (expected session, event, or turn)"
        ));
    }

    if !super::aline_available() {
        return Ok("OneContext search unavailable: `aline` CLI not found on PATH.".into());
    }

    let no_regex = args
        .get("no_regex")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let bounded_query = query
        .chars()
        .take(ONECONTEXT_TOOL_QUERY_MAX_CHARS)
        .collect::<String>();

    let mut cmd = tokio::process::Command::new("aline");
    cmd.arg("search")
        .arg(&bounded_query)
        .arg("-t")
        .arg(scope)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .stdin(std::process::Stdio::null());
    if no_regex {
        cmd.arg("--no-regex");
    }

    let output = tokio::time::timeout(
        std::time::Duration::from_secs(ONECONTEXT_TOOL_TIMEOUT_SECS),
        cmd.output(),
    )
    .await
    .map_err(|_| anyhow::anyhow!("onecontext search timed out"))??;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if stderr.is_empty() {
            return Err(anyhow::anyhow!("onecontext search failed"));
        }
        return Err(anyhow::anyhow!("onecontext search failed: {stderr}"));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Ok(format!(
            "No OneContext matches for \"{bounded_query}\" in {scope} scope."
        ));
    }

    let trimmed_chars = trimmed.chars().count();
    let output_text = if trimmed_chars > ONECONTEXT_TOOL_OUTPUT_MAX_CHARS {
        let shortened = trimmed
            .chars()
            .take(ONECONTEXT_TOOL_OUTPUT_MAX_CHARS)
            .collect::<String>();
        format!(
            "{}\n\n(truncated, {} chars total)",
            shortened, trimmed_chars
        )
    } else {
        trimmed.to_string()
    };

    Ok(format!(
        "OneContext results for \"{bounded_query}\" ({scope} scope):\n\n{output_text}"
    ))
}

async fn execute_list_sessions(session_manager: &Arc<SessionManager>) -> Result<String> {
    // If we have frontend topology, use it for a richer view that includes
    // browser panels and workspace/surface hierarchy.
    if let Some(topology) = session_manager.read_workspace_topology() {
        let sessions = session_manager.list().await;
        let formatted = amux_protocol::format_topology(&topology, &sessions);
        if !formatted.is_empty() {
            return Ok(formatted);
        }
        return Ok("No active sessions or panes.".into());
    }

    // Fallback: no topology reported, list raw sessions.
    let sessions = session_manager.list().await;

    if sessions.is_empty() {
        Ok("No active sessions.".into())
    } else {
        let lines: Vec<String> = sessions
            .iter()
            .map(|s| {
                let mut line = format!(
                    "{} cols={} rows={} alive={} cwd={}",
                    s.id,
                    s.cols,
                    s.rows,
                    s.is_alive,
                    s.cwd.as_deref().unwrap_or("?"),
                );
                if let Some(cmd) = s.active_command.as_deref() {
                    line.push_str(&format!(" cmd={cmd}"));
                }
                if let Some(ws) = s.workspace_id.as_deref() {
                    line.push_str(&format!(" workspace={ws}"));
                }
                line
            })
            .collect();
        Ok(lines.join("\n"))
    }
}

async fn execute_notify(
    args: &serde_json::Value,
    event_tx: &broadcast::Sender<AgentEvent>,
) -> Result<String> {
    let title = args
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("Notification");
    let message = args.get("message").and_then(|v| v.as_str()).unwrap_or("");
    let severity = match args.get("severity").and_then(|v| v.as_str()) {
        Some("warning") => NotificationSeverity::Warning,
        Some("alert") => NotificationSeverity::Alert,
        Some("error") => NotificationSeverity::Error,
        _ => NotificationSeverity::Info,
    };

    let _ = event_tx.send(AgentEvent::Notification {
        title: title.into(),
        body: message.into(),
        severity,
        channels: vec!["in-app".into()],
    });

    Ok(format!("Notification sent: {title}"))
}

async fn execute_update_memory(
    args: &serde_json::Value,
    agent_data_dir: &std::path::Path,
) -> Result<String> {
    let content = args
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'content' argument"))?;

    let memory_dir = super::active_memory_dir(agent_data_dir);
    let memory_path = memory_dir.join("MEMORY.md");
    tokio::fs::create_dir_all(&memory_dir).await?;
    tokio::fs::write(&memory_path, content).await?;

    Ok("Memory updated successfully.".into())
}

async fn execute_list_skills(
    args: &serde_json::Value,
    agent_data_dir: &std::path::Path,
) -> Result<String> {
    let skills_root = super::skills_dir(agent_data_dir);
    let query = args
        .get("query")
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_lowercase())
        .filter(|value| !value.is_empty());
    let limit = args
        .get("limit")
        .and_then(|value| value.as_u64())
        .unwrap_or(20)
        .clamp(1, 100) as usize;

    let mut files = Vec::new();
    collect_skill_documents(&skills_root, &mut files)?;
    if files.is_empty() {
        return Ok(format!(
            "No local skills found under {}.",
            skills_root.display()
        ));
    }

    files.sort();
    let root_canonical = std::fs::canonicalize(&skills_root).unwrap_or(skills_root.clone());
    let mut entries = files
        .into_iter()
        .filter_map(|path| {
            let relative = path
                .strip_prefix(&root_canonical)
                .or_else(|_| path.strip_prefix(&skills_root))
                .ok()?
                .to_string_lossy()
                .replace('\\', "/");
            let stem = path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("skill");
            Some((stem.to_string(), relative))
        })
        .filter(|(stem, relative)| match query.as_ref() {
            Some(needle) => {
                stem.to_lowercase().contains(needle) || relative.to_lowercase().contains(needle)
            }
            None => true,
        })
        .take(limit)
        .collect::<Vec<_>>();

    if entries.is_empty() {
        return Ok(format!(
            "No local skills matched under {}.",
            skills_root.display()
        ));
    }

    let mut body = format!("Local skills under {}:\n", skills_root.display());
    for (stem, relative) in entries.drain(..) {
        body.push_str(&format!("- {} ({})\n", stem, relative));
    }
    Ok(body)
}

async fn execute_read_skill(
    args: &serde_json::Value,
    agent_data_dir: &std::path::Path,
) -> Result<String> {
    let skill = args
        .get("skill")
        .and_then(|value| value.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'skill' argument"))?
        .trim();
    if skill.is_empty() {
        return Err(anyhow::anyhow!("'skill' must not be empty"));
    }

    let max_lines = args
        .get("max_lines")
        .and_then(|value| value.as_u64())
        .unwrap_or(200)
        .clamp(20, 1000) as usize;
    let skills_root = super::skills_dir(agent_data_dir);
    let skill_path = resolve_skill_path(&skills_root, skill)?;
    let content = tokio::fs::read_to_string(&skill_path).await?;
    let total_lines = content.lines().count();
    let lines = content.lines().take(max_lines).collect::<Vec<_>>();
    let relative = skill_path
        .strip_prefix(&skills_root)
        .unwrap_or(skill_path.as_path())
        .display()
        .to_string();

    let mut body = format!("Skill {}:\n\n{}", relative, lines.join("\n"));
    if total_lines > max_lines {
        body.push_str(&format!(
            "\n\n... (truncated, showing {max_lines} of {total_lines} lines)"
        ));
    }
    Ok(body)
}

async fn execute_update_todo(
    args: &serde_json::Value,
    agent: &AgentEngine,
    thread_id: &str,
    task_id: Option<&str>,
) -> Result<String> {
    let raw_items = args
        .get("items")
        .and_then(|value| value.as_array())
        .ok_or_else(|| anyhow::anyhow!("missing 'items' argument"))?;

    let now = super::now_millis();
    let mut items = Vec::new();
    for (index, raw_item) in raw_items.iter().enumerate() {
        let content = raw_item
            .get("content")
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| anyhow::anyhow!("todo item {index} is missing non-empty 'content'"))?;
        let status = match raw_item
            .get("status")
            .and_then(|value| value.as_str())
            .unwrap_or("pending")
        {
            "pending" => TodoStatus::Pending,
            "in_progress" => TodoStatus::InProgress,
            "completed" => TodoStatus::Completed,
            "blocked" => TodoStatus::Blocked,
            other => {
                return Err(anyhow::anyhow!(
                    "todo item {index} has invalid status '{other}'"
                ));
            }
        };

        items.push(TodoItem {
            id: format!("todo_{}", uuid::Uuid::new_v4()),
            content: content.to_string(),
            status,
            position: index,
            step_index: raw_item
                .get("step_index")
                .and_then(|value| value.as_u64())
                .map(|value| value as usize),
            created_at: now,
            updated_at: now,
        });
    }

    agent
        .replace_thread_todos(thread_id, items.clone(), task_id)
        .await;

    Ok(format!("Updated todo list with {} item(s).", items.len()))
}

async fn execute_web_search(
    args: &serde_json::Value,
    http_client: &reqwest::Client,
) -> Result<String> {
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'query' argument"))?;

    let max_results = args
        .get("max_results")
        .and_then(|v| v.as_u64())
        .unwrap_or(5);

    // Use DuckDuckGo lite as a zero-config fallback
    let url = format!(
        "https://lite.duckduckgo.com/lite/?q={}&kl=us-en",
        urlencoding::encode(query)
    );

    let resp = http_client
        .get(&url)
        .header("User-Agent", "tamux-agent/0.1")
        .send()
        .await?;

    let text = resp.text().await?;

    // Extract result snippets from DDG lite HTML
    let mut results = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("<a rel=\"nofollow\"") {
            // Extract URL and text
            if let (Some(href_start), Some(href_end)) =
                (trimmed.find("href=\""), trimmed.find("\">"))
            {
                let url = &trimmed[href_start + 6..href_end];
                let text_start = href_end + 2;
                if let Some(text_end) = trimmed[text_start..].find("</a>") {
                    let title = &trimmed[text_start..text_start + text_end];
                    results.push(format!("- {title}\n  {url}"));
                }
            }
        }
        if results.len() >= max_results as usize {
            break;
        }
    }

    if results.is_empty() {
        Ok(format!("No web results found for: {query}"))
    } else {
        Ok(format!(
            "Web results for \"{query}\":\n\n{}",
            results.join("\n\n")
        ))
    }
}

fn emit_workflow_notice_for_tool(
    event_tx: &broadcast::Sender<AgentEvent>,
    thread_id: &str,
    tool_name: &str,
    args: &serde_json::Value,
) {
    if thread_id.trim().is_empty() {
        return;
    }

    let (kind, message, details) = match tool_name {
        "update_todo" => {
            let count = args
                .get("items")
                .and_then(|value| value.as_array())
                .map(|items| items.len())
                .unwrap_or(0);
            (
                "plan-mode",
                format!("Agent updated plan mode with {count} todo item(s)."),
                None,
            )
        }
        "update_memory" => (
            "memory-updated",
            "Agent updated persistent memory.".to_string(),
            None,
        ),
        "list_skills" | "read_skill" => (
            "skill-consulted",
            format!("Agent consulted local skills via {tool_name}."),
            Some(args.to_string()),
        ),
        "onecontext_search" => (
            "history-consulted",
            "Agent consulted OneContext history.".to_string(),
            Some(args.to_string()),
        ),
        _ => return,
    };

    let _ = event_tx.send(AgentEvent::WorkflowNotice {
        thread_id: thread_id.to_string(),
        kind: kind.to_string(),
        message,
        details,
    });
}

fn collect_skill_documents(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }

    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_skill_documents(&path, out)?;
            continue;
        }
        if !file_type.is_file() {
            continue;
        }

        // Include any .md file in the skills tree — covers SKILL.md, generated
        // skills, and curated skill documents alike.
        let is_md = path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("md"));
        if is_md {
            out.push(path);
        }
    }

    Ok(())
}

fn resolve_skill_path(skills_root: &std::path::Path, skill: &str) -> Result<std::path::PathBuf> {
    validate_read_path(skill)?;
    let root_canonical = std::fs::canonicalize(skills_root).unwrap_or(skills_root.to_path_buf());

    let direct_candidate = std::path::Path::new(skill);
    if direct_candidate.components().count() > 1 || direct_candidate.is_absolute() {
        let candidate = if direct_candidate.is_absolute() {
            direct_candidate.to_path_buf()
        } else {
            skills_root.join(direct_candidate)
        };
        let canonical = std::fs::canonicalize(&candidate)
            .with_context(|| format!("skill '{}' was not found", skill))?;
        if !canonical.starts_with(&root_canonical) {
            anyhow::bail!("skill path must stay inside {}", skills_root.display());
        }
        return Ok(canonical);
    }

    let mut files = Vec::new();
    collect_skill_documents(skills_root, &mut files)?;
    let normalized = skill.to_lowercase();

    files.sort();
    for path in &files {
        let relative = path
            .strip_prefix(&root_canonical)
            .or_else(|_| path.strip_prefix(skills_root))
            .unwrap_or(path.as_path())
            .to_string_lossy()
            .replace('\\', "/");
        let stem = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_lowercase();
        if stem == normalized || relative.to_lowercase() == normalized {
            return Ok(path.clone());
        }
    }

    for path in &files {
        let relative = path
            .strip_prefix(&root_canonical)
            .or_else(|_| path.strip_prefix(skills_root))
            .unwrap_or(path.as_path())
            .to_string_lossy()
            .replace('\\', "/");
        let stem = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_lowercase();
        if stem.contains(&normalized) || relative.to_lowercase().contains(&normalized) {
            return Ok(path.clone());
        }
    }

    anyhow::bail!(
        "skill '{}' was not found under {}",
        skill,
        skills_root.display()
    )
}

async fn execute_fetch_url(
    args: &serde_json::Value,
    http_client: &reqwest::Client,
) -> Result<String> {
    let url = args
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'url' argument"))?;

    let max_length = args
        .get("max_length")
        .and_then(|v| v.as_u64())
        .unwrap_or(10_000) as usize;

    let resp = http_client
        .get(url)
        .header("User-Agent", "tamux-agent/0.1")
        .timeout(std::time::Duration::from_secs(15))
        .send()
        .await?;

    let status = resp.status();
    let text = resp.text().await?;

    // Basic HTML tag stripping
    let stripped = strip_html_tags(&text);
    let truncated = if stripped.len() > max_length {
        format!(
            "{}...\n\n(truncated, {} chars total)",
            &stripped[..max_length],
            stripped.len()
        )
    } else {
        stripped
    };

    Ok(format!("HTTP {status}\n\n{truncated}"))
}

// ---------------------------------------------------------------------------
// Terminal/session tools — daemon owns sessions directly
// ---------------------------------------------------------------------------

async fn execute_read_terminal(
    args: &serde_json::Value,
    session_manager: &Arc<SessionManager>,
) -> Result<String> {
    let sessions = session_manager.list().await;
    if sessions.is_empty() {
        return Ok("No active terminal sessions.".into());
    }

    let target_id = if let Some(pane) = args.get("pane").and_then(|v| v.as_str()) {
        sessions
            .iter()
            .find(|s| s.id.to_string().contains(pane))
            .map(|s| s.id)
    } else {
        None
    };

    let sid = target_id.unwrap_or(sessions[0].id);

    // Read full scrollback, no line limit — get everything the session has
    match session_manager.get_scrollback(sid, None).await {
        Ok(data) => {
            if data.is_empty() {
                return Ok("(terminal buffer is empty)".into());
            }

            // Strip ANSI escapes using the strip-ansi-escapes crate (already in deps)
            let stripped = strip_ansi_escapes::strip(&data);
            let text = String::from_utf8_lossy(&stripped);

            // Take last 200 lines to keep output manageable
            let lines: Vec<&str> = text.lines().collect();
            let start = if lines.len() > 200 {
                lines.len() - 200
            } else {
                0
            };
            let visible: Vec<&str> = lines[start..]
                .iter()
                .filter(|l| !l.trim().is_empty())
                .copied()
                .collect();

            if visible.is_empty() {
                Ok("(terminal buffer contains only whitespace/control characters)".into())
            } else {
                let mut result = visible.join("\n");
                if start > 0 {
                    result = format!("... ({} earlier lines omitted)\n\n{result}", start);
                }
                Ok(result)
            }
        }
        Err(e) => Ok(format!("Failed to read terminal: {e}")),
    }
}

async fn execute_run_terminal_command(
    args: &serde_json::Value,
    session_manager: &Arc<SessionManager>,
    session_id: Option<SessionId>,
    event_tx: &broadcast::Sender<AgentEvent>,
    thread_id: &str,
) -> Result<(String, Option<ToolPendingApproval>)> {
    let managed_args =
        managed_alias_args(args, "Run a shell command in a managed terminal session");
    execute_managed_command(
        &managed_args,
        session_manager,
        session_id,
        event_tx,
        thread_id,
    )
    .await
}

async fn execute_bash_command(
    args: &serde_json::Value,
    session_manager: &Arc<SessionManager>,
    session_id: Option<SessionId>,
    event_tx: &broadcast::Sender<AgentEvent>,
    thread_id: &str,
) -> Result<(String, Option<ToolPendingApproval>)> {
    let managed_args =
        managed_alias_args(args, "Run a shell command in a managed terminal session");
    execute_managed_command(
        &managed_args,
        session_manager,
        session_id,
        event_tx,
        thread_id,
    )
    .await
}

fn managed_alias_args(args: &serde_json::Value, fallback_rationale: &str) -> serde_json::Value {
    let command = args
        .get("command")
        .and_then(|value| value.as_str())
        .unwrap_or_default();
    let rationale = args
        .get("rationale")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(fallback_rationale);

    let mut mapped = serde_json::Map::new();
    mapped.insert(
        "command".to_string(),
        serde_json::Value::String(command.to_string()),
    );
    mapped.insert(
        "rationale".to_string(),
        serde_json::Value::String(rationale.to_string()),
    );

    for key in [
        "session",
        "cwd",
        "allow_network",
        "sandbox_enabled",
        "security_level",
        "language_hint",
        "wait_for_completion",
        "timeout_seconds",
    ] {
        if let Some(value) = args.get(key) {
            mapped.insert(key.to_string(), value.clone());
        }
    }
    if !mapped.contains_key("security_level") {
        mapped.insert(
            "security_level".to_string(),
            serde_json::Value::String("lowest".to_string()),
        );
    }

    serde_json::Value::Object(mapped)
}

async fn execute_managed_command(
    args: &serde_json::Value,
    session_manager: &Arc<SessionManager>,
    session_id: Option<SessionId>,
    event_tx: &broadcast::Sender<AgentEvent>,
    thread_id: &str,
) -> Result<(String, Option<ToolPendingApproval>)> {
    let command = args
        .get("command")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'command' argument"))?;
    let rationale = args
        .get("rationale")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'rationale' argument"))?;

    let sessions = session_manager.list().await;
    if sessions.is_empty() {
        anyhow::bail!("No active terminal sessions are available for managed execution");
    }

    let resolved_session_id =
        if let Some(session_ref) = args.get("session").and_then(|v| v.as_str()) {
            sessions
                .iter()
                .find(|session| {
                    session.id.to_string() == session_ref
                        || session.id.to_string().contains(session_ref)
                })
                .map(|session| session.id)
                .ok_or_else(|| anyhow::anyhow!("session not found: {session_ref}"))?
        } else {
            session_id.unwrap_or(sessions[0].id)
        };

    let security_level = match args
        .get("security_level")
        .and_then(|value| value.as_str())
        .unwrap_or("lowest")
    {
        "highest" => SecurityLevel::Highest,
        "lowest" => SecurityLevel::Lowest,
        "yolo" => SecurityLevel::Yolo,
        _ => SecurityLevel::Moderate,
    };
    let requested_timeout = args
        .get("timeout_seconds")
        .and_then(|value| value.as_u64())
        .unwrap_or(30);
    let timeout_secs = requested_timeout.min(600);
    // Auto-background: if requested timeout exceeds max, run in background with monitoring
    let auto_background = requested_timeout > 600;
    let wait_for_completion = if auto_background {
        false
    } else {
        args.get("wait_for_completion")
            .and_then(|value| value.as_bool())
            .unwrap_or(true)
    };
    let mut wait_rx = if wait_for_completion {
        Some(session_manager.subscribe(resolved_session_id).await?.0)
    } else {
        None
    };

    let request = ManagedCommandRequest {
        command: command.to_string(),
        rationale: rationale.to_string(),
        allow_network: args
            .get("allow_network")
            .and_then(|value| value.as_bool())
            .unwrap_or(false),
        sandbox_enabled: args
            .get("sandbox_enabled")
            .and_then(|value| value.as_bool())
            .unwrap_or(false),
        security_level,
        cwd: args
            .get("cwd")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned),
        language_hint: args
            .get("language_hint")
            .and_then(|value| value.as_str())
            .map(ToOwned::to_owned),
        source: ManagedCommandSource::Agent,
    };

    match session_manager
        .execute_managed_command(resolved_session_id, request)
        .await?
    {
        DaemonMessage::ManagedCommandQueued {
            execution_id,
            position,
            snapshot,
            ..
        } => {
            let snapshot_suffix = snapshot
                .as_ref()
                .map(|item| format!(" (snapshot: {})", item.snapshot_id))
                .unwrap_or_default();
            let queued_summary = format!(
                "Managed command queued in session {} as {} at lane position {}{}",
                resolved_session_id, execution_id, position, snapshot_suffix
            );

            if !wait_for_completion {
                // Spawn background monitor if auto-backgrounded due to high timeout
                if auto_background {
                    let sm = session_manager.clone();
                    let sid = resolved_session_id.clone();
                    let eid = execution_id.clone();
                    let etx = event_tx.clone();
                    let tid = thread_id.to_string();
                    let monitor_timeout = requested_timeout;
                    tokio::spawn(async move {
                        if let Ok((rx, _)) = sm.subscribe(sid).await {
                            let mut rx = rx;
                            match wait_for_managed_command_outcome(
                                &mut rx,
                                sid,
                                &eid,
                                monitor_timeout,
                            )
                            .await
                            {
                                Ok(ManagedCommandWaitOutcome::Finished {
                                    exit_code,
                                    duration_ms,
                                    output_tail,
                                }) => {
                                    let timing = duration_ms
                                        .map(|v| format!(" in {}ms", v))
                                        .unwrap_or_default();
                                    let status = if exit_code == Some(0) {
                                        "completed successfully"
                                    } else {
                                        "failed"
                                    };
                                    let msg = format!(
                                        "Background command {} {}{} (exit_code: {:?})\n\nOutput (tail):\n{}",
                                        eid, status, timing, exit_code, output_tail
                                    );
                                    let _ = etx.send(AgentEvent::Delta {
                                        thread_id: tid.clone(),
                                        content: format!("\n\n[Background monitor] {msg}"),
                                    });
                                    let _ = etx.send(AgentEvent::WorkflowNotice {
                                        thread_id: tid,
                                        kind: "background-command-finished".to_string(),
                                        message: msg,
                                        details: None,
                                    });
                                }
                                Ok(ManagedCommandWaitOutcome::Timeout { output_tail }) => {
                                    let _ = etx.send(AgentEvent::WorkflowNotice {
                                        thread_id: tid,
                                        kind: "background-command-timeout".to_string(),
                                        message: format!(
                                            "Background command {} still running after {}s. Last output:\n{}",
                                            eid, monitor_timeout, output_tail
                                        ),
                                        details: None,
                                    });
                                }
                                _ => {}
                            }
                        }
                    });
                    return Ok((
                        format!(
                            "{queued_summary}\nCommand auto-backgrounded (requested timeout {}s > max 600s). \
                             A background monitor will notify this thread when the command completes.",
                            requested_timeout
                        ),
                        None,
                    ));
                }
                return Ok((
                    format!(
                        "{queued_summary}\nNot waiting for completion because wait_for_completion=false."
                    ),
                    None,
                ));
            }

            let Some(ref mut rx) = wait_rx else {
                return Ok((queued_summary, None));
            };

            match wait_for_managed_command_outcome(
                rx,
                resolved_session_id,
                &execution_id,
                timeout_secs,
            )
            .await?
            {
                ManagedCommandWaitOutcome::Finished {
                    exit_code,
                    duration_ms,
                    output_tail,
                } => {
                    let timing = duration_ms
                        .map(|value| format!(" in {}ms", value))
                        .unwrap_or_default();
                    if exit_code == Some(0) {
                        let output_section = if output_tail.trim().is_empty() {
                            String::new()
                        } else {
                            format!("\n\nTerminal output (tail):\n{output_tail}")
                        };
                        Ok((
                            format!(
                                "Managed command finished{timing} in session {} (execution_id: {}, exit_code: 0).{}",
                                resolved_session_id, execution_id, output_section
                            ),
                            None,
                        ))
                    } else {
                        let output_section = if output_tail.trim().is_empty() {
                            String::new()
                        } else {
                            format!("\n\nTerminal output (tail):\n{output_tail}")
                        };
                        Err(anyhow::anyhow!(
                            "Managed command failed in session {} (execution_id: {}, exit_code: {:?}).{}",
                            resolved_session_id,
                            execution_id,
                            exit_code,
                            output_section
                        ))
                    }
                }
                ManagedCommandWaitOutcome::Rejected { message } => Err(anyhow::anyhow!(
                    "Managed command rejected after queueing (execution_id: {}): {}",
                    execution_id,
                    message
                )),
                ManagedCommandWaitOutcome::Timeout { output_tail } => {
                    let output_section = if output_tail.trim().is_empty() {
                        String::new()
                    } else {
                        format!("\n\nTerminal output so far (tail):\n{output_tail}")
                    };
                    Err(anyhow::anyhow!(
                        "{queued_summary}\nManaged command is still running after {}s in session {}. Do not reuse this terminal for additional blocking work. Continue monitoring this execution_id or switch to another terminal/session before proceeding. If you need another lane in the same workspace, call allocate_terminal first.{}",
                        timeout_secs,
                        resolved_session_id,
                        output_section
                    ))
                }
            }
        }
        DaemonMessage::ApprovalRequired { approval, .. } => Ok((
            format!(
                "Managed command requires approval before execution. Approval ID: {}\nRisk: {}\nBlast radius: {}\nCommand: {}",
                approval.approval_id,
                approval.risk_level,
                approval.blast_radius,
                approval.command,
            ),
            Some(ToolPendingApproval {
                approval_id: approval.approval_id,
                execution_id: approval.execution_id,
                command: approval.command,
                rationale: approval.rationale,
                risk_level: approval.risk_level,
                blast_radius: approval.blast_radius,
                reasons: approval.reasons,
                session_id: Some(resolved_session_id.to_string()),
            }),
        )),
        other => Err(anyhow::anyhow!(
            "unexpected managed command response: {}",
            serde_json::to_string(&other).unwrap_or_else(|_| "<unserializable>".to_string())
        )),
    }
}

#[derive(Clone)]
struct AllocatedTerminalLane {
    source_session_id: SessionId,
    source_active_command: Option<String>,
    workspace_id: String,
    session_id: SessionId,
    pane_name: String,
}

async fn allocate_terminal_lane(
    args: &serde_json::Value,
    session_manager: &Arc<SessionManager>,
    preferred_session_id: Option<SessionId>,
    event_tx: &broadcast::Sender<AgentEvent>,
    default_pane_name: &str,
) -> Result<AllocatedTerminalLane> {
    let sessions = session_manager.list().await;
    if sessions.is_empty() {
        anyhow::bail!("No active terminal sessions are available to allocate another terminal");
    }

    let source_session =
        if let Some(session_ref) = args.get("session").and_then(|value| value.as_str()) {
            sessions
                .iter()
                .find(|session| {
                    session.id.to_string() == session_ref
                        || session.id.to_string().contains(session_ref)
                })
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("session not found: {session_ref}"))?
        } else {
            let resolved_id = preferred_session_id.unwrap_or(sessions[0].id);
            sessions
                .iter()
                .find(|session| session.id == resolved_id)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("session not found: {resolved_id}"))?
        };

    let workspace_id = source_session.workspace_id.clone().ok_or_else(|| {
        anyhow::anyhow!(
            "session {} is not attached to a workspace; cannot allocate another terminal lane",
            source_session.id
        )
    })?;
    let pane_name = args
        .get("pane_name")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| default_pane_name.to_string());
    let cwd = args
        .get("cwd")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| source_session.cwd.clone());

    let (new_session_id, _, source_active_command) = session_manager
        .clone_session(
            source_session.id,
            Some(workspace_id.clone()),
            None,
            None,
            false,
            cwd.clone(),
        )
        .await?;

    let _ = event_tx.send(AgentEvent::WorkspaceCommand {
        command: "attach_agent_terminal".to_string(),
        args: serde_json::json!({
            "workspace_id": workspace_id.clone(),
            "session_id": new_session_id.to_string(),
            "pane_name": pane_name.clone(),
            "cwd": cwd.clone(),
        }),
    });

    Ok(AllocatedTerminalLane {
        source_session_id: source_session.id,
        source_active_command,
        workspace_id,
        session_id: new_session_id,
        pane_name,
    })
}

async fn execute_allocate_terminal(
    args: &serde_json::Value,
    session_manager: &Arc<SessionManager>,
    preferred_session_id: Option<SessionId>,
    event_tx: &broadcast::Sender<AgentEvent>,
) -> Result<String> {
    let default_pane_name =
        if let Some(session_ref) = args.get("session").and_then(|value| value.as_str()) {
            let sessions = session_manager.list().await;
            let workspace_id = sessions
                .iter()
                .find(|session| {
                    session.id.to_string() == session_ref
                        || session.id.to_string().contains(session_ref)
                })
                .and_then(|session| session.workspace_id.as_ref())
                .cloned();
            if let Some(workspace_id) = workspace_id {
                format!(
                    "Work {}",
                    session_manager.list_workspace(&workspace_id).await.len() + 1
                )
            } else {
                "Work".to_string()
            }
        } else {
            "Work".to_string()
        };
    let lane = allocate_terminal_lane(
        args,
        session_manager,
        preferred_session_id,
        event_tx,
        &default_pane_name,
    )
    .await?;

    let source_command_suffix = lane
        .source_active_command
        .as_deref()
        .map(|command| format!("\nSource session active command: {command}"))
        .unwrap_or_default();
    Ok(format!(
        "Allocated terminal {} in workspace {} from source session {}. Frontend attachment requested for pane \"{}\". Use the new session ID for subsequent managed commands.{}",
        lane.session_id,
        lane.workspace_id,
        lane.source_session_id,
        lane.pane_name,
        source_command_suffix
    ))
}

fn normalize_task_runtime(value: Option<&str>) -> Result<String> {
    match value.unwrap_or("daemon").trim() {
        "" | "daemon" => Ok("daemon".to_string()),
        "hermes" => Ok("hermes".to_string()),
        "openclaw" => Ok("openclaw".to_string()),
        other => Err(anyhow::anyhow!("unsupported subagent runtime: {other}")),
    }
}

async fn execute_spawn_subagent(
    args: &serde_json::Value,
    agent: &AgentEngine,
    thread_id: &str,
    task_id: Option<&str>,
    session_manager: &Arc<SessionManager>,
    preferred_session_id: Option<SessionId>,
    event_tx: &broadcast::Sender<AgentEvent>,
) -> Result<String> {
    let title = args
        .get("title")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'title' argument"))?
        .to_string();
    let description = args
        .get("description")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'description' argument"))?
        .to_string();
    let runtime = normalize_task_runtime(args.get("runtime").and_then(|value| value.as_str()))?;
    if runtime != "daemon" {
        let status = agent
            .external_agent_status(&runtime)
            .await
            .ok_or_else(|| anyhow::anyhow!("runtime {runtime} is not configured"))?;
        if !status.available {
            anyhow::bail!("runtime {runtime} is not available on this machine");
        }
    }

    let priority = args
        .get("priority")
        .and_then(|value| value.as_str())
        .unwrap_or("normal");
    let command = args
        .get("command")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let dependencies = args
        .get("dependencies")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let task_snapshot = if let Some(current_task_id) = task_id {
        agent
            .list_tasks()
            .await
            .into_iter()
            .find(|task| task.id == current_task_id)
    } else {
        None
    };

    let mut chosen_session = args
        .get("session")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let mut allocated_lane_summary = None;
    if chosen_session.is_none() {
        let default_source_session = task_snapshot
            .as_ref()
            .and_then(|task| task.session_id.as_deref())
            .map(ToOwned::to_owned);
        let lane_request = serde_json::json!({
            "session": default_source_session,
            "cwd": args.get("cwd").and_then(|value| value.as_str()),
            "pane_name": format!("Subagent · {}", title.chars().take(24).collect::<String>()),
        });
        if let Ok(lane) = allocate_terminal_lane(
            &lane_request,
            session_manager,
            preferred_session_id,
            event_tx,
            "Subagent",
        )
        .await
        {
            chosen_session = Some(lane.session_id.to_string());
            allocated_lane_summary = Some(format!(
                "allocated terminal {} in workspace {} as \"{}\"",
                lane.session_id, lane.workspace_id, lane.pane_name
            ));
        }
    }

    let subagent = agent
        .enqueue_task(
            title,
            description,
            priority,
            command,
            chosen_session,
            dependencies,
            None,
            "subagent",
            task_snapshot
                .as_ref()
                .and_then(|task| task.goal_run_id.clone()),
            task_id.map(ToOwned::to_owned),
            Some(thread_id.to_string()),
            Some(runtime.clone()),
        )
        .await;

    let lane_suffix = allocated_lane_summary
        .map(|value| format!("\nDedicated lane: {value}"))
        .unwrap_or_default();
    Ok(format!(
        "Spawned subagent {} with runtime {}.{}",
        subagent.id, runtime, lane_suffix
    ))
}

async fn execute_list_subagents(
    args: &serde_json::Value,
    agent: &AgentEngine,
    thread_id: &str,
    task_id: Option<&str>,
) -> Result<String> {
    let status_filter = args
        .get("status")
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_ascii_lowercase());
    let parent_task_id = args
        .get("parent_task_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| task_id.map(ToOwned::to_owned));
    let parent_thread_id = args
        .get("parent_thread_id")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .or_else(|| Some(thread_id.to_string()));
    let limit = args
        .get("limit")
        .and_then(|value| value.as_u64())
        .map(|value| value as usize)
        .unwrap_or(20);

    let mut subagents = agent
        .list_tasks()
        .await
        .into_iter()
        .filter(|task| {
            if task.source != "subagent" {
                return false;
            }
            parent_task_id
                .as_deref()
                .map(|value| task.parent_task_id.as_deref() == Some(value))
                .unwrap_or(false)
                || parent_thread_id
                    .as_deref()
                    .map(|value| task.parent_thread_id.as_deref() == Some(value))
                    .unwrap_or(false)
        })
        .collect::<Vec<_>>();

    if let Some(status_filter) = status_filter {
        subagents.retain(|task| {
            serde_json::to_value(task.status)
                .ok()
                .and_then(|value| value.as_str().map(ToOwned::to_owned))
                .map(|value| value == status_filter)
                .unwrap_or(false)
        });
    }

    subagents.truncate(limit);
    Ok(serde_json::to_string_pretty(&subagents).unwrap_or_else(|_| "[]".to_string()))
}

async fn execute_enqueue_task(args: &serde_json::Value, agent: &AgentEngine) -> Result<String> {
    let description = args
        .get("description")
        .and_then(|value| value.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'description' argument"))?
        .trim()
        .to_string();
    if description.is_empty() {
        anyhow::bail!("'description' must not be empty");
    }

    let command = args
        .get("command")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let title = args
        .get("title")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| default_task_title(&description, command.as_deref()));
    let priority = args
        .get("priority")
        .and_then(|value| value.as_str())
        .unwrap_or("normal");
    let session = args
        .get("session")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned);
    let dependencies = args
        .get("dependencies")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let scheduled_at = parse_scheduled_at(args)?;

    let task = agent
        .enqueue_task(
            title,
            description,
            priority,
            command,
            session,
            dependencies,
            scheduled_at,
            "agent",
            None,
            None,
            None,
            None,
        )
        .await;

    Ok(serde_json::to_string_pretty(&task).unwrap_or_else(|_| format!("queued task {}", task.id)))
}

async fn execute_list_tasks(args: &serde_json::Value, agent: &AgentEngine) -> Result<String> {
    let status_filter = args
        .get("status")
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_ascii_lowercase());
    let limit = args
        .get("limit")
        .and_then(|value| value.as_u64())
        .map(|value| value as usize);

    let mut tasks = agent.list_tasks().await;
    if let Some(status_filter) = status_filter {
        tasks.retain(|task| {
            serde_json::to_value(task.status)
                .ok()
                .and_then(|value| value.as_str().map(ToOwned::to_owned))
                .map(|value| value == status_filter)
                .unwrap_or(false)
        });
    }
    if let Some(limit) = limit {
        tasks.truncate(limit);
    }

    Ok(serde_json::to_string_pretty(&tasks).unwrap_or_else(|_| "[]".to_string()))
}

async fn execute_cancel_task(args: &serde_json::Value, agent: &AgentEngine) -> Result<String> {
    let task_id = args
        .get("task_id")
        .and_then(|value| value.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'task_id' argument"))?;
    let cancelled = agent.cancel_task(task_id).await;
    Ok(serde_json::json!({
        "task_id": task_id,
        "cancelled": cancelled,
    })
    .to_string())
}

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

// ---------------------------------------------------------------------------
// Gateway messaging — execute via CLI subprocess
// ---------------------------------------------------------------------------

async fn execute_gateway_message(
    tool_name: &str,
    args: &serde_json::Value,
    http_client: &reqwest::Client,
    agent_data_dir: &std::path::Path,
) -> Result<String> {
    let message = args
        .get("message")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing 'message' argument"))?;

    // Read gateway tokens from agent config or settings.json
    let config_path = agent_data_dir.join("config.json");
    let settings_path = agent_data_dir
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join("settings.json");

    let (slack_token, telegram_token, discord_token) = {
        // Try agent config first
        let mut st = String::new();
        let mut tt = String::new();
        let mut dt = String::new();

        if let Ok(raw) = tokio::fs::read_to_string(&config_path).await {
            if let Ok(cfg) = serde_json::from_str::<serde_json::Value>(&raw) {
                st = cfg
                    .pointer("/gateway/slack_token")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                tt = cfg
                    .pointer("/gateway/telegram_token")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                dt = cfg
                    .pointer("/gateway/discord_token")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
            }
        }

        // Fall back to settings.json
        if st.is_empty() && tt.is_empty() && dt.is_empty() {
            if let Ok(raw) = tokio::fs::read_to_string(&settings_path).await {
                if let Ok(s) = serde_json::from_str::<serde_json::Value>(&raw) {
                    st = s
                        .pointer("/settings/slackToken")
                        .or_else(|| s.get("slackToken"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    tt = s
                        .pointer("/settings/telegramToken")
                        .or_else(|| s.get("telegramToken"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    dt = s
                        .pointer("/settings/discordToken")
                        .or_else(|| s.get("discordToken"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                }
            }
        }

        (st, tt, dt)
    };

    // Read default targets from settings for when user doesn't specify
    let settings: serde_json::Value =
        if let Ok(raw) = tokio::fs::read_to_string(&settings_path).await {
            serde_json::from_str(&raw).unwrap_or_default()
        } else {
            serde_json::Value::Null
        };

    let setting = |key: &str| -> String {
        settings
            .pointer(&format!("/settings/{key}"))
            .or_else(|| settings.get(key))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string()
    };

    let first_csv =
        |val: &str| -> String { val.split(',').next().unwrap_or("").trim().to_string() };

    match tool_name {
        "send_slack_message" => {
            let channel = args
                .get("channel")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| first_csv(&setting("slackChannelFilter")));
            if channel.is_empty() {
                return Err(anyhow::anyhow!(
                    "No channel specified and no default slackChannelFilter in settings"
                ));
            }
            let channel = channel.as_str();
            if slack_token.is_empty() {
                return Err(anyhow::anyhow!("Slack token not configured in settings"));
            }
            tracing::info!(platform = "slack", channel = %channel, message = %message, "gateway: sending message");
            let resp = http_client
                .post("https://slack.com/api/chat.postMessage")
                .bearer_auth(&slack_token)
                .json(&serde_json::json!({ "channel": channel, "text": message }))
                .send()
                .await?;
            let body: serde_json::Value = resp.json().await?;
            if body.get("ok").and_then(|v| v.as_bool()) == Some(true) {
                Ok(format!("Slack message sent to #{channel}"))
            } else {
                let err = body
                    .get("error")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown error");
                Err(anyhow::anyhow!("Slack API error: {err}"))
            }
        }
        "send_discord_message" => {
            let mut channel_id = args
                .get("channel_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let mut user_id = args
                .get("user_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            // Fall back to defaults from settings
            if channel_id.is_empty() && user_id.is_empty() {
                let default_channel = first_csv(&setting("discordChannelFilter"));
                if !default_channel.is_empty() {
                    channel_id = default_channel;
                } else {
                    let default_user = first_csv(&setting("discordAllowedUsers"));
                    if !default_user.is_empty() {
                        user_id = default_user;
                    }
                }
            }
            let channel_id = channel_id.as_str();
            let user_id = user_id.as_str();
            if discord_token.is_empty() {
                return Err(anyhow::anyhow!("Discord token not configured in settings"));
            }

            let target_channel = if !channel_id.is_empty() {
                channel_id.to_string()
            } else if !user_id.is_empty() {
                // Create DM channel first
                let resp = http_client
                    .post("https://discord.com/api/v10/users/@me/channels")
                    .header("Authorization", format!("Bot {discord_token}"))
                    .json(&serde_json::json!({ "recipient_id": user_id }))
                    .send()
                    .await?;
                let body: serde_json::Value = resp.json().await?;
                body.get("id")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| anyhow::anyhow!("Failed to create DM channel: {body}"))?
                    .to_string()
            } else {
                return Err(anyhow::anyhow!("Either channel_id or user_id is required"));
            };

            tracing::info!(platform = "discord", channel = %target_channel, message = %message, "gateway: sending message");
            let resp = http_client
                .post(format!(
                    "https://discord.com/api/v10/channels/{target_channel}/messages"
                ))
                .header("Authorization", format!("Bot {discord_token}"))
                .json(&serde_json::json!({ "content": message }))
                .send()
                .await?;

            if resp.status().is_success() {
                Ok(format!("Discord message sent to {target_channel}"))
            } else {
                let body = resp.text().await.unwrap_or_default();
                Err(anyhow::anyhow!("Discord API error: {body}"))
            }
        }
        "send_telegram_message" => {
            let chat_id = args
                .get("chat_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| first_csv(&setting("telegramAllowedChats")));
            if chat_id.is_empty() {
                return Err(anyhow::anyhow!(
                    "No chat_id specified and no default telegramAllowedChats in settings"
                ));
            }
            let chat_id = chat_id.as_str();
            if telegram_token.is_empty() {
                return Err(anyhow::anyhow!("Telegram token not configured in settings"));
            }
            tracing::info!(platform = "telegram", chat_id = %chat_id, message = %message, "gateway: sending message");
            let url = format!("https://api.telegram.org/bot{telegram_token}/sendMessage");
            let resp = http_client
                .post(&url)
                .json(&serde_json::json!({ "chat_id": chat_id, "text": message }))
                .send()
                .await?;
            let body: serde_json::Value = resp.json().await?;
            if body.get("ok").and_then(|v| v.as_bool()) == Some(true) {
                Ok(format!("Telegram message sent to {chat_id}"))
            } else {
                let desc = body
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown error");
                Err(anyhow::anyhow!("Telegram API error: {desc}"))
            }
        }
        "send_whatsapp_message" => {
            let phone = args
                .get("phone")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| first_csv(&setting("whatsappAllowedContacts")));
            if phone.is_empty() {
                return Err(anyhow::anyhow!(
                    "No phone specified and no default whatsappAllowedContacts in settings"
                ));
            }
            let phone = phone.as_str();
            // WhatsApp requires separate phone_number_id and token — read from settings
            let settings: serde_json::Value =
                if let Ok(raw) = tokio::fs::read_to_string(&settings_path).await {
                    serde_json::from_str(&raw).unwrap_or_default()
                } else {
                    serde_json::Value::Null
                };
            let wa_token = settings
                .pointer("/settings/whatsappToken")
                .or_else(|| settings.get("whatsappToken"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let phone_id = settings
                .pointer("/settings/whatsappPhoneNumberId")
                .or_else(|| settings.get("whatsappPhoneNumberId"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if wa_token.is_empty() || phone_id.is_empty() {
                return Err(anyhow::anyhow!(
                    "WhatsApp token/phoneNumberId not configured"
                ));
            }
            tracing::info!(platform = "whatsapp", phone = %phone, "gateway: sending message");
            let url = format!("https://graph.facebook.com/v18.0/{phone_id}/messages");
            let resp = http_client
                .post(&url)
                .bearer_auth(wa_token)
                .json(&serde_json::json!({
                    "messaging_product": "whatsapp",
                    "to": phone,
                    "type": "text",
                    "text": { "body": message }
                }))
                .send()
                .await?;
            if resp.status().is_success() {
                Ok(format!("WhatsApp message sent to {phone}"))
            } else {
                let body = resp.text().await.unwrap_or_default();
                Err(anyhow::anyhow!("WhatsApp API error: {body}"))
            }
        }
        _ => Err(anyhow::anyhow!("unknown gateway tool")),
    }
}

// ---------------------------------------------------------------------------
// Workspace/snippet tools — read/write persistence files
// ---------------------------------------------------------------------------

async fn execute_workspace_tool(
    tool_name: &str,
    args: &serde_json::Value,
    event_tx: &broadcast::Sender<AgentEvent>,
) -> Result<String> {
    let data_dir = super::agent_data_dir()
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .to_path_buf();

    match tool_name {
        "list_workspaces" => {
            let session_path = data_dir.join("session.json");
            match tokio::fs::read_to_string(&session_path).await {
                Ok(raw) => {
                    let parsed: serde_json::Value = serde_json::from_str(&raw)?;
                    let workspaces = parsed.get("workspaces").and_then(|w| w.as_array());
                    match workspaces {
                        Some(ws) => {
                            let mut lines = Vec::new();
                            for w in ws {
                                let name = w.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                                let id = w.get("id").and_then(|v| v.as_str()).unwrap_or("?");
                                let surfaces = w
                                    .get("surfaces")
                                    .and_then(|v| v.as_array())
                                    .map(|s| s.len())
                                    .unwrap_or(0);
                                lines.push(format!("- {name} (id: {id}, {surfaces} surfaces)"));
                            }
                            Ok(lines.join("\n"))
                        }
                        None => Ok("No workspaces found.".into()),
                    }
                }
                Err(_) => Ok("No session file found (app may not have saved state yet).".into()),
            }
        }
        "list_snippets" => {
            let snippets_path = data_dir.join("snippets.json");
            match tokio::fs::read_to_string(&snippets_path).await {
                Ok(raw) => {
                    let parsed: serde_json::Value = serde_json::from_str(&raw)?;
                    let snippets = parsed.as_array();
                    match snippets {
                        Some(ss) => {
                            let mut lines = Vec::new();
                            for s in ss {
                                let name = s.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                                let content =
                                    s.get("content").and_then(|v| v.as_str()).unwrap_or("");
                                let preview: String = content.chars().take(60).collect();
                                lines.push(format!("- {name}: {preview}"));
                            }
                            Ok(lines.join("\n"))
                        }
                        None => Ok("No snippets found.".into()),
                    }
                }
                Err(_) => Ok("No snippets file found.".into()),
            }
        }
        // Mutation tools — emit WorkspaceCommand event for frontend execution
        other => {
            let _ = event_tx.send(AgentEvent::WorkspaceCommand {
                command: other.to_string(),
                args: args.clone(),
            });
            Ok(format!("Executed {other}"))
        }
    }
}

fn strip_ansi_codes(text: &str) -> String {
    // Simple ANSI escape stripping
    let mut result = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            // Skip escape sequence
            if let Some(&next) = chars.peek() {
                if next == '[' {
                    chars.next();
                    // Skip until terminator (letter)
                    while let Some(&c) = chars.peek() {
                        chars.next();
                        if c.is_ascii_alphabetic() || c == '~' {
                            break;
                        }
                    }
                } else if next == ']' {
                    chars.next();
                    // Skip OSC until BEL or ST
                    while let Some(c) = chars.next() {
                        if c == '\x07' {
                            break;
                        }
                        if c == '\x1b' {
                            if chars.peek() == Some(&'\\') {
                                chars.next();
                                break;
                            }
                        }
                    }
                }
            }
        } else {
            result.push(ch);
        }
    }
    result
}

fn strip_html_tags(html: &str) -> String {
    let mut result = String::with_capacity(html.len());
    let mut in_tag = false;
    let mut in_script = false;
    let mut in_style = false;

    let lower = html.to_lowercase();
    let chars: Vec<char> = html.chars().collect();
    let lower_chars: Vec<char> = lower.chars().collect();

    let mut i = 0;
    while i < chars.len() {
        if !in_tag && chars[i] == '<' {
            // Check for script/style
            let remaining: String = lower_chars[i..].iter().take(10).collect();
            if remaining.starts_with("<script") {
                in_script = true;
            } else if remaining.starts_with("<style") {
                in_style = true;
            } else if remaining.starts_with("</script") {
                in_script = false;
            } else if remaining.starts_with("</style") {
                in_style = false;
            }
            in_tag = true;
        } else if in_tag && chars[i] == '>' {
            in_tag = false;
        } else if !in_tag && !in_script && !in_style {
            result.push(chars[i]);
        }
        i += 1;
    }

    // Collapse whitespace
    let mut collapsed = String::new();
    let mut last_was_space = false;
    for ch in result.chars() {
        if ch.is_whitespace() {
            if !last_was_space {
                collapsed.push(if ch == '\n' { '\n' } else { ' ' });
                last_was_space = true;
            }
        } else {
            collapsed.push(ch);
            last_was_space = false;
        }
    }

    collapsed.trim().to_string()
}

// Minimal URL encoding (only used for web_search query)
mod urlencoding {
    pub fn encode(s: &str) -> String {
        let mut result = String::new();
        for byte in s.bytes() {
            match byte {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                    result.push(byte as char);
                }
                b' ' => result.push('+'),
                _ => {
                    result.push('%');
                    result.push_str(&format!("{:02X}", byte));
                }
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_list_files_script, build_write_file_command, build_write_file_script,
        managed_alias_args, parse_capture_output, resolve_skill_path, validate_read_path,
        validate_write_path,
    };
    use base64::Engine;
    use std::fs;

    #[test]
    fn write_file_rejects_paths_with_trailing_whitespace() {
        let error = validate_write_path("/tmp/Dockerfile ")
            .expect_err("write_file should reject trailing whitespace");
        assert!(error.to_string().contains("leading/trailing whitespace"));
    }

    #[test]
    fn write_file_rejects_paths_with_control_characters() {
        let error = validate_write_path("/tmp/dock\nerfile")
            .expect_err("write_file should reject control characters");
        assert!(error.to_string().contains("control characters"));
    }

    #[test]
    fn write_file_command_encodes_path_and_content() {
        let command = build_write_file_command("/tmp/Dockerfile", "FROM scratch\n");
        assert!(command.contains("python3 -c"));
        assert!(command.contains("base64.b64decode"));
        assert!(!command.contains("/tmp/Dockerfile"));
        assert!(!command.contains("FROM scratch"));
    }

    #[test]
    fn write_file_script_keeps_python_block_indentation() {
        let script = build_write_file_script("cGF0aA==", "Y29udGVudA==");
        assert!(script.contains("\nif actual != expected:\n    raise SystemExit("));
    }

    #[test]
    fn list_files_rejects_paths_with_control_characters() {
        let error = validate_read_path("/tmp/ba\td")
            .expect_err("list_files should reject control characters");
        assert!(error.to_string().contains("control characters"));
    }

    #[test]
    fn parse_capture_output_decodes_payload_and_status() {
        let token = "tok123";
        let payload = "file\t12\tDockerfile\n";
        let encoded = base64::engine::general_purpose::STANDARD.encode(payload.as_bytes());
        let output = format!(
            "prefix\n__AMUX_CAPTURE_BEGIN_{token}__\n{encoded}\n__AMUX_CAPTURE_END_{token}__:0\nsuffix"
        );

        let parsed =
            parse_capture_output(output.as_bytes(), token).expect("capture output should parse");
        assert_eq!(parsed.0, 0);
        assert_eq!(parsed.1, payload);
    }

    #[test]
    fn list_files_script_keeps_python_try_indentation() {
        let script = build_list_files_script("L3RtcA==", "tok123");
        assert!(script.contains("\ntry:\n    rows = []\n    for entry in sorted("));
        assert!(script.contains("\nexcept Exception as exc:\n    payload = f'Error: {exc}'"));
    }

    #[test]
    fn managed_alias_defaults_security_level_to_lowest() {
        let args = serde_json::json!({
            "command": "echo hello"
        });
        let mapped = managed_alias_args(&args, "test rationale");
        let level = mapped
            .get("security_level")
            .and_then(|value| value.as_str())
            .expect("security_level should be set");
        assert_eq!(level, "lowest");
    }

    #[test]
    fn managed_alias_preserves_wait_controls() {
        let args = serde_json::json!({
            "command": "echo hello",
            "wait_for_completion": false,
            "timeout_seconds": 42
        });
        let mapped = managed_alias_args(&args, "test rationale");
        assert_eq!(
            mapped
                .get("wait_for_completion")
                .and_then(|value| value.as_bool()),
            Some(false)
        );
        assert_eq!(
            mapped
                .get("timeout_seconds")
                .and_then(|value| value.as_u64()),
            Some(42)
        );
    }

    #[test]
    fn resolve_skill_path_finds_generated_skill_by_stem() {
        let root = std::env::temp_dir().join(format!("tamux-skill-test-{}", uuid::Uuid::new_v4()));
        let generated = root.join("generated");
        fs::create_dir_all(&generated).expect("skill test directory should be created");
        let skill_path = generated.join("build-release.md");
        fs::write(&skill_path, "# Build release\n").expect("skill file should be written");

        let resolved =
            resolve_skill_path(&root, "build-release").expect("generated skill should resolve");
        assert_eq!(resolved, skill_path);

        let _ = fs::remove_dir_all(&root);
    }
}
