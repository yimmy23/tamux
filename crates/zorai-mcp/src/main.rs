//! zorai-mcp: An MCP (Model Context Protocol) server that exposes the zorai
//! daemon's capabilities as tools over JSON-RPC stdio transport.
//!
//! Register this binary as an MCP server in Claude Code or other MCP clients:
//!
//! ```json
//! {
//!   "mcpServers": {
//!     "zorai": {
//!       "command": "zorai-mcp"
//!     }
//!   }
//! }
//! ```

#![recursion_limit = "256"]

use std::path::PathBuf;

use anyhow::{Context, Result};
use base64::Engine;
use futures::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::io::BufReader;
use tokio::time::{timeout, Duration};
use tracing::{debug, error, info, warn};
use zorai_protocol::{
    ClientMessage, DaemonMessage, GoalAgentAssignment, ManagedCommandRequest, ManagedCommandSource,
    SecurityLevel,
};

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
    tool_activate_generated_tool, tool_ask_questions, tool_control_goal_run,
    tool_discover_guidelines, tool_discover_skills, tool_generate_soc2_artifact,
    tool_get_causal_trace_report, tool_get_collaboration_sessions, tool_get_counterfactual_report,
    tool_get_goal_run, tool_get_memory_provenance_report, tool_get_operator_model,
    tool_get_provenance_report, tool_inspect_skill_variant, tool_list_generated_tools,
    tool_list_goal_runs, tool_list_skill_variants, tool_promote_generated_tool, tool_query_audits,
    tool_reset_operator_model, tool_run_generated_tool, tool_synthesize_tool,
};
use daemon::{connect_daemon, daemon_roundtrip};
use daemon_tools::{
    tool_execute_command, tool_find_symbol, tool_get_git_status, tool_get_terminal_content,
    tool_list_sessions, tool_list_snapshots, tool_read_memory, tool_read_soul, tool_read_user,
    tool_restore_snapshot, tool_scrub_sensitive, tool_search_history, tool_search_memory,
    tool_search_soul, tool_search_user, tool_semantic_query, tool_type_in_terminal,
    tool_verify_integrity,
};
use rpc::{handle_initialize, handle_tools_list, JsonRpcRequest, JsonRpcResponse};
use skills::{
    collect_guideline_documents, collect_skill_documents, resolve_guideline_path,
    resolve_skill_path, zorai_guidelines_dir, zorai_skills_dir,
};
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
        zorai_protocol::tool_names::EXECUTE_COMMAND => tool_execute_command(args).await,
        zorai_protocol::tool_names::SEARCH_HISTORY => tool_search_history(args).await,
        zorai_protocol::tool_names::FIND_SYMBOL => tool_find_symbol(args).await,
        zorai_protocol::tool_names::LIST_SNAPSHOTS => tool_list_snapshots(args).await,
        zorai_protocol::tool_names::RESTORE_SNAPSHOT => tool_restore_snapshot(args).await,
        zorai_protocol::tool_names::SCRUB_SENSITIVE => tool_scrub_sensitive(args).await,
        zorai_protocol::tool_names::LIST_SESSIONS => tool_list_sessions(args).await,
        zorai_protocol::tool_names::VERIFY_INTEGRITY => tool_verify_integrity(args).await,
        zorai_protocol::tool_names::GET_TERMINAL_CONTENT => tool_get_terminal_content(args).await,
        zorai_protocol::tool_names::TYPE_IN_TERMINAL => tool_type_in_terminal(args).await,
        zorai_protocol::tool_names::READ_MEMORY => tool_read_memory(args).await,
        zorai_protocol::tool_names::READ_USER => tool_read_user(args).await,
        zorai_protocol::tool_names::READ_SOUL => tool_read_soul(args).await,
        zorai_protocol::tool_names::SEARCH_MEMORY => tool_search_memory(args).await,
        zorai_protocol::tool_names::SEARCH_USER => tool_search_user(args).await,
        zorai_protocol::tool_names::SEARCH_SOUL => tool_search_soul(args).await,
        zorai_protocol::tool_names::ENQUEUE_TASK => tool_enqueue_task(args).await,
        zorai_protocol::tool_names::LIST_TASKS => tool_list_tasks().await,
        zorai_protocol::tool_names::CANCEL_TASK => tool_cancel_task(args).await,
        zorai_protocol::tool_names::LIST_TODOS => tool_list_todos().await,
        zorai_protocol::tool_names::GET_TODOS => tool_get_todos(args).await,
        zorai_protocol::tool_names::LIST_SKILLS => tool_list_skills(args).await,
        zorai_protocol::tool_names::DISCOVER_SKILLS => tool_discover_skills(args).await,
        zorai_protocol::tool_names::LIST_GUIDELINES => tool_list_guidelines(args).await,
        zorai_protocol::tool_names::DISCOVER_GUIDELINES => tool_discover_guidelines(args).await,
        zorai_protocol::tool_names::ASK_QUESTIONS => tool_ask_questions(args).await,
        zorai_protocol::tool_names::READ_SKILL => tool_read_skill(args).await,
        zorai_protocol::tool_names::READ_GUIDELINE => tool_read_guideline(args).await,
        zorai_protocol::tool_names::LIST_SKILL_VARIANTS => tool_list_skill_variants(args).await,
        zorai_protocol::tool_names::INSPECT_SKILL_VARIANT => tool_inspect_skill_variant(args).await,
        zorai_protocol::tool_names::START_GOAL_RUN => tool_start_goal_run(args).await,
        zorai_protocol::tool_names::LIST_GOAL_RUNS => tool_list_goal_runs().await,
        zorai_protocol::tool_names::GET_GOAL_RUN => tool_get_goal_run(args).await,
        zorai_protocol::tool_names::CONTROL_GOAL_RUN => tool_control_goal_run(args).await,
        zorai_protocol::tool_names::GET_OPERATOR_MODEL => tool_get_operator_model().await,
        zorai_protocol::tool_names::RESET_OPERATOR_MODEL => tool_reset_operator_model().await,
        zorai_protocol::tool_names::GET_CAUSAL_TRACE_REPORT => {
            tool_get_causal_trace_report(args).await
        }
        zorai_protocol::tool_names::GET_COUNTERFACTUAL_REPORT => {
            tool_get_counterfactual_report(args).await
        }
        zorai_protocol::tool_names::GET_MEMORY_PROVENANCE_REPORT => {
            tool_get_memory_provenance_report(args).await
        }
        zorai_protocol::tool_names::GET_PROVENANCE_REPORT => tool_get_provenance_report(args).await,
        zorai_protocol::tool_names::QUERY_AUDITS => tool_query_audits(args).await,
        zorai_protocol::tool_names::GENERATE_SOC2_ARTIFACT => {
            tool_generate_soc2_artifact(args).await
        }
        zorai_protocol::tool_names::GET_COLLABORATION_SESSIONS => {
            tool_get_collaboration_sessions(args).await
        }
        zorai_protocol::tool_names::LIST_GENERATED_TOOLS => tool_list_generated_tools().await,
        zorai_protocol::tool_names::SYNTHESIZE_TOOL => tool_synthesize_tool(args).await,
        zorai_protocol::tool_names::RUN_GENERATED_TOOL => tool_run_generated_tool(args).await,
        zorai_protocol::tool_names::PROMOTE_GENERATED_TOOL => {
            tool_promote_generated_tool(args).await
        }
        zorai_protocol::tool_names::ACTIVATE_GENERATED_TOOL => {
            tool_activate_generated_tool(args).await
        }
        zorai_protocol::tool_names::GET_GIT_STATUS => tool_get_git_status(args).await,
        zorai_protocol::tool_names::SEMANTIC_QUERY => tool_semantic_query(args).await,
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
    let cursor = args
        .get("cursor")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let skills_root = zorai_skills_dir();
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
        .collect::<Vec<_>>();
    let start_index = decode_local_skill_cursor(cursor)?
        .as_deref()
        .and_then(|path| {
            skills.iter().position(|entry| {
                entry
                    .get("path")
                    .and_then(|v| v.as_str())
                    .is_some_and(|value| value == path)
            })
        })
        .map(|index| index + 1)
        .unwrap_or(0);
    let page = skills
        .iter()
        .skip(start_index)
        .take(limit)
        .cloned()
        .collect::<Vec<_>>();
    let next_cursor = if start_index + page.len() < skills.len() {
        page.last()
            .and_then(|entry| entry.get("path"))
            .and_then(|v| v.as_str())
            .map(encode_local_skill_cursor)
    } else {
        None
    };

    Ok(serde_json::json!({
        "skills_root": skills_root,
        "skills": page,
        "next_cursor": next_cursor,
    }))
}

fn encode_local_skill_cursor(path: &str) -> String {
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(path.as_bytes());
    format!("local-skill-list:{encoded}")
}

fn decode_local_skill_cursor(cursor: Option<&str>) -> Result<Option<String>> {
    let Some(cursor) = cursor else {
        return Ok(None);
    };
    let payload = cursor
        .strip_prefix("local-skill-list:")
        .ok_or_else(|| anyhow::anyhow!("invalid local skill cursor"))?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|error| anyhow::anyhow!("invalid local skill cursor: {error}"))?;
    let value = String::from_utf8(bytes)
        .map_err(|error| anyhow::anyhow!("invalid local skill cursor: {error}"))?;
    Ok(Some(value))
}

async fn tool_read_skill(args: &Value) -> Result<Value> {
    let skills = parse_read_skill_targets(args)?;
    let max_lines = args
        .get("max_lines")
        .and_then(|v| v.as_u64())
        .unwrap_or(200)
        .clamp(20, 1000) as usize;

    let skills_root = zorai_skills_dir();
    let mut entries = Vec::with_capacity(skills.len());
    for skill in &skills {
        entries.push(read_skill_entry(&skills_root, skill, max_lines)?);
    }

    if skills.len() == 1 {
        return Ok(entries
            .into_iter()
            .next()
            .expect("single skill entry should be present"));
    }

    Ok(serde_json::json!({
        "skills_root": skills_root,
        "skills": entries,
    }))
}

fn parse_read_skill_targets(args: &Value) -> Result<Vec<String>> {
    let mut skills = Vec::new();

    if let Some(skill) = args.get("skill") {
        let skill = skill
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("skill must be a string"))?
            .trim();
        if skill.is_empty() {
            anyhow::bail!("skill must not be empty");
        }
        skills.push(skill.to_string());
    }

    if let Some(values) = args.get("skills") {
        let values = values
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("skills must be an array of strings"))?;
        for (index, value) in values.iter().enumerate() {
            let skill = value
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("skills[{index}] must be a string"))?
                .trim();
            if skill.is_empty() {
                anyhow::bail!("skills[{index}] must not be empty");
            }
            skills.push(skill.to_string());
        }
    }

    if skills.is_empty() {
        anyhow::bail!("missing required parameter: skill or skills");
    }

    Ok(skills)
}

#[cfg(test)]
mod read_skill_target_tests {
    use super::*;

    #[test]
    fn read_skill_targets_accept_empty_skills_when_skill_is_present() {
        let targets = parse_read_skill_targets(&serde_json::json!({
            "skill": "executing-plans",
            "skills": []
        }))
        .expect("empty optional skills array should not reject singular skill");

        assert_eq!(targets, vec!["executing-plans"]);
    }

    #[test]
    fn read_skill_targets_reject_empty_skills_without_skill() {
        let err = parse_read_skill_targets(&serde_json::json!({ "skills": [] }))
            .expect_err("empty skills array without skill should still be invalid");

        assert!(
            err.to_string().contains("missing required parameter"),
            "unexpected error: {err:#}"
        );
    }
}

fn read_skill_entry(skills_root: &std::path::Path, skill: &str, max_lines: usize) -> Result<Value> {
    let skill_path = resolve_skill_path(skills_root, skill)?;
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

async fn tool_list_guidelines(args: &Value) -> Result<Value> {
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
    let cursor = args
        .get("cursor")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty());

    let guidelines_root = zorai_guidelines_dir();
    let mut files = Vec::new();
    collect_guideline_documents(&guidelines_root, &mut files)?;
    files.sort();

    let guidelines = files
        .into_iter()
        .map(|path: PathBuf| {
            let relative = path
                .strip_prefix(&guidelines_root)
                .unwrap_or(path.as_path())
                .to_string_lossy()
                .replace('\\', "/");
            let stem = path
                .file_stem()
                .and_then(|value: &std::ffi::OsStr| value.to_str())
                .unwrap_or("guideline")
                .to_string();
            serde_json::json!({
                "name": stem,
                "path": relative,
            })
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
        .collect::<Vec<_>>();
    let start_index = decode_local_guideline_cursor(cursor)?
        .as_deref()
        .and_then(|path| {
            guidelines.iter().position(|entry| {
                entry
                    .get("path")
                    .and_then(|v| v.as_str())
                    .is_some_and(|value| value == path)
            })
        })
        .map(|index| index + 1)
        .unwrap_or(0);
    let page = guidelines
        .iter()
        .skip(start_index)
        .take(limit)
        .cloned()
        .collect::<Vec<_>>();
    let next_cursor = if start_index + page.len() < guidelines.len() {
        page.last()
            .and_then(|entry| entry.get("path"))
            .and_then(|v| v.as_str())
            .map(encode_local_guideline_cursor)
    } else {
        None
    };

    Ok(serde_json::json!({
        "guidelines_root": guidelines_root,
        "guidelines": page,
        "next_cursor": next_cursor,
    }))
}

async fn tool_read_guideline(args: &Value) -> Result<Value> {
    let guideline = args
        .get("guideline")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing required parameter: guideline"))?;
    let max_lines = args
        .get("max_lines")
        .and_then(|v| v.as_u64())
        .unwrap_or(200)
        .clamp(20, 1000) as usize;

    let guidelines_root = zorai_guidelines_dir();
    let guideline_path = resolve_guideline_path(&guidelines_root, guideline)?;
    let content = std::fs::read_to_string(&guideline_path)
        .with_context(|| format!("failed to read {}", guideline_path.display()))?;
    let total_lines = content.lines().count();
    let excerpt = content
        .lines()
        .take(max_lines)
        .collect::<Vec<_>>()
        .join("\n");

    Ok(serde_json::json!({
        "guidelines_root": guidelines_root,
        "path": guideline_path.strip_prefix(&guidelines_root).unwrap_or(guideline_path.as_path()).display().to_string(),
        "content": excerpt,
        "truncated": total_lines > max_lines,
        "total_lines": total_lines,
    }))
}

fn encode_local_guideline_cursor(path: &str) -> String {
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(path.as_bytes());
    format!("local-guideline-list:{encoded}")
}

fn decode_local_guideline_cursor(cursor: Option<&str>) -> Result<Option<String>> {
    let Some(cursor) = cursor else {
        return Ok(None);
    };
    let payload = cursor
        .strip_prefix("local-guideline-list:")
        .ok_or_else(|| anyhow::anyhow!("invalid local guideline cursor"))?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .map_err(|error| anyhow::anyhow!("invalid local guideline cursor: {error}"))?;
    let value = String::from_utf8(bytes)
        .map_err(|error| anyhow::anyhow!("invalid local guideline cursor: {error}"))?;
    Ok(Some(value))
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
    let requires_approval = args
        .get("requires_approval")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let launch_assignments = parse_goal_launch_assignments(args)?;

    let resp = daemon_roundtrip(ClientMessage::AgentStartGoalRun {
        goal,
        title,
        thread_id,
        session_id,
        priority,
        client_request_id,
        launch_assignments,
        autonomy_level,
        client_surface: None,
        requires_approval,
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

fn parse_goal_launch_assignments(args: &Value) -> Result<Vec<GoalAgentAssignment>> {
    let Some(raw) = args.get("launch_assignments") else {
        return Ok(Vec::new());
    };
    let assignments = raw
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("'launch_assignments' must be an array"))?;

    assignments
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let role_id = required_assignment_string(value, index, "role_id")?;
            let provider = required_assignment_string(value, index, "provider")?;
            let model = required_assignment_string(value, index, "model")?;
            let reasoning_effort = value
                .get("reasoning_effort")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned);
            let enabled = value
                .get("enabled")
                .and_then(|value| value.as_bool())
                .unwrap_or(true);
            let inherit_from_main = value
                .get("inherit_from_main")
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
            Ok(GoalAgentAssignment {
                role_id,
                enabled,
                provider,
                model,
                reasoning_effort,
                inherit_from_main,
            })
        })
        .collect()
}

fn required_assignment_string(value: &Value, index: usize, field: &str) -> Result<String> {
    value
        .get(field)
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            anyhow::anyhow!("launch_assignments[{index}].{field} must be a non-empty string")
        })
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

    info!("zorai-mcp server starting");

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

    info!("zorai-mcp server stopped");
    Ok(())
}
