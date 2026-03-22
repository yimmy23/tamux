//! Runtime generated-tool registry with conservative guardrails.

use std::path::{Path, PathBuf};
use std::process::Stdio;

use anyhow::{Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};

use super::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum GeneratedToolKind {
    Cli,
    OpenApi,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GeneratedToolParameter {
    name: String,
    description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cli_flag: Option<String>,
    #[serde(default)]
    required: bool,
    #[serde(default = "default_parameter_type")]
    param_type: String,
    #[serde(default = "default_parameter_location")]
    location: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GeneratedCliSpec {
    invocation: Vec<String>,
    help_source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GeneratedOpenApiSpec {
    spec_url: String,
    base_url: String,
    path: String,
    method: String,
    operation_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct GeneratedToolRecord {
    id: String,
    name: String,
    description: String,
    kind: GeneratedToolKind,
    parameters: Vec<GeneratedToolParameter>,
    status: String,
    created_at: u64,
    updated_at: u64,
    last_used_at: Option<u64>,
    calls_total: u32,
    calls_success: u32,
    calls_failure: u32,
    calls_timeout: u32,
    sessions_used: u32,
    last_session_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    promoted_skill_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cli: Option<GeneratedCliSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    openapi: Option<GeneratedOpenApiSpec>,
}

impl GeneratedToolRecord {
    fn effectiveness(&self) -> f64 {
        if self.calls_total == 0 {
            return 0.5;
        }
        let success = self.calls_success as f64 / self.calls_total as f64;
        let recency = if self.last_used_at.is_some() {
            1.0
        } else {
            0.5
        };
        success * 0.8 + recency * 0.2
    }

    fn is_active(&self) -> bool {
        matches!(self.status.as_str(), "active" | "promoted")
    }
}

pub(super) fn generated_tool_definitions(
    config: &AgentConfig,
    agent_data_dir: &Path,
) -> Vec<ToolDefinition> {
    if !config.tool_synthesis.enabled {
        return Vec::new();
    }
    load_generated_tools(agent_data_dir)
        .unwrap_or_default()
        .into_iter()
        .filter(|tool| tool.is_active())
        .map(|tool| {
            let mut properties = serde_json::Map::new();
            let mut required = Vec::new();
            for parameter in &tool.parameters {
                properties.insert(
                    parameter.name.clone(),
                    serde_json::json!({
                        "type": parameter.param_type,
                        "description": parameter.description,
                    }),
                );
                if parameter.required {
                    required.push(parameter.name.clone());
                }
            }
            ToolDefinition {
                tool_type: "function".to_string(),
                function: ToolFunctionDef {
                    name: tool.id.clone(),
                    description: format!("Generated tool: {}", tool.description),
                    parameters: serde_json::json!({
                        "type": "object",
                        "properties": properties,
                        "required": required,
                    }),
                },
            }
        })
        .collect()
}

pub(super) async fn synthesize_tool(
    args: &serde_json::Value,
    agent: &AgentEngine,
    agent_data_dir: &Path,
    http_client: &reqwest::Client,
) -> Result<String> {
    let config = agent.config.read().await.clone();
    if !config.tool_synthesis.enabled {
        anyhow::bail!("tool synthesis is disabled in agent config");
    }

    let kind = args
        .get("kind")
        .and_then(|value| value.as_str())
        .unwrap_or("cli");
    let target = args
        .get("target")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("missing 'target' argument"))?;
    let requested_name = args.get("name").and_then(|value| value.as_str());
    let activate = args
        .get("activate")
        .and_then(|value| value.as_bool())
        .unwrap_or(false);

    let record = match kind {
        "cli" => synthesize_cli_tool(target, requested_name).await?,
        "openapi" => synthesize_openapi_tool(target, requested_name, args, http_client).await?,
        other => anyhow::bail!("unsupported synthesis kind: {other}"),
    };
    let mut record = record;
    record.status = if activate && !config.tool_synthesis.require_activation {
        "active".to_string()
    } else {
        "new".to_string()
    };
    ensure_generated_tool_capacity(
        agent_data_dir,
        config.tool_synthesis.max_generated_tools,
        &record.id,
    )?;
    save_generated_tool(agent_data_dir, &record)?;
    prune_generated_tools(agent_data_dir, config.tool_synthesis.max_generated_tools)?;

    Ok(serde_json::to_string_pretty(&record)?)
}

pub(super) fn list_generated_tools(agent_data_dir: &Path) -> Result<String> {
    Ok(serde_json::to_string_pretty(&load_generated_tools(
        agent_data_dir,
    )?)?)
}

pub(super) async fn execute_generated_tool(
    tool_name: &str,
    args: &serde_json::Value,
    agent: &AgentEngine,
    agent_data_dir: &Path,
    http_client: &reqwest::Client,
    session_key: Option<&str>,
) -> Result<Option<String>> {
    let mut record = match load_generated_tool(agent_data_dir, tool_name)? {
        Some(record) => record,
        None => return Ok(None),
    };
    if !record.is_active() {
        anyhow::bail!("generated tool `{tool_name}` is not active");
    }

    let sandbox = agent.config.read().await.tool_synthesis.sandbox.clone();
    let output = match record.kind {
        GeneratedToolKind::Cli => run_cli_generated_tool(&record, args, &sandbox).await,
        GeneratedToolKind::OpenApi => {
            run_openapi_generated_tool(&record, args, &sandbox, http_client).await
        }
    };

    let now = now_millis();
    record.calls_total = record.calls_total.saturating_add(1);
    record.updated_at = now;
    record.last_used_at = Some(now);
    if let Some(session_key) = session_key {
        if record.last_session_key.as_deref() != Some(session_key) {
            record.sessions_used = record.sessions_used.saturating_add(1);
            record.last_session_key = Some(session_key.to_string());
        }
    }
    match &output {
        Ok(_) => record.calls_success = record.calls_success.saturating_add(1),
        Err(error) if error.to_string().contains("timed out") => {
            record.calls_timeout = record.calls_timeout.saturating_add(1);
        }
        Err(_) => record.calls_failure = record.calls_failure.saturating_add(1),
    }
    if record.effectiveness()
        > agent
            .config
            .read()
            .await
            .tool_synthesis
            .auto_promote_threshold
        && record.calls_total >= 3
        && record.status == "active"
    {
        record.status = "promotable".to_string();
    }
    save_generated_tool(agent_data_dir, &record)?;
    output.map(Some)
}

pub(super) fn promote_generated_tool(agent_data_dir: &Path, tool_name: &str) -> Result<String> {
    let mut record = load_generated_tool(agent_data_dir, tool_name)?
        .ok_or_else(|| anyhow::anyhow!("unknown generated tool `{tool_name}`"))?;
    let skill_dir = super::skills_dir(agent_data_dir).join("generated");
    std::fs::create_dir_all(&skill_dir)?;
    let path = skill_dir.join(format!("use-{}.md", record.id));
    let parameter_lines = if record.parameters.is_empty() {
        "- none".to_string()
    } else {
        record
            .parameters
            .iter()
            .map(|parameter| format!("- `{}`: {}", parameter.name, parameter.description))
            .collect::<Vec<_>>()
            .join("\n")
    };
    let implementation = match (&record.cli, &record.openapi) {
        (Some(cli), None) => cli.invocation.join(" "),
        (None, Some(api)) => format!("{} {}", api.method, api.path),
        _ => record.name.clone(),
    };
    let content = format!(
        "# Skill: Use {}\n\n## When to Use\n- Use this generated tool when the task needs `{}`.\n\n## How\n```text\n{}\n```\n\n## Parameters\n{}\n\n## Notes\n- Generated from {}\n- Effectiveness: {:.0}%\n- Used in {} sessions\n",
        record.name,
        record.description,
        implementation,
        parameter_lines,
        record
            .cli
            .as_ref()
            .map(|cli| cli.help_source.as_str())
            .or_else(|| record.openapi.as_ref().map(|api| api.spec_url.as_str()))
            .unwrap_or("generated runtime metadata"),
        record.effectiveness() * 100.0,
        record.sessions_used,
    );
    std::fs::write(&path, content)?;
    record.status = "promoted".to_string();
    record.promoted_skill_path = Some(path.display().to_string());
    save_generated_tool(agent_data_dir, &record)?;
    Ok(serde_json::to_string_pretty(&record)?)
}

pub(super) fn activate_generated_tool(agent_data_dir: &Path, tool_name: &str) -> Result<String> {
    let mut record = load_generated_tool(agent_data_dir, tool_name)?
        .ok_or_else(|| anyhow::anyhow!("unknown generated tool `{tool_name}`"))?;
    record.status = "active".to_string();
    record.updated_at = now_millis();
    save_generated_tool(agent_data_dir, &record)?;
    Ok(serde_json::to_string_pretty(&record)?)
}

fn load_generated_tools(agent_data_dir: &Path) -> Result<Vec<GeneratedToolRecord>> {
    let root = generated_tools_dir(agent_data_dir);
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut tools = Vec::new();
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path().join("tool.json");
        if !path.exists() {
            continue;
        }
        let raw = std::fs::read_to_string(path)?;
        if let Ok(record) = serde_json::from_str::<GeneratedToolRecord>(&raw) {
            tools.push(record);
        }
    }
    tools.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    Ok(tools)
}

fn load_generated_tool(
    agent_data_dir: &Path,
    tool_name: &str,
) -> Result<Option<GeneratedToolRecord>> {
    for tool in load_generated_tools(agent_data_dir)? {
        if tool.id == tool_name {
            return Ok(Some(tool));
        }
    }
    Ok(None)
}

fn save_generated_tool(agent_data_dir: &Path, record: &GeneratedToolRecord) -> Result<()> {
    let root = generated_tools_dir(agent_data_dir).join(&record.id);
    std::fs::create_dir_all(&root)?;
    std::fs::write(
        root.join("tool.json"),
        serde_json::to_string_pretty(record)?,
    )?;
    Ok(())
}

fn ensure_generated_tool_capacity(
    agent_data_dir: &Path,
    max_tools: usize,
    incoming_id: &str,
) -> Result<()> {
    let tools = load_generated_tools(agent_data_dir)?;
    if tools.iter().any(|record| record.id == incoming_id) || tools.len() < max_tools {
        return Ok(());
    }
    let prunable = tools
        .iter()
        .filter(|record| generated_tool_is_prunable(record))
        .count();
    if prunable == 0 {
        anyhow::bail!(
            "generated tool registry is full at {max_tools} tools and only active/promoted tools remain; remove or demote one before synthesizing another"
        );
    }
    Ok(())
}

fn generated_tool_is_prunable(record: &GeneratedToolRecord) -> bool {
    !matches!(record.status.as_str(), "active" | "promoted")
}

fn prune_generated_tools(agent_data_dir: &Path, max_tools: usize) -> Result<()> {
    let tools = load_generated_tools(agent_data_dir)?;
    if tools.len() <= max_tools {
        return Ok(());
    }
    let excess = tools.len() - max_tools;
    let mut removed = 0usize;
    for record in tools.iter().rev() {
        if removed >= excess {
            break;
        }
        if !generated_tool_is_prunable(record) {
            continue;
        }
        let dir = generated_tools_dir(agent_data_dir).join(&record.id);
        std::fs::remove_dir_all(&dir).with_context(|| {
            format!(
                "failed to prune generated tool `{}` from {}",
                record.id,
                dir.display()
            )
        })?;
        removed += 1;
    }
    if tools.len().saturating_sub(removed) > max_tools {
        anyhow::bail!(
            "generated tool registry still exceeds the configured limit because active/promoted tools are protected from pruning"
        );
    }
    Ok(())
}

async fn synthesize_cli_tool(
    target: &str,
    requested_name: Option<&str>,
) -> Result<GeneratedToolRecord> {
    let invocation = target
        .split_whitespace()
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    if invocation.is_empty() {
        anyhow::bail!("empty CLI target");
    }
    validate_safe_cli_invocation(&invocation)?;
    let output = tokio::time::timeout(
        Duration::from_secs(10),
        tokio::process::Command::new(&invocation[0])
            .args(&invocation[1..])
            .arg("--help")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output(),
    )
    .await
    .context("CLI help timed out")??;
    let help = String::from_utf8_lossy(&output.stdout).to_string();
    let description = help
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(str::trim)
        .unwrap_or(target)
        .to_string();
    let parameters = parse_cli_help_parameters(&help);
    let name = requested_name
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| sanitize_tool_name(&invocation.join("_")));
    Ok(GeneratedToolRecord {
        id: name.clone(),
        name,
        description,
        kind: GeneratedToolKind::Cli,
        parameters,
        status: "new".to_string(),
        created_at: now_millis(),
        updated_at: now_millis(),
        last_used_at: None,
        calls_total: 0,
        calls_success: 0,
        calls_failure: 0,
        calls_timeout: 0,
        sessions_used: 0,
        last_session_key: None,
        promoted_skill_path: None,
        cli: Some(GeneratedCliSpec {
            invocation,
            help_source: target.to_string(),
        }),
        openapi: None,
    })
}

async fn synthesize_openapi_tool(
    spec_url: &str,
    requested_name: Option<&str>,
    args: &serde_json::Value,
    http_client: &reqwest::Client,
) -> Result<GeneratedToolRecord> {
    let spec = http_client
        .get(spec_url)
        .send()
        .await?
        .error_for_status()?
        .json::<serde_json::Value>()
        .await?;
    let operation_id_hint = args.get("operation_id").and_then(|value| value.as_str());
    let (path, method, operation) = select_openapi_operation(&spec, operation_id_hint)?;
    let parameters = operation
        .get("parameters")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    Some(GeneratedToolParameter {
                        name: item.get("name")?.as_str()?.to_string(),
                        description: item
                            .get("description")
                            .and_then(|value| value.as_str())
                            .unwrap_or("openapi parameter")
                            .to_string(),
                        cli_flag: None,
                        required: item
                            .get("required")
                            .and_then(|value| value.as_bool())
                            .unwrap_or(false),
                        param_type: item
                            .get("schema")
                            .and_then(|value| value.get("type"))
                            .and_then(|value| value.as_str())
                            .unwrap_or("string")
                            .to_string(),
                        location: item
                            .get("in")
                            .and_then(|value| value.as_str())
                            .unwrap_or("query")
                            .to_string(),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let name = requested_name
        .map(ToOwned::to_owned)
        .or_else(|| {
            operation
                .get("operationId")
                .and_then(|value| value.as_str())
                .map(sanitize_tool_name)
        })
        .unwrap_or_else(|| sanitize_tool_name(&format!("{}_{}", method, path.replace('/', "_"))));
    let base_url = spec
        .get("servers")
        .and_then(|value| value.as_array())
        .and_then(|items| items.first())
        .and_then(|value| value.get("url"))
        .and_then(|value| value.as_str())
        .map(ToOwned::to_owned)
        .or_else(|| {
            reqwest::Url::parse(spec_url)
                .ok()
                .map(|url| format!("{}://{}", url.scheme(), url.host_str().unwrap_or_default()))
        })
        .unwrap_or_else(|| spec_url.to_string());
    Ok(GeneratedToolRecord {
        id: name.clone(),
        name,
        description: operation
            .get("summary")
            .and_then(|value| value.as_str())
            .unwrap_or("generated openapi tool")
            .to_string(),
        kind: GeneratedToolKind::OpenApi,
        parameters,
        status: "new".to_string(),
        created_at: now_millis(),
        updated_at: now_millis(),
        last_used_at: None,
        calls_total: 0,
        calls_success: 0,
        calls_failure: 0,
        calls_timeout: 0,
        sessions_used: 0,
        last_session_key: None,
        promoted_skill_path: None,
        cli: None,
        openapi: Some(GeneratedOpenApiSpec {
            spec_url: spec_url.to_string(),
            base_url,
            path,
            method,
            operation_id: operation
                .get("operationId")
                .and_then(|value| value.as_str())
                .map(ToOwned::to_owned),
        }),
    })
}

async fn run_cli_generated_tool(
    record: &GeneratedToolRecord,
    args: &serde_json::Value,
    sandbox: &ToolSynthesisSandboxConfig,
) -> Result<String> {
    let cli = record.cli.as_ref().context("missing CLI spec")?;
    validate_safe_cli_invocation(&cli.invocation)?;
    let mut command = tokio::process::Command::new(&cli.invocation[0]);
    command.args(&cli.invocation[1..]);
    for parameter in &record.parameters {
        let Some(value) = args.get(&parameter.name) else {
            continue;
        };
        if let Some(flag) = parameter.cli_flag.as_deref() {
            if value.as_bool() == Some(false) {
                continue;
            }
            command.arg(flag);
            if parameter.param_type != "boolean" {
                command.arg(value_to_command_arg(value));
            }
        }
    }
    let output = tokio::time::timeout(
        Duration::from_secs(sandbox.max_execution_time_secs.max(1)),
        command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output(),
    )
    .await
    .context("generated tool execution timed out")??;
    let mut text = String::from_utf8_lossy(&output.stdout).to_string();
    if text.trim().is_empty() {
        text = String::from_utf8_lossy(&output.stderr).to_string();
    }
    cap_output(text, sandbox.max_output_kb)
}

async fn run_openapi_generated_tool(
    record: &GeneratedToolRecord,
    args: &serde_json::Value,
    sandbox: &ToolSynthesisSandboxConfig,
    http_client: &reqwest::Client,
) -> Result<String> {
    if !sandbox.allow_network {
        anyhow::bail!("generated OpenAPI tools require network access; enable tool_synthesis.sandbox.allow_network first");
    }
    let spec = record.openapi.as_ref().context("missing OpenAPI spec")?;
    if spec.method.to_uppercase() != "GET" {
        anyhow::bail!("only GET OpenAPI operations are supported for generated tools");
    }
    let mut url = reqwest::Url::parse(&format!(
        "{}{}",
        spec.base_url.trim_end_matches('/'),
        spec.path
    ))?;
    for parameter in &record.parameters {
        if parameter.location != "query" {
            continue;
        }
        if let Some(value) = args.get(&parameter.name) {
            url.query_pairs_mut()
                .append_pair(&parameter.name, &value_to_command_arg(value));
        }
    }
    let response = tokio::time::timeout(
        Duration::from_secs(sandbox.max_execution_time_secs.max(1)),
        http_client.get(url).send(),
    )
    .await
    .context("generated OpenAPI call timed out")??;
    let body = response.error_for_status()?.text().await?;
    cap_output(body, sandbox.max_output_kb)
}

fn parse_cli_help_parameters(help: &str) -> Vec<GeneratedToolParameter> {
    let regex = Regex::new(
        r"^\s*(?:-[A-Za-z],\s*)?(--[A-Za-z0-9][A-Za-z0-9-]*)(?:[ =]<?([A-Za-z0-9_-]+)>?)?\s{2,}(.*)$",
    )
    .expect("valid CLI help regex");
    help.lines()
        .filter_map(|line| {
            let captures = regex.captures(line)?;
            let flag = captures.get(1)?.as_str().to_string();
            let name = flag.trim_start_matches("--").replace('-', "_");
            let takes_value = captures.get(2).is_some();
            Some(GeneratedToolParameter {
                name,
                description: captures
                    .get(3)
                    .map(|value| value.as_str().trim().to_string())
                    .unwrap_or_else(|| "generated CLI parameter".to_string()),
                cli_flag: Some(flag),
                required: false,
                param_type: if takes_value { "string" } else { "boolean" }.to_string(),
                location: "argument".to_string(),
            })
        })
        .collect()
}

fn select_openapi_operation<'a>(
    spec: &'a serde_json::Value,
    operation_id_hint: Option<&str>,
) -> Result<(String, String, &'a serde_json::Value)> {
    let paths = spec
        .get("paths")
        .and_then(|value| value.as_object())
        .context("OpenAPI spec has no paths object")?;
    for (path, item) in paths {
        let Some(item_obj) = item.as_object() else {
            continue;
        };
        for (method, operation) in item_obj {
            if method.to_ascii_uppercase() != "GET" {
                continue;
            }
            if let Some(operation_id_hint) = operation_id_hint {
                if operation
                    .get("operationId")
                    .and_then(|value| value.as_str())
                    != Some(operation_id_hint)
                {
                    continue;
                }
            }
            return Ok((path.to_string(), method.to_ascii_uppercase(), operation));
        }
    }
    anyhow::bail!("no matching GET OpenAPI operation found")
}

fn generated_tools_dir(agent_data_dir: &Path) -> PathBuf {
    agent_data_dir
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("generated-tools")
}

fn sanitize_tool_name(raw: &str) -> String {
    let mut value = raw
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    while value.contains("__") {
        value = value.replace("__", "_");
    }
    value.trim_matches('_').to_string()
}

fn validate_safe_cli_invocation(invocation: &[String]) -> Result<()> {
    const SAFE_READONLY: &[&str] = &[
        "git", "kubectl", "docker", "cargo", "npm", "pnpm", "rg", "find", "ls", "cat",
    ];
    const DENY_TOKENS: &[&str] = &[
        "apply", "push", "commit", "delete", "rm", "write", "set", "scale", "patch", "exec", "run",
        "install", "publish",
    ];
    let base = invocation.first().map(String::as_str).unwrap_or_default();
    if !SAFE_READONLY.contains(&base) {
        anyhow::bail!(
            "CLI synthesis only allows conservative read-mostly commands; `{base}` is not allowed"
        );
    }
    if invocation
        .iter()
        .skip(1)
        .any(|token| DENY_TOKENS.contains(&token.as_str()))
    {
        anyhow::bail!(
            "CLI synthesis rejected because the invocation includes a mutating or risky token"
        );
    }
    Ok(())
}

fn value_to_command_arg(value: &serde_json::Value) -> String {
    value
        .as_str()
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| value.to_string())
}

fn cap_output(mut text: String, max_output_kb: usize) -> Result<String> {
    let max_bytes = max_output_kb.max(1) * 1024;
    if text.len() > max_bytes {
        text.truncate(max_bytes);
        text.push_str("\n[output truncated]");
    }
    Ok(text)
}

fn default_parameter_type() -> String {
    "string".to_string()
}

fn default_parameter_location() -> String {
    "argument".to_string()
}

impl AgentEngine {
    pub async fn list_generated_tools_json(&self) -> Result<String> {
        list_generated_tools(&self.data_dir)
    }

    pub async fn synthesize_tool_json(&self, request_json: &str) -> Result<String> {
        let args = serde_json::from_str::<serde_json::Value>(request_json)
            .context("invalid generated-tool synthesis request JSON")?;
        synthesize_tool(&args, self, &self.data_dir, &self.http_client).await
    }

    pub async fn run_generated_tool_json(
        &self,
        tool_name: &str,
        args_json: &str,
        session_key: Option<&str>,
    ) -> Result<String> {
        let args = serde_json::from_str::<serde_json::Value>(args_json)
            .context("invalid generated-tool arguments JSON")?;
        execute_generated_tool(
            tool_name,
            &args,
            self,
            &self.data_dir,
            &self.http_client,
            session_key,
        )
        .await?
        .ok_or_else(|| anyhow::anyhow!("unknown generated tool `{tool_name}`"))
    }

    pub async fn promote_generated_tool_json(&self, tool_name: &str) -> Result<String> {
        promote_generated_tool(&self.data_dir, tool_name)
    }

    pub async fn activate_generated_tool_json(&self, tool_name: &str) -> Result<String> {
        activate_generated_tool(&self.data_dir, tool_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cli_help_extracts_long_flags() {
        let help = "\
Usage: demo [OPTIONS]\n\
\n\
Options:\n\
  -n, --namespace <NAMESPACE>  Namespace to inspect\n\
      --all                    Include everything\n";
        let params = parse_cli_help_parameters(help);
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].name, "namespace");
        assert_eq!(params[0].param_type, "string");
        assert_eq!(params[1].name, "all");
        assert_eq!(params[1].param_type, "boolean");
    }

    #[test]
    fn prune_generated_tools_keeps_active_and_promoted_records() -> Result<()> {
        let agent_data_dir = std::env::temp_dir().join(format!(
            "amux-generated-tools-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&agent_data_dir)?;
        for (id, status, updated_at) in [
            ("tool-new-old", "new", 10),
            ("tool-active", "active", 11),
            ("tool-promoted", "promoted", 12),
            ("tool-new-fresh", "new", 13),
        ] {
            save_generated_tool(
                &agent_data_dir,
                &GeneratedToolRecord {
                    id: id.to_string(),
                    name: id.to_string(),
                    description: id.to_string(),
                    kind: GeneratedToolKind::Cli,
                    parameters: Vec::new(),
                    status: status.to_string(),
                    created_at: updated_at,
                    updated_at,
                    last_used_at: None,
                    calls_total: 0,
                    calls_success: 0,
                    calls_failure: 0,
                    calls_timeout: 0,
                    sessions_used: 0,
                    last_session_key: None,
                    promoted_skill_path: None,
                    cli: Some(GeneratedCliSpec {
                        invocation: vec!["echo".to_string()],
                        help_source: "help".to_string(),
                    }),
                    openapi: None,
                },
            )?;
        }

        prune_generated_tools(&agent_data_dir, 3)?;
        let remaining = load_generated_tools(&agent_data_dir)?
            .into_iter()
            .map(|record| record.id)
            .collect::<Vec<_>>();
        assert!(remaining.iter().any(|id| id == "tool-active"));
        assert!(remaining.iter().any(|id| id == "tool-promoted"));
        assert!(!remaining.iter().any(|id| id == "tool-new-old"));
        let _ = std::fs::remove_dir_all(&agent_data_dir);
        Ok(())
    }
}
