fn add_available_tools_part_b(
    tools: &mut Vec<ToolDefinition>,
    config: &AgentConfig,
    _agent_data_dir: &std::path::Path,
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
        "Replace the current todo list for this conversation. For goal-owned main tasks, set the todo list once per goal step, then submit the same items with only status changes.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "goal_run_id": {
                    "type": "string",
                    "description": "Required when the current task is the main task for a goal run. Must match the active goal run ID."
                },
                "goal_step_id": {
                    "type": "string",
                    "description": "Required when the current task is the main task for a goal run. Must match the active goal step ID and binds the full todo list to that one goal step; later calls in the same step may only update statuses."
                },
                "items": {
                    "type": "array",
                    "description": "Ordered todo items representing the current plan",
                    "items": {
                        "type": "object",
                        "properties": {
                            "content": { "type": "string", "description": "Short todo item text" },
                            "status": { "type": "string", "enum": ["pending", "in_progress", "completed", "blocked"], "description": "Current execution state" },
                            "step_index": { "type": "integer", "description": "Optional thread-local step label for non-goal todo displays. Do not use this to bind goal todos to a goal step." }
                        },
                        "required": ["content", "status"]
                    }
                }
            },
            "required": ["items"]
        }),
    ));

    tools.push(tool_def(
        "get_todos",
        "Fetch the current planner todos for a thread. Lookup is thread-scoped; optional task_id is only used for task or goal-run context validation.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "thread_id": { "type": "string", "description": "Agent thread ID whose todos should be returned" },
                "task_id": { "type": "string", "description": "Optional current task ID for validation and goal-run context" }
            },
            "required": ["thread_id"]
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
        "read_memory",
        "Read persistent MEMORY.md plus related structured memory layers. This response is injection-aware and avoids re-sending already injected fresh base markdown unless explicitly requested.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "include_already_injected": { "type": "boolean", "description": "Force inclusion of base markdown even when a fresh copy is already injected in the prompt (default: false)" },
                "include_base_markdown": { "type": "boolean", "description": "Include MEMORY.md content when not skipped by injection state (default: true)" },
                "include_operator_profile_json": { "type": "boolean", "description": "Include structured operator profile JSON (default: true)" },
                "include_operator_model_summary": { "type": "boolean", "description": "Include operator-model prompt summary when available (default: true)" },
                "include_thread_structural_memory": { "type": "boolean", "description": "Include thread structural memory summaries when available (default: true)" },
                "limit_per_layer": { "type": "integer", "description": "Maximum items returned for list-style layers (default: 5, max: 25)" }
            }
        }),
    ));

    tools.push(tool_def(
        "read_user",
        "Read persistent USER.md plus related structured operator layers. This response is injection-aware and avoids re-sending already injected fresh base markdown unless explicitly requested.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "include_already_injected": { "type": "boolean", "description": "Force inclusion of base markdown even when a fresh copy is already injected in the prompt (default: false)" },
                "include_base_markdown": { "type": "boolean", "description": "Include USER.md content when not skipped by injection state (default: true)" },
                "include_operator_profile_json": { "type": "boolean", "description": "Include structured operator profile JSON (default: true)" },
                "include_operator_model_summary": { "type": "boolean", "description": "Include operator-model prompt summary when available (default: true)" },
                "include_thread_structural_memory": { "type": "boolean", "description": "Include thread structural memory summaries when available (default: false)" },
                "limit_per_layer": { "type": "integer", "description": "Maximum items returned for list-style layers (default: 5, max: 25)" }
            }
        }),
    ));

    tools.push(tool_def(
        "read_soul",
        "Read persistent SOUL.md plus related structured context layers. This response is injection-aware and avoids re-sending already injected fresh base markdown unless explicitly requested.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "include_already_injected": { "type": "boolean", "description": "Force inclusion of base markdown even when a fresh copy is already injected in the prompt (default: false)" },
                "include_base_markdown": { "type": "boolean", "description": "Include SOUL.md content when not skipped by injection state (default: true)" },
                "include_operator_profile_json": { "type": "boolean", "description": "Include structured operator profile JSON (default: true)" },
                "include_operator_model_summary": { "type": "boolean", "description": "Include operator-model prompt summary when available (default: true)" },
                "include_thread_structural_memory": { "type": "boolean", "description": "Include thread structural memory summaries when available (default: false)" },
                "limit_per_layer": { "type": "integer", "description": "Maximum items returned for list-style layers (default: 5, max: 25)" }
            }
        }),
    ));

    tools.push(tool_def(
        "search_memory",
        "Search MEMORY.md plus related structured memory layers. This response is injection-aware and skips already injected fresh base markdown unless explicitly requested.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search query" },
                "limit": { "type": "integer", "description": "Maximum matches to return (default: 5, max: 25)" },
                "include_already_injected": { "type": "boolean", "description": "Allow fresh injected MEMORY.md content to appear in search matches (default: false)" },
                "include_base_markdown": { "type": "boolean", "description": "Search MEMORY.md when not skipped by injection state (default: true)" },
                "include_operator_profile_json": { "type": "boolean", "description": "Search structured operator profile JSON (default: true)" },
                "include_operator_model_summary": { "type": "boolean", "description": "Search operator-model prompt summary when available (default: true)" },
                "include_thread_structural_memory": { "type": "boolean", "description": "Search thread structural memory summaries when available (default: true)" }
            },
            "required": ["query"]
        }),
    ));

    tools.push(tool_def(
        "search_user",
        "Search USER.md plus related structured operator layers. This response is injection-aware and skips already injected fresh base markdown unless explicitly requested.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search query" },
                "limit": { "type": "integer", "description": "Maximum matches to return (default: 5, max: 25)" },
                "include_already_injected": { "type": "boolean", "description": "Allow fresh injected USER.md content to appear in search matches (default: false)" },
                "include_base_markdown": { "type": "boolean", "description": "Search USER.md when not skipped by injection state (default: true)" },
                "include_operator_profile_json": { "type": "boolean", "description": "Search structured operator profile JSON (default: true)" },
                "include_operator_model_summary": { "type": "boolean", "description": "Search operator-model prompt summary when available (default: true)" },
                "include_thread_structural_memory": { "type": "boolean", "description": "Search thread structural memory summaries when available (default: false)" }
            },
            "required": ["query"]
        }),
    ));

    tools.push(tool_def(
        "search_soul",
        "Search SOUL.md plus related structured context layers. This response is injection-aware and skips already injected fresh base markdown unless explicitly requested.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Search query" },
                "limit": { "type": "integer", "description": "Maximum matches to return (default: 5, max: 25)" },
                "include_already_injected": { "type": "boolean", "description": "Allow fresh injected SOUL.md content to appear in search matches (default: false)" },
                "include_base_markdown": { "type": "boolean", "description": "Search SOUL.md when not skipped by injection state (default: true)" },
                "include_operator_profile_json": { "type": "boolean", "description": "Search structured operator profile JSON (default: true)" },
                "include_operator_model_summary": { "type": "boolean", "description": "Search operator-model prompt summary when available (default: true)" },
                "include_thread_structural_memory": { "type": "boolean", "description": "Search thread structural memory summaries when available (default: false)" }
            },
            "required": ["query"]
        }),
    ));

    tools.push(tool_def(
        "list_skills",
        "List reusable local skills available to the zorai agent from ~/.zorai/skills (platform dependent). Includes built-in, generated, community, and plugin-bundled skills.",
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
        "list_tools",
        "List the tools currently available to the agent in this runtime context, including descriptions and argument schemas.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "limit": { "type": "integer", "description": "Maximum number of tools to return (default: 20)" },
                "offset": { "type": "integer", "description": "Zero-based pagination offset (default: 0)" }
            }
        }),
    ));

    tools.push(tool_def(
        "tool_search",
        "Search the currently available tools by name, description, and parameter names to find the best tool for a task.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "What capability or action you are looking for" },
                "limit": { "type": "integer", "description": "Maximum number of matches to return (default: 10)" },
                "offset": { "type": "integer", "description": "Zero-based pagination offset (default: 0)" }
            },
            "required": ["query"]
        }),
    ));

    tools.push(tool_def(
        "discover_guidelines",
        "Find matching local guidelines before choosing skills.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Brief intent query, 3-6 words." },
                "limit": { "type": "integer", "description": "Maximum number of ranked guideline candidates to return (default: 3)" },
                "session": { "type": "string", "description": "Optional live terminal session UUID for workspace-aware ranking" }
            },
            "required": ["query"]
        }),
    ));

    tools.push(tool_def(
        "read_guideline",
        "Read a local guideline document before skill discovery. Accepts a guideline name, relative path, or filename.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "guideline": { "type": "string", "description": "Guideline name, file stem, or relative path under the zorai guidelines directory" },
                "max_lines": { "type": "integer", "description": "Max lines to read (default: 200)" }
            },
            "required": ["guideline"]
        }),
    ));

    tools.push(tool_def(
        "list_guidelines",
        "List local guidelines available above the skill layer.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Optional guideline name or path filter" },
                "limit": { "type": "integer", "description": "Maximum guidelines to return (default: 20)" }
            }
        }),
    ));

    tools.push(tool_def(
        "discover_skills",
        "Find matching local skills fast.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string", "description": "Brief intent query, 3-6 words." },
                "limit": { "type": "integer", "description": "Maximum number of ranked skill candidates to return (default: 3)" },
                "session": { "type": "string", "description": "Optional live terminal session UUID for workspace-aware ranking" }
            },
            "required": ["query"]
        }),
    ));

    tools.push(tool_def(
        "read_skill",
        "Read one or more local skill documents before acting. Accepts skill names, relative paths, or generated skill filenames.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "skill": { "type": "string", "description": "Skill name, file stem, or relative path under the zorai skills directory" },
                "skills": {
                    "type": "array",
                    "description": "Skill names, file stems, or relative paths to read in one call",
                    "items": { "type": "string" }
                },
                "max_lines": { "type": "integer", "description": "Max lines to read (default: 200)" }
            }
        }),
    ));

    tools.push(tool_def(
        "ask_questions",
        "Show a blocking multiple-choice question to the operator in zorai clients and wait for one compact token answer. Put the full prompt and answer text in `content`; keep `options` limited to short ordered tokens like A/B/C/D or 1/2/3/4.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "content": { "type": "string", "description": "Full question content including the detailed answer text for each option" },
                "options": {
                    "type": "array",
                    "description": "Compact button tokens only, such as [\"A\", \"B\", \"C\"] or [\"1\", \"2\"]",
                    "items": { "type": "string" }
                },
                "session": { "type": "string", "description": "Optional live terminal session UUID to bias the prompt toward one workspace surface" }
            },
            "required": ["content", "options"]
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
