//! tamux-mcp: An MCP (Model Context Protocol) server that exposes the tamux
//! daemon's capabilities as tools over JSON-RPC stdio transport.
//!
//! Register this binary as an MCP server in Claude Code or other MCP clients:
//!
//! ```json
//! {
//!   "mcpServers": {
//!     "tamux": {
//!       "command": "tamux-mcp"
//!     }
//!   }
//! }
//! ```

use std::io::Write;
use std::path::{Path, PathBuf};

use amux_protocol::{
    AmuxCodec, ClientMessage, DaemonMessage, ManagedCommandRequest, ManagedCommandSource,
    SecurityLevel,
};
use anyhow::{Context, Result};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::time::{timeout, Duration};
use tokio_util::codec::Framed;
use tracing::{debug, error, info, warn};

// ---------------------------------------------------------------------------
// JSON-RPC types
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Option<Value>,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i64,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

impl JsonRpcResponse {
    fn success(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: Some(result),
            error: None,
        }
    }

    fn error(id: Option<Value>, code: i64, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }
}

// JSON-RPC error codes
const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
#[allow(dead_code)]
const INTERNAL_ERROR: i64 = -32603;

// ---------------------------------------------------------------------------
// MCP tool definitions
// ---------------------------------------------------------------------------

fn tool_definitions() -> Value {
    serde_json::json!([
        {
            "name": "execute_command",
            "description": "Execute a managed command inside a tamux terminal session. The command runs in a sandboxed lane with approval gating, automatic snapshots, and telemetry.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": {
                        "type": "string",
                        "description": "UUID of the target terminal session"
                    },
                    "command": {
                        "type": "string",
                        "description": "The shell command to execute"
                    },
                    "rationale": {
                        "type": "string",
                        "description": "Human-readable explanation of why this command is being run"
                    }
                },
                "required": ["session_id", "command", "rationale"]
            }
        },
        {
            "name": "search_history",
            "description": "Search the daemon's command and transcript history using full-text search.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Full-text search query"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results to return"
                    }
                },
                "required": ["query"]
            }
        },
        {
            "name": "find_symbol",
            "description": "Search for code symbols (functions, types, variables) using the daemon's semantic indexing.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace_root": {
                        "type": "string",
                        "description": "Absolute path to the workspace root directory"
                    },
                    "symbol": {
                        "type": "string",
                        "description": "Symbol name or pattern to search for"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results to return"
                    }
                },
                "required": ["workspace_root", "symbol"]
            }
        },
        {
            "name": "list_snapshots",
            "description": "List recorded workspace snapshots and checkpoints. Snapshots are created automatically before managed commands.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace_id": {
                        "type": "string",
                        "description": "Optional workspace ID to filter snapshots"
                    }
                },
                "required": []
            }
        },
        {
            "name": "restore_snapshot",
            "description": "Restore a previously recorded workspace snapshot, reverting the workspace to that point in time.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "snapshot_id": {
                        "type": "string",
                        "description": "ID of the snapshot to restore"
                    }
                },
                "required": ["snapshot_id"]
            }
        },
        {
            "name": "scrub_sensitive",
            "description": "Scrub sensitive data (secrets, tokens, passwords) from a text string using the daemon's detection engine.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "text": {
                        "type": "string",
                        "description": "Text to scrub for sensitive data"
                    }
                },
                "required": ["text"]
            }
        },
        {
            "name": "list_sessions",
            "description": "List all active terminal sessions and browser panels. Includes workspace/surface hierarchy, pane types (terminal/browser), URLs for browser panes, session IDs, CWD, and active commands.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "required": []
            }
        },
        {
            "name": "verify_integrity",
            "description": "Verify the integrity of WORM (Write-Once-Read-Many) telemetry ledgers to detect tampering.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "required": []
            }
        },
        {
            "name": "get_terminal_content",
            "description": "Read the scrollback buffer (visible content) of a terminal session. Use this to see what is currently displayed in a user's terminal pane.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": {
                        "type": "string",
                        "description": "UUID of the terminal session to read"
                    },
                    "max_lines": {
                        "type": "integer",
                        "description": "Maximum number of lines to return from the tail (default: 100)"
                    }
                },
                "required": ["session_id"]
            }
        },
        {
            "name": "type_in_terminal",
            "description": "Send keystrokes/input to a terminal session. Use this to type commands or interact with running programs in the user's terminal.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": {
                        "type": "string",
                        "description": "UUID of the terminal session"
                    },
                    "input": {
                        "type": "string",
                        "description": "Text to type into the terminal. Use \\n for Enter key."
                    }
                },
                "required": ["session_id", "input"]
            }
        },
        {
            "name": "enqueue_task",
            "description": "Queue a daemon-managed background task. Supports dependencies and scheduled execution for reminders, follow-up gateway messages, and deferred automation.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "title": {
                        "type": "string",
                        "description": "Optional short title for the task"
                    },
                    "description": {
                        "type": "string",
                        "description": "Detailed task instructions for the daemon agent"
                    },
                    "priority": {
                        "type": "string",
                        "enum": ["low", "normal", "high", "urgent"],
                        "description": "Task priority"
                    },
                    "command": {
                        "type": "string",
                        "description": "Optional preferred command or entrypoint"
                    },
                    "session_id": {
                        "type": "string",
                        "description": "Optional target terminal session"
                    },
                    "dependencies": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Task IDs that must complete first"
                    },
                    "scheduled_at": {
                        "type": "integer",
                        "description": "Optional Unix timestamp in milliseconds"
                    },
                    "schedule_at": {
                        "type": "string",
                        "description": "Optional RFC3339 timestamp"
                    },
                    "delay_seconds": {
                        "type": "integer",
                        "description": "Optional relative delay before task start"
                    }
                },
                "required": ["description"]
            }
        },
        {
            "name": "list_tasks",
            "description": "List daemon-managed background tasks with status, schedule, and dependency metadata.",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        },
        {
            "name": "cancel_task",
            "description": "Cancel a queued, blocked, running, or approval-pending daemon task by ID.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "task_id": {
                        "type": "string",
                        "description": "Task ID to cancel"
                    }
                },
                "required": ["task_id"]
            }
        },
        {
            "name": "list_todos",
            "description": "List daemon-managed planner todos for all agent threads.",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        },
        {
            "name": "get_todos",
            "description": "Fetch daemon-managed planner todos for a specific agent thread.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "thread_id": {
                        "type": "string",
                        "description": "Agent thread ID"
                    }
                },
                "required": ["thread_id"]
            }
        },
        {
            "name": "list_skills",
            "description": "List reusable local tamux skills from the platform-specific ~/.tamux/skills directory.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Optional skill name or path filter"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum skills to return"
                    }
                }
            }
        },
        {
            "name": "read_skill",
            "description": "Read a local tamux skill document by name, stem, or relative path under the skills directory.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "skill": {
                        "type": "string",
                        "description": "Skill name, file stem, or relative path"
                    },
                    "max_lines": {
                        "type": "integer",
                        "description": "Maximum lines to read"
                    }
                },
                "required": ["skill"]
            }
        },
        {
            "name": "start_goal_run",
            "description": "Start a durable goal run that plans, executes child tasks, handles approvals, and reflects on outcomes.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "goal": {
                        "type": "string",
                        "description": "The long-running objective to pursue"
                    },
                    "title": {
                        "type": "string",
                        "description": "Optional short title for the goal run"
                    },
                    "thread_id": {
                        "type": "string",
                        "description": "Optional existing agent thread to attach to"
                    },
                    "session_id": {
                        "type": "string",
                        "description": "Optional preferred terminal session"
                    },
                    "priority": {
                        "type": "string",
                        "enum": ["low", "normal", "high", "urgent"],
                        "description": "Goal priority"
                    }
                },
                "required": ["goal"]
            }
        },
        {
            "name": "list_goal_runs",
            "description": "List durable goal runs with status, current step, metrics, and history metadata.",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        },
        {
            "name": "get_goal_run",
            "description": "Fetch a specific goal run with full plan, events, and derived metrics.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "goal_run_id": {
                        "type": "string",
                        "description": "Goal run ID"
                    }
                },
                "required": ["goal_run_id"]
            }
        },
        {
            "name": "control_goal_run",
            "description": "Control a goal run lifecycle or rerun a specific step. Supported actions: pause, resume, cancel, retry_step, rerun_from_step.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "goal_run_id": {
                        "type": "string",
                        "description": "Goal run ID"
                    },
                    "action": {
                        "type": "string",
                        "enum": ["pause", "resume", "cancel", "retry_step", "rerun_from_step"],
                        "description": "Control action"
                    },
                    "step_index": {
                        "type": "integer",
                        "description": "Optional zero-based step index for retry_step or rerun_from_step"
                    }
                },
                "required": ["goal_run_id", "action"]
            }
        },
        {
            "name": "get_operator_model",
            "description": "Fetch the daemon's aggregate operator model, including learned cognitive style, risk tolerance, session rhythm, and attention topology.",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        },
        {
            "name": "get_causal_trace_report",
            "description": "Summarize causal trace outcomes for a tool or decision option, including success/failure counts and recent reasons.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "option_type": {
                        "type": "string",
                        "description": "Tool or option type, such as bash_command, execute_managed_command, goal_plan, or replan_after_failure"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of recent causal traces to aggregate"
                    }
                },
                "required": ["option_type"]
            }
        },
        {
            "name": "get_counterfactual_report",
            "description": "Estimate likely risk for a candidate command family using recent causal history for the given tool or option type.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "option_type": {
                        "type": "string",
                        "description": "Tool or option type, such as bash_command or execute_managed_command"
                    },
                    "command_family": {
                        "type": "string",
                        "description": "Representative command or family hint to compare against recent causal history"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of recent causal traces to inspect"
                    }
                },
                "required": ["option_type", "command_family"]
            }
        },
        {
            "name": "get_memory_provenance_report",
            "description": "Inspect durable tamux memory provenance with source, age-based confidence, and active/uncertain/retracted status.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "target": {
                        "type": "string",
                        "description": "Optional target memory file filter, such as MEMORY.md, USER.md, or SOUL.md"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of recent provenance entries to include"
                    }
                }
            }
        },
        {
            "name": "get_provenance_report",
            "description": "Inspect trusted execution provenance, including hash/signature validity and recent event summaries.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of recent provenance entries to include"
                    }
                }
            }
        },
        {
            "name": "generate_soc2_artifact",
            "description": "Generate a SOC2-style audit artifact from recent provenance events.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "period_days": {
                        "type": "integer",
                        "description": "How many recent days of provenance to include"
                    }
                }
            }
        },
        {
            "name": "get_collaboration_sessions",
            "description": "Inspect active subagent collaboration sessions, contributions, disagreements, and consensus state.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "parent_task_id": {
                        "type": "string",
                        "description": "Optional parent task ID to narrow to one collaboration session"
                    }
                }
            }
        },
        {
            "name": "list_generated_tools",
            "description": "List runtime-generated tools with status, effectiveness, and promotion metadata.",
            "inputSchema": {
                "type": "object",
                "properties": {}
            }
        },
        {
            "name": "synthesize_tool",
            "description": "Generate a guarded runtime tool from a conservative CLI or GET OpenAPI operation.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "kind": {
                        "type": "string",
                        "enum": ["cli", "openapi"]
                    },
                    "target": {
                        "type": "string",
                        "description": "CLI invocation or OpenAPI spec URL"
                    },
                    "name": {
                        "type": "string",
                        "description": "Optional generated tool name override"
                    },
                    "operation_id": {
                        "type": "string",
                        "description": "Optional OpenAPI operationId"
                    },
                    "activate": {
                        "type": "boolean",
                        "description": "Request immediate activation when policy allows it"
                    }
                },
                "required": ["target"]
            }
        },
        {
            "name": "run_generated_tool",
            "description": "Execute a generated runtime tool by name.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "tool_name": {
                        "type": "string",
                        "description": "Generated tool ID"
                    },
                    "args": {
                        "type": "object",
                        "description": "Arguments object for the generated tool"
                    }
                },
                "required": ["tool_name"]
            }
        },
        {
            "name": "promote_generated_tool",
            "description": "Promote a generated runtime tool into the generated skills library.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "tool_name": {
                        "type": "string",
                        "description": "Generated tool ID"
                    }
                },
                "required": ["tool_name"]
            }
        },
        {
            "name": "activate_generated_tool",
            "description": "Activate a newly synthesized runtime tool after review so it can be called on subsequent turns.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "tool_name": {
                        "type": "string",
                        "description": "Generated tool ID"
                    }
                },
                "required": ["tool_name"]
            }
        },
        {
            "name": "get_git_status",
            "description": "Get git status for a working directory, showing modified, staged, and untracked files.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": {
                        "type": "string",
                        "description": "Absolute path to the git repository"
                    }
                },
                "required": ["path"]
            }
        }
    ])
}

// ---------------------------------------------------------------------------
// Daemon IPC connection
// ---------------------------------------------------------------------------

/// Connect to the tamux daemon and return a framed codec stream.
async fn connect_daemon(
) -> Result<Framed<impl tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin, AmuxCodec>> {
    #[cfg(unix)]
    {
        let runtime_dir = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".to_string());
        let path = std::path::PathBuf::from(runtime_dir).join("tamux-daemon.sock");
        let stream = tokio::net::UnixStream::connect(&path)
            .await
            .with_context(|| format!("cannot connect to tamux daemon at {}", path.display()))?;
        Ok(Framed::new(stream, AmuxCodec))
    }

    #[cfg(windows)]
    {
        let addr = amux_protocol::default_tcp_addr();
        let stream = tokio::net::TcpStream::connect(&addr)
            .await
            .with_context(|| format!("cannot connect to tamux daemon on {addr}"))?;
        Ok(Framed::new(stream, AmuxCodec))
    }
}

/// Send a single message to the daemon and read back one response.
async fn daemon_roundtrip(msg: ClientMessage) -> Result<DaemonMessage> {
    let mut framed = connect_daemon().await?;
    framed.send(msg).await.context("failed to send to daemon")?;
    let resp = framed
        .next()
        .await
        .ok_or_else(|| anyhow::anyhow!("daemon closed connection before responding"))??;
    Ok(resp)
}

// ---------------------------------------------------------------------------
// Tool dispatch
// ---------------------------------------------------------------------------

/// Execute an MCP tool call against the daemon and return the result as JSON.
async fn handle_tool_call(name: &str, args: &Value) -> Value {
    match dispatch_tool(name, args).await {
        Ok(result) => serde_json::json!({
            "content": [{
                "type": "text",
                "text": serde_json::to_string_pretty(&result).unwrap_or_else(|_| result.to_string())
            }]
        }),
        Err(err) => serde_json::json!({
            "content": [{
                "type": "text",
                "text": format!("Error: {err:#}")
            }],
            "isError": true
        }),
    }
}

async fn dispatch_tool(name: &str, args: &Value) -> Result<Value> {
    match name {
        "execute_command" => tool_execute_command(args).await,
        "search_history" => tool_search_history(args).await,
        "find_symbol" => tool_find_symbol(args).await,
        "list_snapshots" => tool_list_snapshots(args).await,
        "restore_snapshot" => tool_restore_snapshot(args).await,
        "scrub_sensitive" => tool_scrub_sensitive(args).await,
        "list_sessions" => tool_list_sessions(args).await,
        "verify_integrity" => tool_verify_integrity(args).await,
        "get_terminal_content" => tool_get_terminal_content(args).await,
        "type_in_terminal" => tool_type_in_terminal(args).await,
        "enqueue_task" => tool_enqueue_task(args).await,
        "list_tasks" => tool_list_tasks().await,
        "cancel_task" => tool_cancel_task(args).await,
        "list_todos" => tool_list_todos().await,
        "get_todos" => tool_get_todos(args).await,
        "list_skills" => tool_list_skills(args).await,
        "read_skill" => tool_read_skill(args).await,
        "start_goal_run" => tool_start_goal_run(args).await,
        "list_goal_runs" => tool_list_goal_runs().await,
        "get_goal_run" => tool_get_goal_run(args).await,
        "control_goal_run" => tool_control_goal_run(args).await,
        "get_operator_model" => tool_get_operator_model().await,
        "get_causal_trace_report" => tool_get_causal_trace_report(args).await,
        "get_counterfactual_report" => tool_get_counterfactual_report(args).await,
        "get_memory_provenance_report" => tool_get_memory_provenance_report(args).await,
        "get_provenance_report" => tool_get_provenance_report(args).await,
        "generate_soc2_artifact" => tool_generate_soc2_artifact(args).await,
        "get_collaboration_sessions" => tool_get_collaboration_sessions(args).await,
        "list_generated_tools" => tool_list_generated_tools().await,
        "synthesize_tool" => tool_synthesize_tool(args).await,
        "run_generated_tool" => tool_run_generated_tool(args).await,
        "promote_generated_tool" => tool_promote_generated_tool(args).await,
        "activate_generated_tool" => tool_activate_generated_tool(args).await,
        "get_git_status" => tool_get_git_status(args).await,
        _ => anyhow::bail!("unknown tool: {name}"),
    }
}

// ---------------------------------------------------------------------------
// Tool implementations
// ---------------------------------------------------------------------------

async fn tool_execute_command(args: &Value) -> Result<Value> {
    let session_id: uuid::Uuid = args
        .get("session_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: session_id"))?
        .parse()
        .context("invalid session_id UUID")?;

    let command = args
        .get("command")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: command"))?
        .to_string();

    let rationale = args
        .get("rationale")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: rationale"))?
        .to_string();

    let msg = ClientMessage::ExecuteManagedCommand {
        id: session_id,
        request: ManagedCommandRequest {
            command: command.clone(),
            rationale,
            allow_network: false,
            sandbox_enabled: true,
            security_level: SecurityLevel::Moderate,
            cwd: None,
            language_hint: None,
            source: ManagedCommandSource::Agent,
        },
    };

    // For managed commands we may receive multiple messages (queued, started,
    // finished). We need to consume the stream until we get a terminal state.
    let mut framed = connect_daemon().await?;
    framed.send(msg).await.context("failed to send to daemon")?;

    let mut events: Vec<Value> = Vec::new();

    while let Some(resp) = framed.next().await {
        let resp = resp.context("error reading from daemon")?;
        match resp {
            DaemonMessage::ManagedCommandQueued {
                execution_id,
                position,
                snapshot,
                ..
            } => {
                events.push(serde_json::json!({
                    "event": "queued",
                    "execution_id": execution_id,
                    "position": position,
                    "snapshot": snapshot,
                }));
            }
            DaemonMessage::ManagedCommandStarted {
                execution_id,
                command: cmd,
                ..
            } => {
                events.push(serde_json::json!({
                    "event": "started",
                    "execution_id": execution_id,
                    "command": cmd,
                }));
            }
            DaemonMessage::ManagedCommandFinished {
                execution_id,
                command: cmd,
                exit_code,
                duration_ms,
                snapshot,
                ..
            } => {
                events.push(serde_json::json!({
                    "event": "finished",
                    "execution_id": execution_id,
                    "command": cmd,
                    "exit_code": exit_code,
                    "duration_ms": duration_ms,
                    "snapshot": snapshot,
                }));
                // Terminal state reached.
                break;
            }
            DaemonMessage::ManagedCommandRejected {
                execution_id,
                message,
                ..
            } => {
                events.push(serde_json::json!({
                    "event": "rejected",
                    "execution_id": execution_id,
                    "message": message,
                }));
                break;
            }
            DaemonMessage::ApprovalRequired { approval, .. } => {
                events.push(serde_json::json!({
                    "event": "approval_required",
                    "approval_id": approval.approval_id,
                    "risk_level": approval.risk_level,
                    "blast_radius": approval.blast_radius,
                    "reasons": approval.reasons,
                }));
                // Approval blocks execution; report back to caller.
                break;
            }
            DaemonMessage::Error { message } => {
                anyhow::bail!("daemon error: {message}");
            }
            DaemonMessage::GatewayBootstrap { .. }
            | DaemonMessage::GatewaySendRequest { .. }
            | DaemonMessage::GatewayReloadCommand { .. }
            | DaemonMessage::GatewayShutdownCommand { .. } => {
                // MCP shares the socket but does not participate in gateway runtime control.
            }
            _ => {
                // Ignore other messages (output bytes, etc.)
            }
        }
    }

    Ok(serde_json::json!({
        "command": command,
        "session_id": session_id.to_string(),
        "events": events,
    }))
}

async fn tool_search_history(args: &Value) -> Result<Value> {
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: query"))?
        .to_string();

    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize);

    let resp = daemon_roundtrip(ClientMessage::SearchHistory { query, limit }).await?;

    match resp {
        DaemonMessage::HistorySearchResult {
            query,
            summary,
            hits,
        } => Ok(serde_json::json!({
            "query": query,
            "summary": summary,
            "hits": hits,
        })),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

async fn tool_find_symbol(args: &Value) -> Result<Value> {
    let workspace_root = args
        .get("workspace_root")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: workspace_root"))?
        .to_string();

    let symbol = args
        .get("symbol")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: symbol"))?
        .to_string();

    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize);

    let resp = daemon_roundtrip(ClientMessage::FindSymbol {
        workspace_root,
        symbol,
        limit,
    })
    .await?;

    match resp {
        DaemonMessage::SymbolSearchResult { symbol, matches } => Ok(serde_json::json!({
            "symbol": symbol,
            "matches": matches,
        })),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

async fn tool_list_snapshots(args: &Value) -> Result<Value> {
    let workspace_id = args
        .get("workspace_id")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let resp = daemon_roundtrip(ClientMessage::ListSnapshots { workspace_id }).await?;

    match resp {
        DaemonMessage::SnapshotList { snapshots } => Ok(serde_json::json!({
            "snapshots": snapshots,
        })),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

async fn tool_restore_snapshot(args: &Value) -> Result<Value> {
    let snapshot_id = args
        .get("snapshot_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: snapshot_id"))?
        .to_string();

    let resp = daemon_roundtrip(ClientMessage::RestoreSnapshot { snapshot_id }).await?;

    match resp {
        DaemonMessage::SnapshotRestored {
            snapshot_id,
            ok,
            message,
        } => Ok(serde_json::json!({
            "snapshot_id": snapshot_id,
            "ok": ok,
            "message": message,
        })),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

async fn tool_scrub_sensitive(args: &Value) -> Result<Value> {
    let text = args
        .get("text")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: text"))?
        .to_string();

    let resp = daemon_roundtrip(ClientMessage::ScrubSensitive { text }).await?;

    match resp {
        DaemonMessage::ScrubResult { text } => Ok(serde_json::json!({
            "scrubbed_text": text,
        })),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

async fn tool_list_sessions(_args: &Value) -> Result<Value> {
    let resp = daemon_roundtrip(ClientMessage::ListSessions).await?;

    let sessions = match resp {
        DaemonMessage::SessionList { sessions } => sessions,
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    };

    // Try to read workspace topology for a richer view that includes browser panels.
    let topology_path = amux_protocol::ensure_amux_data_dir()
        .ok()
        .map(|dir| dir.join("workspace-topology.json"));
    let topology: Option<amux_protocol::WorkspaceTopology> = topology_path
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|data| serde_json::from_str(&data).ok());

    if let Some(topology) = topology {
        let formatted = amux_protocol::format_topology(&topology, &sessions);
        if !formatted.is_empty() {
            return Ok(serde_json::json!({ "topology": formatted }));
        }
    }

    // Fallback: raw sessions.
    Ok(serde_json::json!({ "sessions": sessions }))
}

async fn tool_verify_integrity(_args: &Value) -> Result<Value> {
    let resp = daemon_roundtrip(ClientMessage::VerifyTelemetryIntegrity).await?;

    match resp {
        DaemonMessage::TelemetryIntegrityResult { results } => Ok(serde_json::json!({
            "results": results,
        })),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

async fn tool_get_terminal_content(args: &Value) -> Result<Value> {
    let session_id: uuid::Uuid = args
        .get("session_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: session_id"))?
        .parse()
        .context("invalid session_id UUID")?;

    let max_lines = args
        .get("max_lines")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize);

    let resp = daemon_roundtrip(ClientMessage::GetScrollback {
        id: session_id,
        max_lines: Some(max_lines.unwrap_or(100)),
    })
    .await?;

    match resp {
        DaemonMessage::Scrollback { id, data } => {
            let text = String::from_utf8_lossy(&data);
            // Strip ANSI escape sequences for cleaner output
            let clean = strip_ansi_basic(&text);
            Ok(serde_json::json!({
                "session_id": id.to_string(),
                "content": clean,
                "lines": clean.lines().count(),
            }))
        }
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

async fn tool_enqueue_task(args: &Value) -> Result<Value> {
    let description = args
        .get("description")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: description"))?
        .trim()
        .to_string();
    if description.is_empty() {
        anyhow::bail!("description must not be empty");
    }

    let command = args
        .get("command")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned);
    let title = args
        .get("title")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| default_task_title(&description, command.as_deref()));
    let priority = args
        .get("priority")
        .and_then(|v| v.as_str())
        .unwrap_or("normal")
        .to_string();
    let session_id = args
        .get("session_id")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
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

    let resp = daemon_roundtrip(ClientMessage::AgentAddTask {
        title,
        description,
        priority,
        command,
        session_id,
        scheduled_at,
        dependencies,
    })
    .await?;

    match resp {
        DaemonMessage::AgentTaskEnqueued { task_json } => Ok(serde_json::json!({
            "task": serde_json::from_str::<Value>(&task_json).unwrap_or(Value::String(task_json)),
        })),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

async fn tool_list_tasks() -> Result<Value> {
    let resp = daemon_roundtrip(ClientMessage::AgentListTasks).await?;

    match resp {
        DaemonMessage::AgentTaskList { tasks_json } => Ok(serde_json::json!({
            "tasks": serde_json::from_str::<Value>(&tasks_json).unwrap_or(Value::Array(Vec::new())),
        })),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

async fn tool_cancel_task(args: &Value) -> Result<Value> {
    let task_id = args
        .get("task_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: task_id"))?
        .to_string();

    let resp = daemon_roundtrip(ClientMessage::AgentCancelTask {
        task_id: task_id.clone(),
    })
    .await?;

    match resp {
        DaemonMessage::AgentTaskCancelled { task_id, cancelled } => Ok(serde_json::json!({
            "task_id": task_id,
            "cancelled": cancelled,
        })),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

async fn tool_list_todos() -> Result<Value> {
    let resp = daemon_roundtrip(ClientMessage::AgentListTodos).await?;

    match resp {
        DaemonMessage::AgentTodoList { todos_json } => Ok(serde_json::json!({
            "todos": serde_json::from_str::<Value>(&todos_json).unwrap_or(Value::Object(Default::default())),
        })),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

async fn tool_get_todos(args: &Value) -> Result<Value> {
    let thread_id = args
        .get("thread_id")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: thread_id"))?
        .to_string();

    let resp = daemon_roundtrip(ClientMessage::AgentGetTodos {
        thread_id: thread_id.clone(),
    })
    .await?;

    match resp {
        DaemonMessage::AgentTodoDetail {
            thread_id,
            todos_json,
        } => Ok(serde_json::json!({
            "thread_id": thread_id,
            "items": serde_json::from_str::<Value>(&todos_json).unwrap_or(Value::Array(Vec::new())),
        })),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

async fn tool_list_skills(args: &Value) -> Result<Value> {
    let query = args
        .get("query")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|v| v.to_lowercase());
    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .unwrap_or(20)
        .clamp(1, 100) as usize;

    let skills_root = tamux_skills_dir();
    let mut files = Vec::new();
    collect_skill_documents(&skills_root, &mut files)?;
    files.sort();

    let skills = files
        .into_iter()
        .filter_map(|path| {
            let relative = path
                .strip_prefix(&skills_root)
                .unwrap_or(path.as_path())
                .to_string_lossy()
                .replace('\\', "/");
            let stem = path
                .file_stem()
                .and_then(|value| value.to_str())
                .unwrap_or("skill")
                .to_string();
            Some(serde_json::json!({
                "name": stem,
                "path": relative,
            }))
        })
        .filter(|entry| match query.as_ref() {
            Some(needle) => {
                entry
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(needle)
                    || entry
                        .get("path")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_lowercase()
                        .contains(needle)
            }
            None => true,
        })
        .take(limit)
        .collect::<Vec<_>>();

    Ok(serde_json::json!({
        "skills_root": skills_root,
        "skills": skills,
    }))
}

async fn tool_read_skill(args: &Value) -> Result<Value> {
    let skill = args
        .get("skill")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: skill"))?;
    let max_lines = args
        .get("max_lines")
        .and_then(|v| v.as_u64())
        .unwrap_or(200)
        .clamp(20, 1000) as usize;

    let skills_root = tamux_skills_dir();
    let skill_path = resolve_skill_path(&skills_root, skill)?;
    let content = std::fs::read_to_string(&skill_path)
        .with_context(|| format!("failed to read {}", skill_path.display()))?;
    let total_lines = content.lines().count();
    let excerpt = content
        .lines()
        .take(max_lines)
        .collect::<Vec<_>>()
        .join("\n");

    Ok(serde_json::json!({
        "skills_root": skills_root,
        "path": skill_path.strip_prefix(&skills_root).unwrap_or(skill_path.as_path()).display().to_string(),
        "content": excerpt,
        "truncated": total_lines > max_lines,
        "total_lines": total_lines,
    }))
}

async fn tool_start_goal_run(args: &Value) -> Result<Value> {
    let goal = args
        .get("goal")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: goal"))?
        .to_string();

    let title = args
        .get("title")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned);
    let thread_id = args
        .get("thread_id")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned);
    let session_id = args
        .get("session_id")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned);
    let priority = args
        .get("priority")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned);
    let client_request_id = args
        .get("client_request_id")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned);

    let autonomy_level = args
        .get("autonomy_level")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned);

    let resp = daemon_roundtrip(ClientMessage::AgentStartGoalRun {
        goal,
        title,
        thread_id,
        session_id,
        priority,
        client_request_id,
        autonomy_level,
    })
    .await?;

    match resp {
        DaemonMessage::AgentGoalRunStarted { goal_run_json } => Ok(serde_json::json!({
            "goal_run": serde_json::from_str::<Value>(&goal_run_json).unwrap_or(Value::Null),
        })),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

async fn tool_list_goal_runs() -> Result<Value> {
    let resp = daemon_roundtrip(ClientMessage::AgentListGoalRuns).await?;

    match resp {
        DaemonMessage::AgentGoalRunList { goal_runs_json } => Ok(serde_json::json!({
            "goal_runs": serde_json::from_str::<Value>(&goal_runs_json).unwrap_or(Value::Array(Vec::new())),
        })),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

async fn tool_get_goal_run(args: &Value) -> Result<Value> {
    let goal_run_id = args
        .get("goal_run_id")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: goal_run_id"))?
        .to_string();

    let resp = daemon_roundtrip(ClientMessage::AgentGetGoalRun {
        goal_run_id: goal_run_id.clone(),
    })
    .await?;

    match resp {
        DaemonMessage::AgentGoalRunDetail { goal_run_json } => Ok(serde_json::json!({
            "goal_run_id": goal_run_id,
            "goal_run": serde_json::from_str::<Value>(&goal_run_json).unwrap_or(Value::Null),
        })),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

async fn tool_control_goal_run(args: &Value) -> Result<Value> {
    let goal_run_id = args
        .get("goal_run_id")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: goal_run_id"))?
        .to_string();
    let action = args
        .get("action")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: action"))?
        .to_string();
    let step_index = args
        .get("step_index")
        .and_then(|v| v.as_u64())
        .map(|v| v as usize);

    let resp = daemon_roundtrip(ClientMessage::AgentControlGoalRun {
        goal_run_id: goal_run_id.clone(),
        action: action.clone(),
        step_index,
    })
    .await?;

    match resp {
        DaemonMessage::AgentGoalRunControlled { goal_run_id, ok } => Ok(serde_json::json!({
            "goal_run_id": goal_run_id,
            "action": action,
            "step_index": step_index,
            "ok": ok,
        })),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

async fn tool_get_operator_model() -> Result<Value> {
    let resp = daemon_roundtrip(ClientMessage::AgentGetOperatorModel).await?;

    match resp {
        DaemonMessage::AgentOperatorModel { model_json } => Ok(serde_json::json!({
            "operator_model": serde_json::from_str::<Value>(&model_json).unwrap_or(Value::Null),
        })),
        DaemonMessage::AgentError { message } | DaemonMessage::Error { message } => {
            anyhow::bail!("daemon error: {message}")
        }
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

async fn tool_get_causal_trace_report(args: &Value) -> Result<Value> {
    let option_type = args
        .get("option_type")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: option_type"))?
        .to_string();
    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .map(|value| value.clamp(1, 200) as u32);

    let resp = daemon_roundtrip(ClientMessage::AgentGetCausalTraceReport {
        option_type: option_type.clone(),
        limit,
    })
    .await?;

    match resp {
        DaemonMessage::AgentCausalTraceReport { report_json } => Ok(serde_json::json!({
            "option_type": option_type,
            "report": serde_json::from_str::<Value>(&report_json).unwrap_or(Value::Null),
        })),
        DaemonMessage::AgentError { message } | DaemonMessage::Error { message } => {
            anyhow::bail!("daemon error: {message}")
        }
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

async fn tool_get_counterfactual_report(args: &Value) -> Result<Value> {
    let option_type = args
        .get("option_type")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: option_type"))?
        .to_string();
    let command_family = args
        .get("command_family")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: command_family"))?
        .to_string();
    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .map(|value| value.clamp(1, 200) as u32);

    let resp = daemon_roundtrip(ClientMessage::AgentGetCounterfactualReport {
        option_type: option_type.clone(),
        command_family: command_family.clone(),
        limit,
    })
    .await?;

    match resp {
        DaemonMessage::AgentCounterfactualReport { report_json } => Ok(serde_json::json!({
            "option_type": option_type,
            "command_family": command_family,
            "report": serde_json::from_str::<Value>(&report_json).unwrap_or(Value::Null),
        })),
        DaemonMessage::AgentError { message } | DaemonMessage::Error { message } => {
            anyhow::bail!("daemon error: {message}")
        }
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

async fn tool_get_memory_provenance_report(args: &Value) -> Result<Value> {
    let target = args
        .get("target")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned);
    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .map(|value| value.clamp(1, 200) as u32);

    let resp = daemon_roundtrip(ClientMessage::AgentGetMemoryProvenanceReport {
        target: target.clone(),
        limit,
    })
    .await?;

    match resp {
        DaemonMessage::AgentMemoryProvenanceReport { report_json } => Ok(serde_json::json!({
            "target": target,
            "report": serde_json::from_str::<Value>(&report_json).unwrap_or(Value::Null),
        })),
        DaemonMessage::AgentError { message } | DaemonMessage::Error { message } => {
            anyhow::bail!("daemon error: {message}")
        }
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

async fn tool_get_provenance_report(args: &Value) -> Result<Value> {
    let limit = args
        .get("limit")
        .and_then(|v| v.as_u64())
        .map(|value| value.clamp(1, 500) as u32);
    let resp = daemon_roundtrip(ClientMessage::AgentGetProvenanceReport { limit }).await?;
    match resp {
        DaemonMessage::AgentProvenanceReport { report_json } => Ok(serde_json::json!({
            "report": serde_json::from_str::<Value>(&report_json).unwrap_or(Value::Null),
        })),
        DaemonMessage::AgentError { message } | DaemonMessage::Error { message } => {
            anyhow::bail!("daemon error: {message}")
        }
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

async fn tool_generate_soc2_artifact(args: &Value) -> Result<Value> {
    let period_days = args
        .get("period_days")
        .and_then(|v| v.as_u64())
        .map(|value| value.clamp(1, 365) as u32);
    let resp = daemon_roundtrip(ClientMessage::AgentGenerateSoc2Artifact { period_days }).await?;
    match resp {
        DaemonMessage::AgentSoc2Artifact { artifact_path } => Ok(serde_json::json!({
            "artifact_path": artifact_path,
        })),
        DaemonMessage::AgentError { message } | DaemonMessage::Error { message } => {
            anyhow::bail!("daemon error: {message}")
        }
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

async fn tool_get_collaboration_sessions(args: &Value) -> Result<Value> {
    let parent_task_id = args
        .get("parent_task_id")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(ToOwned::to_owned);
    let resp = daemon_roundtrip(ClientMessage::AgentGetCollaborationSessions {
        parent_task_id: parent_task_id.clone(),
    })
    .await?;
    match resp {
        DaemonMessage::AgentCollaborationSessions { sessions_json } => Ok(serde_json::json!({
            "parent_task_id": parent_task_id,
            "sessions": serde_json::from_str::<Value>(&sessions_json).unwrap_or(Value::Null),
        })),
        DaemonMessage::AgentError { message } | DaemonMessage::Error { message } => {
            anyhow::bail!("daemon error: {message}")
        }
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

async fn tool_list_generated_tools() -> Result<Value> {
    let resp = daemon_roundtrip(ClientMessage::AgentListGeneratedTools).await?;
    match resp {
        DaemonMessage::AgentGeneratedTools { tools_json } => Ok(serde_json::json!({
            "tools": serde_json::from_str::<Value>(&tools_json).unwrap_or(Value::Null),
        })),
        DaemonMessage::AgentError { message } | DaemonMessage::Error { message } => {
            anyhow::bail!("daemon error: {message}")
        }
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

async fn tool_synthesize_tool(args: &Value) -> Result<Value> {
    let resp = daemon_roundtrip(ClientMessage::AgentSynthesizeTool {
        request_json: serde_json::to_string(args)?,
    })
    .await?;
    match resp {
        DaemonMessage::AgentGeneratedToolResult {
            tool_name,
            result_json,
        } => Ok(serde_json::json!({
            "tool_name": tool_name,
            "result": serde_json::from_str::<Value>(&result_json).unwrap_or(Value::Null),
        })),
        DaemonMessage::AgentError { message } | DaemonMessage::Error { message } => {
            anyhow::bail!("daemon error: {message}")
        }
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

async fn tool_run_generated_tool(args: &Value) -> Result<Value> {
    let tool_name = args
        .get("tool_name")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: tool_name"))?
        .to_string();
    let tool_args = args
        .get("args")
        .cloned()
        .unwrap_or_else(|| serde_json::json!({}));
    let resp = daemon_roundtrip(ClientMessage::AgentRunGeneratedTool {
        tool_name: tool_name.clone(),
        args_json: serde_json::to_string(&tool_args)?,
    })
    .await?;
    match resp {
        DaemonMessage::AgentGeneratedToolResult {
            tool_name,
            result_json,
        } => Ok(serde_json::json!({
            "tool_name": tool_name,
            "result": result_json,
        })),
        DaemonMessage::AgentError { message } | DaemonMessage::Error { message } => {
            anyhow::bail!("daemon error: {message}")
        }
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

async fn tool_promote_generated_tool(args: &Value) -> Result<Value> {
    let tool_name = args
        .get("tool_name")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: tool_name"))?
        .to_string();
    let resp = daemon_roundtrip(ClientMessage::AgentPromoteGeneratedTool {
        tool_name: tool_name.clone(),
    })
    .await?;
    match resp {
        DaemonMessage::AgentGeneratedToolResult {
            tool_name,
            result_json,
        } => Ok(serde_json::json!({
            "tool_name": tool_name,
            "result": serde_json::from_str::<Value>(&result_json).unwrap_or(Value::Null),
        })),
        DaemonMessage::AgentError { message } | DaemonMessage::Error { message } => {
            anyhow::bail!("daemon error: {message}")
        }
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

async fn tool_activate_generated_tool(args: &Value) -> Result<Value> {
    let tool_name = args
        .get("tool_name")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: tool_name"))?
        .to_string();
    let resp = daemon_roundtrip(ClientMessage::AgentActivateGeneratedTool {
        tool_name: tool_name.clone(),
    })
    .await?;
    match resp {
        DaemonMessage::AgentGeneratedToolResult {
            tool_name,
            result_json,
        } => Ok(serde_json::json!({
            "tool_name": tool_name,
            "result": serde_json::from_str::<Value>(&result_json).unwrap_or(Value::Null),
        })),
        DaemonMessage::AgentError { message } | DaemonMessage::Error { message } => {
            anyhow::bail!("daemon error: {message}")
        }
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

fn tamux_root_dir() -> PathBuf {
    if cfg!(windows) {
        std::env::var("LOCALAPPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                std::env::var("USERPROFILE")
                    .map(PathBuf::from)
                    .unwrap_or_default()
                    .join("AppData")
                    .join("Local")
            })
            .join("tamux")
    } else {
        std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_default()
            .join(".tamux")
    }
}

fn tamux_skills_dir() -> PathBuf {
    tamux_root_dir().join("skills")
}

fn collect_skill_documents(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
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

        let file_name = path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("");
        let include = file_name.eq_ignore_ascii_case("SKILL.md")
            || (path
                .extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("md"))
                && path
                    .components()
                    .any(|component| component.as_os_str() == "generated"));
        if include {
            out.push(path);
        }
    }

    Ok(())
}

fn resolve_skill_path(skills_root: &Path, skill: &str) -> Result<PathBuf> {
    if skill.trim().is_empty() {
        anyhow::bail!("skill must not be empty");
    }

    let root_canonical = std::fs::canonicalize(skills_root).unwrap_or(skills_root.to_path_buf());
    let candidate = Path::new(skill);
    if candidate.components().count() > 1 || candidate.is_absolute() {
        let full = if candidate.is_absolute() {
            candidate.to_path_buf()
        } else {
            skills_root.join(candidate)
        };
        let canonical = std::fs::canonicalize(&full)
            .with_context(|| format!("skill '{}' was not found", skill))?;
        if !canonical.starts_with(&root_canonical) {
            anyhow::bail!("skill path must stay inside {}", skills_root.display());
        }
        return Ok(canonical);
    }

    let mut files = Vec::new();
    collect_skill_documents(skills_root, &mut files)?;
    files.sort();
    let normalized = skill.to_lowercase();

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

fn parse_scheduled_at(args: &Value) -> Result<Option<u64>> {
    if let Some(timestamp) = args.get("scheduled_at").and_then(|value| value.as_u64()) {
        return Ok(Some(timestamp));
    }

    if let Some(value) = args.get("schedule_at").and_then(|value| value.as_str()) {
        let timestamp = humantime::parse_rfc3339_weak(value)
            .map_err(|error| anyhow::anyhow!("invalid schedule_at value: {error}"))?;
        let millis = timestamp
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| anyhow::anyhow!("invalid schedule_at value: {error}"))?
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

async fn tool_type_in_terminal(args: &Value) -> Result<Value> {
    let session_id: uuid::Uuid = args
        .get("session_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: session_id"))?
        .parse()
        .context("invalid session_id UUID")?;

    let input = args
        .get("input")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: input"))?;

    // Convert escape sequences
    let data = input
        .replace("\\n", "\n")
        .replace("\\r", "\r")
        .replace("\\t", "\t")
        .into_bytes();

    let mut framed = connect_daemon().await?;
    framed
        .send(ClientMessage::Input {
            id: session_id,
            data: data.clone(),
        })
        .await
        .context("failed to send input to daemon")?;

    // Input is fire-and-forget on success, but invalid/exited sessions yield
    // an immediate daemon Error. Wait briefly to propagate that outcome.
    match timeout(Duration::from_millis(250), framed.next()).await {
        Ok(Some(Ok(DaemonMessage::Error { message }))) => {
            anyhow::bail!("daemon error: {message}");
        }
        Ok(Some(Err(e))) => {
            return Err(e.into());
        }
        Ok(None) => {
            anyhow::bail!("daemon connection closed while sending input");
        }
        Ok(Some(Ok(_))) | Err(_) => {
            // No immediate error => treat as accepted.
        }
    }

    Ok(serde_json::json!({
        "session_id": session_id.to_string(),
        "bytes_sent": data.len(),
        "status": "ok",
    }))
}

async fn tool_get_git_status(args: &Value) -> Result<Value> {
    let path = args
        .get("path")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: path"))?
        .to_string();

    let resp = daemon_roundtrip(ClientMessage::GetGitStatus { path }).await?;

    match resp {
        DaemonMessage::GitStatus {
            path: repo_path,
            info,
        } => Ok(serde_json::json!({
            "path": repo_path,
            "branch": info.branch,
            "is_dirty": info.is_dirty,
            "ahead": info.ahead,
            "behind": info.behind,
            "untracked": info.untracked,
            "modified": info.modified,
            "staged": info.staged,
        })),
        DaemonMessage::Error { message } => anyhow::bail!("daemon error: {message}"),
        other => anyhow::bail!("unexpected daemon response: {other:?}"),
    }
}

/// Basic ANSI escape sequence stripping for terminal scrollback output.
fn strip_ansi_basic(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next();
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if ('\x40'..='\x7e').contains(&next) {
                        break;
                    }
                }
            } else if chars.peek() == Some(&']') {
                chars.next();
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if next == '\x07' {
                        break;
                    }
                    if next == '\x1b' && chars.peek() == Some(&'\\') {
                        chars.next();
                        break;
                    }
                }
            } else {
                chars.next();
            }
        } else {
            result.push(c);
        }
    }

    result
}

// ---------------------------------------------------------------------------
// MCP protocol method handlers
// ---------------------------------------------------------------------------

fn handle_initialize(id: Option<Value>) -> JsonRpcResponse {
    JsonRpcResponse::success(
        id,
        serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "tamux-mcp",
                "version": env!("CARGO_PKG_VERSION")
            }
        }),
    )
}

fn handle_tools_list(id: Option<Value>) -> JsonRpcResponse {
    JsonRpcResponse::success(
        id,
        serde_json::json!({
            "tools": tool_definitions()
        }),
    )
}

async fn handle_tools_call(id: Option<Value>, params: Option<Value>) -> JsonRpcResponse {
    let params = match params {
        Some(p) => p,
        None => {
            return JsonRpcResponse::error(id, INVALID_PARAMS, "missing params for tools/call");
        }
    };

    let tool_name = match params.get("name").and_then(|v| v.as_str()) {
        Some(n) => n.to_string(),
        None => {
            return JsonRpcResponse::error(id, INVALID_PARAMS, "missing tool name in params");
        }
    };

    let arguments = params
        .get("arguments")
        .cloned()
        .unwrap_or(serde_json::json!({}));

    let result = handle_tool_call(&tool_name, &arguments).await;
    JsonRpcResponse::success(id, result)
}

// ---------------------------------------------------------------------------
// Stdio transport — content-length framing
// ---------------------------------------------------------------------------

/// Read a single JSON-RPC message from stdin using content-length framing.
///
/// The MCP specification uses HTTP-style headers:
/// ```text
/// Content-Length: <n>\r\n
/// \r\n
/// <n bytes of JSON>
/// ```
/// Read a JSON-RPC message from stdin.
///
/// Auto-detects framing format:
/// - If a line starts with `{`, it's newline-delimited JSON (Python `mcp` SDK style)
/// - If a line starts with `Content-Length:`, it's LSP-style content-length framing
async fn read_message(reader: &mut BufReader<tokio::io::Stdin>) -> Result<Option<String>> {
    loop {
        let mut line = String::new();
        let n = reader
            .read_line(&mut line)
            .await
            .context("failed to read from stdin")?;

        if n == 0 {
            return Ok(None); // EOF
        }

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue; // Skip blank lines
        }

        // Newline-delimited JSON: line starts with '{' — it's a complete message
        if trimmed.starts_with('{') {
            return Ok(Some(trimmed.to_string()));
        }

        // Content-Length framing: read headers, then body
        if let Some(value) = trimmed.strip_prefix("Content-Length:") {
            let length: usize = value
                .trim()
                .parse()
                .context("invalid Content-Length value")?;

            // Read remaining headers until blank line
            loop {
                let mut header_line = String::new();
                let hn = reader
                    .read_line(&mut header_line)
                    .await
                    .context("failed to read header")?;
                if hn == 0 || header_line.trim().is_empty() {
                    break;
                }
            }

            let mut body = vec![0u8; length];
            tokio::io::AsyncReadExt::read_exact(reader, &mut body)
                .await
                .context("failed to read message body")?;

            let text = String::from_utf8(body).context("message body is not valid UTF-8")?;
            return Ok(Some(text));
        }

        // Skip unknown header lines
    }
}

/// Write a JSON-RPC response to stdout.
///
/// Supports two framing modes based on an environment variable:
/// - `TAMUX_MCP_FRAMING=content-length` — Content-Length header framing (LSP-style)
/// - Default — newline-delimited JSON (one JSON object per line, as used by the
///   Python `mcp` SDK's stdio transport)
fn write_message(msg: &JsonRpcResponse) -> Result<()> {
    let body = serde_json::to_string(msg).context("failed to serialize response")?;

    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    if std::env::var("TAMUX_MCP_FRAMING").as_deref() == Ok("content-length") {
        let header = format!("Content-Length: {}\r\n\r\n", body.len());
        out.write_all(header.as_bytes())?;
        out.write_all(body.as_bytes())?;
    } else {
        // Newline-delimited JSON — compatible with Python mcp SDK
        out.write_all(body.as_bytes())?;
        out.write_all(b"\n")?;
    }

    out.flush()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Main loop
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<()> {
    // Log to stderr so stdout stays clean for JSON-RPC.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("warn")),
        )
        .with_writer(std::io::stderr)
        .init();

    info!("tamux-mcp server starting");

    let mut reader = BufReader::new(tokio::io::stdin());

    loop {
        let message = match read_message(&mut reader).await {
            Ok(Some(msg)) => msg,
            Ok(None) => {
                debug!("stdin closed, shutting down");
                break;
            }
            Err(err) => {
                error!("failed to read message: {err:#}");
                let resp = JsonRpcResponse::error(None, PARSE_ERROR, format!("{err:#}"));
                if let Err(e) = write_message(&resp) {
                    error!("failed to write error response: {e:#}");
                }
                continue;
            }
        };

        debug!("received: {message}");

        let request: JsonRpcRequest = match serde_json::from_str(&message) {
            Ok(req) => req,
            Err(err) => {
                warn!("invalid JSON-RPC message: {err}");
                let resp =
                    JsonRpcResponse::error(None, PARSE_ERROR, format!("invalid JSON: {err}"));
                write_message(&resp)?;
                continue;
            }
        };

        if request.jsonrpc != "2.0" {
            let resp = JsonRpcResponse::error(
                request.id.clone(),
                INVALID_REQUEST,
                "only JSON-RPC 2.0 is supported",
            );
            write_message(&resp)?;
            continue;
        }

        let response = match request.method.as_str() {
            "initialize" => Some(handle_initialize(request.id)),
            "initialized" | "notifications/initialized" => {
                // Notification — no response needed.
                debug!("received initialized notification");
                None
            }
            "tools/list" => Some(handle_tools_list(request.id)),
            "tools/call" => Some(handle_tools_call(request.id, request.params).await),
            "notifications/cancelled" => {
                debug!("received cancellation notification");
                None
            }
            "ping" => Some(JsonRpcResponse::success(request.id, serde_json::json!({}))),
            method => {
                warn!("unknown method: {method}");
                // Notifications (no id) should not receive error responses.
                if request.id.is_some() {
                    Some(JsonRpcResponse::error(
                        request.id,
                        METHOD_NOT_FOUND,
                        format!("unknown method: {method}"),
                    ))
                } else {
                    None
                }
            }
        };

        if let Some(resp) = response {
            if let Err(e) = write_message(&resp) {
                error!("failed to write response: {e:#}");
            }
        }
    }

    info!("tamux-mcp server stopped");
    Ok(())
}
