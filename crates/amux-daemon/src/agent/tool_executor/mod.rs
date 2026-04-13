#![allow(dead_code)]

//! Tool execution for the agent engine.
//!
//! Maps tool calls to daemon infrastructure. Tools that require frontend
//! state (workspace/pane/browser) are not available in daemon mode — only
//! tools that can execute headlessly are included here.

include!("prelude.rs");
include!("search_runtime.rs");
include!("result_metadata.rs");
include!("catalog/part_a.rs");
include!("catalog/part_b.rs");
include!("catalog/part_c.rs");
include!("catalog/part_d.rs");
include!("thread_tools.rs");

fn tool_required_fields(parameters: &serde_json::Value) -> Vec<String> {
    parameters
        .get("required")
        .and_then(|value| value.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn tool_parameter_names(parameters: &serde_json::Value) -> Vec<String> {
    parameters
        .get("properties")
        .and_then(|value| value.as_object())
        .map(|properties| properties.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default()
}

fn summarize_tool_definition(tool: ToolDefinition) -> amux_protocol::ToolDescriptorPublic {
    amux_protocol::ToolDescriptorPublic {
        name: tool.function.name,
        description: tool.function.description,
        required: tool_required_fields(&tool.function.parameters),
        parameters: tool.function.parameters.to_string(),
    }
}

fn query_tokens(query: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    for ch in query.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            current.push(ch.to_ascii_lowercase());
        } else if !current.is_empty() {
            tokens.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens.sort();
    tokens.dedup();
    tokens
}

fn score_tool_definition(
    tool: &ToolDefinition,
    query_lower: &str,
    tokens: &[String],
) -> Option<amux_protocol::ToolSearchMatchPublic> {
    let name = tool.function.name.to_ascii_lowercase();
    let description = tool.function.description.to_ascii_lowercase();
    let parameter_names = tool_parameter_names(&tool.function.parameters)
        .into_iter()
        .map(|value| value.to_ascii_lowercase())
        .collect::<Vec<_>>();

    let mut score = 0u32;
    let mut matched_fields = std::collections::BTreeSet::new();

    if name == query_lower {
        score += 100;
        matched_fields.insert("name_exact".to_string());
    } else if name.contains(query_lower) {
        score += 60;
        matched_fields.insert("name".to_string());
    }
    if description.contains(query_lower) {
        score += 25;
        matched_fields.insert("description".to_string());
    }
    if parameter_names
        .iter()
        .any(|value| value.contains(query_lower))
    {
        score += 15;
        matched_fields.insert("parameters".to_string());
    }

    for token in tokens {
        if token == query_lower {
            continue;
        }
        if name.contains(token) {
            score += 12;
            matched_fields.insert("name".to_string());
        }
        if description.contains(token) {
            score += 4;
            matched_fields.insert("description".to_string());
        }
        if parameter_names.iter().any(|value| value.contains(token)) {
            score += 3;
            matched_fields.insert("parameters".to_string());
        }
    }

    if score == 0 {
        return None;
    }

    Some(amux_protocol::ToolSearchMatchPublic {
        name: tool.function.name.clone(),
        description: tool.function.description.clone(),
        required: tool_required_fields(&tool.function.parameters),
        parameters: tool.function.parameters.to_string(),
        score,
        matched_fields: matched_fields.into_iter().collect(),
    })
}

pub fn reorder_tools_by_heuristics(
    tools: &mut [ToolDefinition],
    heuristic_store: &super::learning::heuristics::HeuristicStore,
    task_type: &str,
) {
    if task_type.is_empty() {
        return;
    }

    // Get effectiveness scores for tools relevant to this task type (min 5 samples)
    let scores: std::collections::HashMap<String, f64> = heuristic_store
        .tool_heuristics
        .iter()
        .filter(|h| h.task_type == task_type && h.usage_count >= 5)
        .map(|h| (h.tool_name.clone(), h.effectiveness_score))
        .collect();

    if scores.is_empty() {
        return;
    }

    // Stable sort: tools with heuristic scores go first (sorted by score desc),
    // tools without scores keep their relative order after.
    tools.sort_by(|a, b| {
        let score_a = scores.get(&a.function.name).copied().unwrap_or(-1.0);
        let score_b = scores.get(&b.function.name).copied().unwrap_or(-1.0);
        score_b
            .partial_cmp(&score_a)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
}

pub fn get_available_tools(
    config: &AgentConfig,
    agent_data_dir: &std::path::Path,
    has_workspace_topology: bool,
) -> Vec<ToolDefinition> {
    let mut tools = Vec::new();
    add_available_tools_part_a(&mut tools, config, agent_data_dir, has_workspace_topology);
    add_available_tools_part_b(&mut tools, config, agent_data_dir, has_workspace_topology);
    add_available_tools_part_c(&mut tools, config, agent_data_dir, has_workspace_topology);
    add_available_tools_part_d(&mut tools, config, agent_data_dir, has_workspace_topology);
    tools
}

pub(crate) fn list_available_tools_public(
    config: &AgentConfig,
    agent_data_dir: &std::path::Path,
    has_workspace_topology: bool,
    limit: usize,
    offset: usize,
) -> amux_protocol::ToolListResultPublic {
    let tools = get_available_tools(config, agent_data_dir, has_workspace_topology);
    let total = tools.len();
    let limit = limit.clamp(1, 200);
    let items = tools
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(summarize_tool_definition)
        .collect();

    amux_protocol::ToolListResultPublic {
        total,
        limit,
        offset,
        items,
    }
}

pub(crate) fn search_available_tools_public(
    config: &AgentConfig,
    agent_data_dir: &std::path::Path,
    has_workspace_topology: bool,
    query: &str,
    limit: usize,
    offset: usize,
) -> amux_protocol::ToolSearchResultPublic {
    let normalized_query = query.trim().to_ascii_lowercase();
    let tokens = query_tokens(&normalized_query);
    let mut matches = get_available_tools(config, agent_data_dir, has_workspace_topology)
        .into_iter()
        .filter_map(|tool| score_tool_definition(&tool, &normalized_query, &tokens))
        .collect::<Vec<_>>();
    matches.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.name.cmp(&right.name))
    });
    let total = matches.len();
    let limit = limit.clamp(1, 200);
    let items = matches.into_iter().skip(offset).take(limit).collect();

    amux_protocol::ToolSearchResultPublic {
        query: query.to_string(),
        total,
        limit,
        offset,
        items,
    }
}

include!("memory_flush.rs");
include!("memory_tools.rs");
include!("execute_tool_impl.rs");

include!("parse_args.rs");
include!("file_tools.rs");
include!("managed_helpers.rs");
include!("system_history.rs");
include!("skills_and_search.rs");
include!("workflow_fetch.rs");
include!("setup_web.rs");
include!("terminal_runtime.rs");
include!("python_execute.rs");
include!("managed_commands.rs");
include!("terminal_alloc.rs");
include!("subagents.rs");
include!("tasks.rs");
include!("terminal_input.rs");
include!("gateway_workspace.rs");

#[cfg(test)]
mod tests;
