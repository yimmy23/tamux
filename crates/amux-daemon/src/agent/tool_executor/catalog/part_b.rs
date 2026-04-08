fn add_available_tools_part_b(
    tools: &mut Vec<ToolDefinition>,
    config: &AgentConfig,
    agent_data_dir: &std::path::Path,
    has_workspace_topology: bool,
) {
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
        "fetch_gateway_history",
        "Fetch recent messages from the current gateway conversation thread. Use this when handling platform messages and you need additional prior context.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "count": { "type": "integer", "description": "Number of recent messages to fetch (default: 10, max: 100)" }
            }
        }),
    ));
    tools.push(tool_def(
        "session_search",
        "Search prior sessions, transcripts, cognitive traces, and operational history for relevant past context.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search query" },
                "limit": { "type": "integer", "description": "Max results (default: 8)" }
            },
            "required": ["query"]
        }),
    ));
    if config.enable_honcho_memory && !config.honcho_api_key.trim().is_empty() {
        tools.push(tool_def(
            "agent_query_memory",
            "Query Honcho cross-session memory for long-term user, workspace, or assistant context.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Question to ask Honcho memory." }
                },
                "required": ["query"]
            }),
        ));
    }
    tools.push(tool_def(
        "onecontext_search",
        "Search Aline OneContext history for related prior sessions/events/turns.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search query" },
                "scope": { "type": "string", "enum": ["session", "event", "turn"], "description": "Search scope (default: session)" },
                "no_regex": { "type": "boolean", "description": "Treat query as plain text (default: true)" },
                "timeout_seconds": { "type": "integer", "minimum": 0, "maximum": 600, "description": "Max time to wait for completion (default: 300, max: 600)" }
            },
            "required": ["query"]
        }),
    ));

    if has_workspace_topology {
        tools.push(tool_def(
            "list_sessions",
            "List frontend-reported workspace sessions and panes when workspace topology is available.",
            serde_json::json!({ "type": "object", "properties": {} }),
        ));
    }

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
        "Update curated persistent memory. Use this only for durable operator preferences or stable project facts, not temporary task state.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "target": { "type": "string", "enum": ["memory", "user", "soul"], "description": "Memory file to update (default: memory)" },
                "mode": { "type": "string", "enum": ["replace", "append", "remove"], "description": "How to apply the content (default: replace)" },
                "content": { "type": "string", "description": "Markdown content or exact fragment for remove mode" }
            },
            "required": ["content"]
        }),
    ));

    tools.push(tool_def(
        "list_skills",
        "List reusable local skills available to the tamux agent from ~/.tamux/skills (platform dependent). Includes built-in, generated, community, and plugin-bundled skills.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Optional name/path filter for relevant skills" },
                "limit": { "type": "integer", "description": "Max skills to return (default: 20)" }
            }
        }),
    ));

    tools.push(tool_def(
        "get_current_datetime",
        "Fetch the current local and UTC datetime from the daemon host, including a Unix timestamp in milliseconds.",
        serde_json::json!({
            "type": "object",
            "properties": {}
        }),
    ));

    tools.push(tool_def(
        "semantic_query",
        "Query local workspace manifests, compose services, code import relationships, learned workspace conventions, and recent temporal workspace history.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "kind": { "type": "string", "enum": ["summary", "packages", "dependencies", "dependents", "services", "service_dependencies", "service_dependents", "imports", "imported_by", "conventions", "temporal"], "description": "Semantic query mode (default: summary)" },
                "target": { "type": "string", "description": "Package, service, file path fragment, or module name depending on the selected semantic query mode" },
                "path": { "type": "string", "description": "Optional workspace root directory; defaults to the active session cwd or current directory" },
                "limit": { "type": "integer", "description": "Max results to list for list-oriented semantic modes (default: 20)" }
            }
        }),
    ));

    tools.push(tool_def(
        "summary",
        "Backward-compatible alias for semantic_query with kind set to summary. Use this to get a workspace summary while preserving legacy tool callers.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "target": { "type": "string", "description": "Optional package, service, file path fragment, or module name to focus the summary" },
                "path": { "type": "string", "description": "Optional workspace root directory; defaults to the active session cwd or current directory" },
                "limit": { "type": "integer", "description": "Max results to list for list-oriented summary output (default: 20)" }
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

    tools.push(tool_def(
        "justify_skill_skip",
        "Record an explicit rationale for proceeding without a local skill recommendation when discovery confidence is weak or none.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "rationale": { "type": "string", "description": "Why no installed local skill is suitable for the current request" }
            },
            "required": ["rationale"]
        }),
    ));

}
