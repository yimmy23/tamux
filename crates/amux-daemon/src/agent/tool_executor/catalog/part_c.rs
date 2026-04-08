fn add_available_tools_part_c(
    tools: &mut Vec<ToolDefinition>,
    config: &AgentConfig,
    agent_data_dir: &std::path::Path,
    has_workspace_topology: bool,
) {
    if config.tools.web_search {
        tools.push(tool_def(
            "web_search",
            "Search the web and return results. Requires a search API key in config.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Search query" },
                    "max_results": { "type": "integer", "description": "Max results (default: 5)" },
                    "timeout_seconds": { "type": "integer", "minimum": 0, "maximum": 600, "description": "Max time to wait for completion (default: 300, max: 600)" }
                },
                "required": ["query"]
            }),
        ));
    }

    if config.tools.web_browse {
        tools.push(tool_def(
            "fetch_url",
            "Browse a URL and return its text content. Uses a headless browser (Lightpanda or Chrome) when available for JS-rendered pages, falls back to raw HTTP.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "URL to fetch" },
                    "max_length": { "type": "integer", "description": "Max characters to return (default: 10000)" },
                    "timeout_seconds": { "type": "integer", "minimum": 0, "maximum": 600, "description": "Max time to wait for completion (default: 300, max: 600)" }
                },
                "required": ["url"]
            }),
        ));
    }

    // Always available — the agent can detect, install, and configure web browsing.
    tools.push(tool_def(
        "setup_web_browsing",
        "Detect, install, and configure a headless browser for web browsing. \
         action=detect: check what browsers are on PATH (always safe). \
         action=install: install Lightpanda via npm (requires approval). \
         action=configure: set the browse_provider in agent config. \
         Call with detect first, then install if needed, then configure.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "enum": ["detect", "install", "configure"],
                    "description": "detect: check available browsers. install: install Lightpanda via npm. configure: set browse_provider."
                },
                "provider": {
                    "type": "string",
                    "enum": ["auto", "lightpanda", "chrome", "none"],
                    "description": "For configure action: which browse_provider to set (default: auto)"
                }
            },
            "required": ["action"]
        }),
    ));

    if config.tools.gateway_messaging {
        for (name, desc, params) in [
            ("send_slack_message", "Send a message to a Slack channel. If channel is omitted, sends to the default channel from gateway settings (slack_channel_filter).", serde_json::json!({
                "type": "object",
                "properties": {
                    "channel": { "type": "string", "description": "Slack channel name or ID. Optional — uses default from config if omitted." },
                    "message": { "type": "string", "description": "Message text" }
                },
                "required": ["message"]
            })),
            ("send_discord_message", "Send a message to a Discord channel or user. If channel_id and user_id are both omitted, sends to the default channel (discord_channel_filter) or default user DM (discord_allowed_users) from gateway settings.", serde_json::json!({
                "type": "object",
                "properties": {
                    "channel_id": { "type": "string", "description": "Discord channel ID. Optional — uses default from config if omitted." },
                    "user_id": { "type": "string", "description": "Discord user ID for DM. Optional." },
                    "message": { "type": "string", "description": "Message text" }
                },
                "required": ["message"]
            })),
            ("send_telegram_message", "Send a message to a Telegram chat. If chat_id is omitted, sends to the default chat from gateway settings (telegram_allowed_chats).", serde_json::json!({
                "type": "object",
                "properties": {
                    "chat_id": { "type": "string", "description": "Telegram chat ID. Optional — uses default from config if omitted." },
                    "message": { "type": "string", "description": "Message text" }
                },
                "required": ["message"]
            })),
            ("send_whatsapp_message", "Send a message to a WhatsApp contact. If phone is omitted, sends to the default contact from gateway settings (whatsapp_allowed_contacts).", serde_json::json!({
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
        "python_execute",
        "Execute Python code directly in a daemon-native subprocess without going through a shell. Use this instead of shell-launched Python when you need Python execution.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "code": { "type": "string", "description": "Python source code to execute" },
                "cwd": { "type": "string", "description": "Optional working directory" },
                "timeout_seconds": { "type": "integer", "minimum": 0, "maximum": 600, "description": "Max time to wait for completion (default: 30, max: 600)" }
            },
            "required": ["code"]
        }),
    ));
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
    tools.push(tool_def("fetch_authenticated_providers", "List the currently authenticated providers that are ready for agent execution, including auth source, configured/default model, and base URL. Use this before setting `spawn_subagent.provider`.", serde_json::json!({
        "type": "object",
        "properties": {}
    })));
    tools.push(tool_def("fetch_provider_models", "Fetch the remotely available models for one authenticated provider using its configured credentials and base URL. Use this before setting `spawn_subagent.model`.", serde_json::json!({
        "type": "object",
        "properties": {
            "provider": { "type": "string", "description": "Authenticated provider ID to inspect, such as `openai` or `github_copilot`." }
        },
        "required": ["provider"]
    })));
    tools.push(tool_def("spawn_subagent", "Spawn a bounded child task under the current task or thread. Use this to split a large task into parallel subagents with dedicated runtime/session metadata. Keep each child narrowly scoped and monitor it with list_subagents. If you want a specific provider/model, call `fetch_authenticated_providers` first and `fetch_provider_models` for the chosen provider before setting the optional override fields.", serde_json::json!({
        "type": "object",
        "properties": {
            "title": { "type": "string", "description": "Short subagent title" },
            "description": { "type": "string", "description": "Detailed instructions for the child task" },
            "runtime": { "type": "string", "enum": ["daemon", "hermes", "openclaw"], "description": "Preferred runtime for the child agent (default: daemon)" },
            "provider": { "type": "string", "description": "Optional authenticated provider override. Use `fetch_authenticated_providers` first." },
            "model": { "type": "string", "description": "Optional model override for the chosen provider. Requires `provider`; use `fetch_provider_models` first." },
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
    tools.push(tool_def("message_agent", &format!("Send a concise private internal DM to another tamux agent and get the reply. This is for behind-the-scenes coordination only: it does not switch the active responder for the current operator thread, and future operator turns do not route to the target agent. If the operator should talk directly to another agent, use `handoff_thread_agent` instead. You can coordinate with {} (concierge), {} (main agent), or any other built-in persona without asking the operator to relay messages.", CONCIERGE_AGENT_NAME, MAIN_AGENT_NAME), serde_json::json!({
        "type": "object",
        "properties": {
            "target": { "type": "string", "description": "Which agent should receive the internal message. Use a built-in agent id or persona name such as `svarog`, `rarog`, or `weles`." },
            "message": { "type": "string", "description": "Message to send" }
        },
        "required": ["target", "message"]
    })));
    tools.push(tool_def("handoff_thread_agent", "Switch the active responder for the current thread. Use this when the operator wants to talk directly to another agent persona or when another agent should own future replies. push_handoff moves responsibility to another agent with a structured summary; return_handoff returns responsibility to the previous responder on the thread handoff stack. Agent-initiated push handoffs require approval outside yolo mode.", serde_json::json!({
        "type": "object",
        "properties": {
            "action": { "type": "string", "enum": ["push_handoff", "return_handoff"], "description": "push_handoff moves the thread to another agent; return_handoff pops back to the previous responder." },
            "target_agent_id": { "type": "string", "description": "Required for push_handoff. Agent id or persona name that should take over the thread." },
            "reason": { "type": "string", "description": "Why the handoff is happening." },
            "summary": { "type": "string", "description": "Compact summary the receiving agent should use to continue." },
            "requested_by": { "type": "string", "enum": ["user", "agent"], "description": "Whether this handoff reflects an operator request or the current agent's own judgment." }
        },
        "required": ["action", "reason", "summary", "requested_by"]
    })));
    tools.push(tool_def("route_to_specialist", "Route a task to a specialist subagent with structured handoff. The broker matches capability tags to specialist profiles, assembles a context bundle with episodic refs and negative constraints, and dispatches the work.", serde_json::json!({
        "type": "object",
        "properties": {
            "task_description": { "type": "string", "description": "Detailed description of the work to hand off to a specialist" },
            "capability_tags": { "type": "array", "items": { "type": "string" }, "description": "Required capability tags for specialist matching (e.g., [\"rust\", \"backend\", \"api-design\"])" },
            "acceptance_criteria": { "type": "string", "description": "Structural checks for output validation (e.g., \"non_empty\", \"min_length:100\"). Defaults to \"non_empty\"." }
        },
        "required": ["task_description", "capability_tags"]
    })));
    tools.push(tool_def("run_divergent", "Start a divergent session: spawn 2-3 parallel framings of the same problem with different perspectives, detect disagreements, and surface tensions as the valuable output. Returns a session ID and framing labels. Use this when a problem benefits from multiple viewpoints (e.g., architectural decisions, tradeoff analysis).", serde_json::json!({
        "type": "object",
        "properties": {
            "problem_statement": { "type": "string", "description": "The problem to analyze from multiple perspectives" },
            "custom_framings": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "label": { "type": "string", "description": "Short name for this perspective (e.g., 'performance-lens')" },
                        "system_prompt_override": { "type": "string", "description": "System prompt directing this framing's perspective" }
                    },
                    "required": ["label", "system_prompt_override"]
                },
                "description": "Optional custom framings (2-3). If omitted, default analytical + pragmatic lenses are used."
            }
        },
        "required": ["problem_statement"]
    })));
    tools.push(tool_def("get_divergent_session", "Fetch divergent session status and output payload (framing progress, tensions markdown, mediator prompt, optional mediation result). Use this after run_divergent to retrieve completion artifacts.", serde_json::json!({
        "type": "object",
        "properties": {
            "session_id": { "type": "string", "description": "Divergent session ID returned by run_divergent" }
        },
        "required": ["session_id"]
    })));
}
