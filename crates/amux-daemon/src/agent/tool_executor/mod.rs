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

include!("memory_flush.rs");
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
