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

#![recursion_limit = "256"]

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
use tokio::time::{Duration, timeout};
use tokio_util::codec::Framed;
use tracing::{debug, error, info, warn};

#[path = "main/agent_tools.rs"]
mod agent_tools;
#[path = "main/daemon.rs"]
mod daemon;
#[path = "main/daemon_tools.rs"]
mod daemon_tools;
#[path = "main/rpc.rs"]
mod rpc;
#[path = "main/skills.rs"]
mod skills;
#[path = "main/tool_definitions.rs"]
mod tool_definitions;
#[path = "main/transport.rs"]
mod transport;
#[path = "main/utils.rs"]
mod utils;

use agent_tools::{
    tool_activate_generated_tool, tool_control_goal_run, tool_generate_soc2_artifact,
    tool_get_causal_trace_report, tool_get_collaboration_sessions, tool_get_counterfactual_report,
    tool_get_goal_run, tool_get_memory_provenance_report, tool_get_operator_model,
    tool_get_provenance_report, tool_inspect_skill_variant, tool_list_generated_tools,
    tool_list_goal_runs, tool_list_skill_variants, tool_promote_generated_tool,
    tool_query_audits, tool_reset_operator_model, tool_run_generated_tool,
    tool_synthesize_tool,
};
use daemon::{connect_daemon, daemon_roundtrip};
use daemon_tools::{
    tool_execute_command, tool_find_symbol, tool_get_git_status, tool_get_terminal_content,
    tool_list_sessions, tool_list_snapshots, tool_restore_snapshot, tool_scrub_sensitive,
    tool_search_history, tool_semantic_query, tool_type_in_terminal, tool_verify_integrity,
};
use rpc::{JsonRpcRequest, JsonRpcResponse, handle_initialize, handle_tools_list};
use skills::{collect_skill_documents, resolve_skill_path, tamux_root_dir, tamux_skills_dir};
use tool_definitions::tool_definitions;
use transport::{read_message, write_message};
use utils::{default_task_title, parse_scheduled_at, strip_ansi_basic};

// JSON-RPC error codes
const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
#[allow(dead_code)]
const INTERNAL_ERROR: i64 = -32603;

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
        "list_skill_variants" => tool_list_skill_variants(args).await,
        "inspect_skill_variant" => tool_inspect_skill_variant(args).await,
        "start_goal_run" => tool_start_goal_run(args).await,
        "list_goal_runs" => tool_list_goal_runs().await,
        "get_goal_run" => tool_get_goal_run(args).await,
        "control_goal_run" => tool_control_goal_run(args).await,
        "get_operator_model" => tool_get_operator_model().await,
        "reset_operator_model" => tool_reset_operator_model().await,
        "get_causal_trace_report" => tool_get_causal_trace_report(args).await,
        "get_counterfactual_report" => tool_get_counterfactual_report(args).await,
        "get_memory_provenance_report" => tool_get_memory_provenance_report(args).await,
        "get_provenance_report" => tool_get_provenance_report(args).await,
        "query_audits" => tool_query_audits(args).await,
        "generate_soc2_artifact" => tool_generate_soc2_artifact(args).await,
        "get_collaboration_sessions" => tool_get_collaboration_sessions(args).await,
        "list_generated_tools" => tool_list_generated_tools().await,
        "synthesize_tool" => tool_synthesize_tool(args).await,
        "run_generated_tool" => tool_run_generated_tool(args).await,
        "promote_generated_tool" => tool_promote_generated_tool(args).await,
        "activate_generated_tool" => tool_activate_generated_tool(args).await,
        "get_git_status" => tool_get_git_status(args).await,
        "semantic_query" => tool_semantic_query(args).await,
        _ => anyhow::bail!("unknown tool: {name}"),
    }
}

// ---------------------------------------------------------------------------
// Tool implementations
// ---------------------------------------------------------------------------

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
        .filter_map(|path: PathBuf| {
            let relative = path
                .strip_prefix(&skills_root)
                .unwrap_or(path.as_path())
                .to_string_lossy()
                .replace('\\', "/");
            let stem = path
                .file_stem()
                .and_then(|value: &std::ffi::OsStr| value.to_str())
                .unwrap_or("skill")
                .to_string();
            Some(serde_json::json!({
                "name": stem,
                "path": relative,
            }))
        })
        .filter(|entry: &Value| match query.as_ref() {
            Some(needle) => {
                entry
                    .get("name")
                    .and_then(|v: &Value| v.as_str())
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(needle)
                    || entry
                        .get("path")
                        .and_then(|v: &Value| v.as_str())
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
        client_surface: None,
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
            "tools/list" => Some(handle_tools_list(request.id, tool_definitions())),
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
