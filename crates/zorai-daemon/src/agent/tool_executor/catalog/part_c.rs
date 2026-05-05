fn configured_image_generation_setting<'a>(
    config: &'a AgentConfig,
    field: &str,
) -> Option<&'a str> {
    config
        .extra
        .get("image")
        .and_then(|value| value.get("generation"))
        .and_then(|value| value.get(field))
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| {
            let legacy_key = format!("image_generation_{field}");
            config
                .extra
                .get(&legacy_key)
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
        })
}

fn configured_image_generation_provider(config: &AgentConfig) -> Option<&str> {
    configured_image_generation_setting(config, "provider")
}

fn configured_image_generation_model(config: &AgentConfig) -> Option<&str> {
    configured_image_generation_setting(config, "model")
}

fn add_available_tools_part_c(
    tools: &mut Vec<ToolDefinition>,
    config: &AgentConfig,
    _agent_data_dir: &std::path::Path,
    _has_workspace_topology: bool,
) {
    if config.tools.web_search {
        tools.push(tool_def(
            tool_names::WEB_SEARCH,
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
            tool_names::FETCH_URL,
            "Browse a URL and return its text content for browser-readable pages or APIs that return text/JSON. Uses a headless browser (Lightpanda or Chrome) when available for JS-rendered pages, falls back to raw HTTP.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "url": { "type": "string", "description": "URL to fetch" },
                    "max_length": { "type": "integer", "description": "Max characters to return (default: 10000)" },
                    "timeout_seconds": { "type": "integer", "minimum": 0, "maximum": 600, "description": "Max time to wait for completion (default: 300, max: 600)" },
                    "profile_id": { "type": "string", "description": "Optional named browser profile for authenticated browsing reuse" }
                },
                "required": ["url"]
            }),
        ));
    }

    // Always available — the agent can detect, install, and configure web browsing.
    tools.push(tool_def(
        tool_names::SETUP_WEB_BROWSING,
        "Detect, install, and configure a headless browser for web browsing. \
         action=detect: check what browsers are on PATH (always safe). \
         action=install: install Lightpanda via npm (requires approval); Chrome/Chromium must be installed manually. \
         action=configure: set the browse_provider in agent config, and require Chrome/Chromium to already be on PATH when provider=chrome. \
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
            (tool_names::SEND_SLACK_MESSAGE, "Send a message to a Slack channel. If channel is omitted, sends to the default channel from gateway settings (slack_channel_filter).", serde_json::json!({
                "type": "object",
                "properties": {
                    "channel": { "type": "string", "description": "Slack channel name or ID. Optional — uses default from config if omitted." },
                    "message": { "type": "string", "description": "Message text" }
                },
                "required": ["message"]
            })),
            (tool_names::SEND_DISCORD_MESSAGE, "Send a message to a Discord channel or user. If channel_id and user_id are both omitted, sends to the default channel (discord_channel_filter) or default user DM (discord_allowed_users) from gateway settings.", serde_json::json!({
                "type": "object",
                "properties": {
                    "channel_id": { "type": "string", "description": "Discord channel ID. Optional — uses default from config if omitted." },
                    "user_id": { "type": "string", "description": "Discord user ID for DM. Optional." },
                    "message": { "type": "string", "description": "Message text" }
                },
                "required": ["message"]
            })),
            (tool_names::SEND_TELEGRAM_MESSAGE, "Send a message to a Telegram chat. If chat_id is omitted, sends to the default chat from gateway settings (telegram_allowed_chats).", serde_json::json!({
                "type": "object",
                "properties": {
                    "chat_id": { "type": "string", "description": "Telegram chat ID. Optional — uses default from config if omitted." },
                    "message": { "type": "string", "description": "Message text" }
                },
                "required": ["message"]
            })),
            (tool_names::SEND_WHATSAPP_MESSAGE, "Send a message to a WhatsApp contact. If phone is omitted, sends to the default contact from gateway settings (whatsapp_allowed_contacts).", serde_json::json!({
                "type": "object",
                "properties": {
                    "phone": { "type": "string", "description": "Phone in E.164 format or WhatsApp JID. Optional — uses default from config if omitted." },
                    "message": { "type": "string", "description": "Message text" }
                },
                "required": ["message"]
            })),
            (tool_names::WHATSAPP_LINK_START, "Start the WhatsApp link runtime and begin pairing or reconnect flow.", serde_json::json!({
                "type": "object",
                "properties": {}
            })),
            (tool_names::WHATSAPP_LINK_STOP, "Stop the WhatsApp link runtime and disconnect the current session.", serde_json::json!({
                "type": "object",
                "properties": {}
            })),
            (tool_names::WHATSAPP_LINK_RESET, "Reset the WhatsApp link runtime and clear persisted session state.", serde_json::json!({
                "type": "object",
                "properties": {}
            })),
            (tool_names::WHATSAPP_LINK_STATUS, "Return the current WhatsApp link runtime status, including linked phone and last error when present.", serde_json::json!({
                "type": "object",
                "properties": {}
            })),
        ] {
            tools.push(tool_def(name, desc, params));
        }
    }

    // Terminal pane tools
    tools.push(tool_def(
        tool_names::PYTHON_EXECUTE,
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

    let active_model_features = zorai_shared::providers::derive_model_feature_capabilities(
        &config.provider,
        &config.model,
        None,
        false,
    );
    let image_generation_enabled =
        configured_image_generation_model(config).is_some() || active_model_features.image_generation;
    let active_model_supports_image = crate::agent::types::model_supports(
        &config.provider,
        &config.model,
        crate::agent::types::Modality::Image,
    );
    let image_analysis_enabled =
        config.tools.vision || active_model_features.vision || active_model_supports_image;

    if image_analysis_enabled {
        tools.push(tool_def(
            tool_names::ANALYZE_IMAGE,
            "Analyze an image with the active or specified multimodal model. Accepts exactly one of `path`, `url`, `base64`, or `data_url`, then returns a textual analysis.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Local image path to analyze" },
                    "url": { "type": "string", "description": "Remote image URL to analyze" },
                    "base64": { "type": "string", "description": "Base64-encoded image bytes. Requires `mime_type`." },
                    "data_url": { "type": "string", "description": "Base64 data URL for the image" },
                    "mime_type": { "type": "string", "description": "Override MIME type when using `path` or `base64`" },
                    "prompt": { "type": "string", "description": "Optional analysis instruction. Defaults to a general detailed analysis." },
                    "provider": { "type": "string", "description": "Optional provider override, only when the operator explicitly specifies it" },
                    "model": { "type": "string", "description": "Optional model override, only when the operator explicitly specifies it" },
                    "timeout_seconds": { "type": "integer", "minimum": 0, "maximum": 600, "description": "Max time to wait for completion (default: 600, max: 600)" },
                    "include_reasoning": { "type": "boolean", "description": "Include model reasoning summary when available" },
                    "include_provider_result": { "type": "boolean", "description": "Append the raw structured provider final result when available" }
                }
            }),
        ));

    }

    if image_generation_enabled {
        tools.push(tool_def(
            tool_names::GENERATE_IMAGE,
            "Generate an image through an OpenAI-compatible image generation endpoint and return JSON with the saved artifact path or upstream URL.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "prompt": { "type": "string", "description": "Image generation prompt" },
                    "provider": { "type": "string", "description": "Optional provider override, only when the operator explicitly specifies it" },
                    "model": { "type": "string", "description": "Optional model override, only when the operator explicitly specifies it" },
                    "size": { "type": "string", "description": "Optional output size such as 1024x1024" },
                    "quality": { "type": "string", "description": "Optional quality hint supported by the provider" },
                    "style": { "type": "string", "description": "Optional style hint supported by the provider" },
                    "background": { "type": "string", "description": "Optional background hint supported by the provider" },
                    "output_format": { "type": "string", "description": "Desired image format for saved bytes, e.g. png, jpg, webp" },
                    "timeout_seconds": { "type": "integer", "minimum": 0, "maximum": 600, "description": "Max time to wait for completion (default: 600, max: 600)" }
                },
                "required": ["prompt"]
            }),
        ));
    }

    tools.push(tool_def(
        tool_names::SPEECH_TO_TEXT,
        "Transcribe an audio file through an OpenAI-compatible transcription endpoint and return the recognized text or provider JSON.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "path": { "type": "string", "description": "Local audio file path to transcribe" },
                "mime_type": { "type": "string", "description": "Optional MIME type override for the uploaded audio file" },
                "provider": { "type": "string", "description": "Optional provider override, only when the operator explicitly specifies it" },
                "model": { "type": "string", "description": "Optional model override, only when the operator explicitly specifies it" },
                "language": { "type": "string", "description": "Optional language hint for the transcription model" },
                "prompt": { "type": "string", "description": "Optional transcription prompt or vocabulary hint" },
                "response_format": { "type": "string", "description": "Optional transcription response format such as json, verbose_json, srt, or text" },
                "timeout_seconds": { "type": "integer", "minimum": 0, "maximum": 600, "description": "Max time to wait for completion (default: 600, max: 600)" }
            },
            "required": ["path"]
        }),
    ));

    tools.push(tool_def(
        tool_names::TEXT_TO_SPEECH,
        "Synthesize speech through an OpenAI-compatible speech endpoint and return JSON with the saved audio artifact path. Use this when the operator asks you to say something aloud or read text out loud. After success, do not send a follow-up message that only repeats the temporary file path.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "input": { "type": "string", "description": "Text to synthesize into speech" },
                "provider": { "type": "string", "description": "Optional provider override, only when the operator explicitly specifies it" },
                "model": { "type": "string", "description": "Optional model override, only when the operator explicitly specifies it" },
                "voice": { "type": "string", "description": "Voice identifier supported by the provider" },
                "response_format": { "type": "string", "description": "Desired output format such as mp3, wav, ogg, flac, or m4a" },
                "timeout_seconds": { "type": "integer", "minimum": 0, "maximum": 600, "description": "Max time to wait for completion (default: 600, max: 600)" }
            },
            "required": ["input"]
        }),
    ));

    tools.push(tool_def(
        tool_names::LIST_TERMINALS,
        "List all open terminal panes with their IDs and names.",
        serde_json::json!({"type":"object","properties":{}}),
    ));
    tools.push(tool_def(tool_names::READ_ACTIVE_TERMINAL_CONTENT, "Read the current terminal buffer content from a pane, or browser panel info. For browser panels, returns URL and title; use include_dom to get page text content.", serde_json::json!({
        "type": "object",
        "properties": {
            "pane": { "type": "string", "description": "Pane ID or name (optional, defaults to active)" },
            "include_dom": { "type": "boolean", "description": "For browser panels: include page DOM text content. Ignored for terminal panes." }
        }
    })));
    tools.push(tool_def(tool_names::RUN_TERMINAL_COMMAND, "Execute a shell command through a zorai-managed terminal session. This runs in the app's terminal context (not a daemon-native subprocess). For long-running work, prefer non-blocking execution; background operations will auto-notify the thread when they complete, and `get_operation_status` is available when you need more details. Use this for shell-native networking such as `curl -I`, range requests, large or binary downloads, or streaming transfers; for browser-readable text pages, prefer `web_search` or `fetch_url`.", serde_json::json!({
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
            "timeout_seconds": { "type": "integer", "description": "Wait timeout when wait_for_completion=true (default: 30, max: 600). Above 600 the command auto-backgrounds, returns an `operation_id`, and will auto-notify the thread on completion." }
        },
        "required": ["command"]
    })));
    tools.push(tool_def(tool_names::EXECUTE_MANAGED_COMMAND, "Queue a command in a daemon-managed terminal lane. By default this tool waits for completion and returns final status/output tail. If session is omitted, uses the first active terminal session. For non-blocking execution, background operations will auto-notify the thread when they complete, and `get_operation_status` is available when you need more details. Use this for shell-native networking such as `curl -I`, range requests, large or binary downloads, or streaming transfers; for browser-readable text pages, prefer `web_search` or `fetch_url`.", serde_json::json!({
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
            "timeout_seconds": { "type": "integer", "description": "Wait timeout when wait_for_completion=true (default: 30, max: 600). Above 600 the command auto-backgrounds, returns an `operation_id`, and will auto-notify the thread on completion." }
        },
        "required": ["command", "rationale"]
    })));
    tools.push(tool_def(tool_names::GET_OPERATION_STATUS, "Look up the current lifecycle state of a previously accepted asynchronous operation by its operation_id. Background operations will auto-notify the thread with their completion status and result, so use this tool when you need more details or an explicit status check. For background terminal commands, pass the returned operation_id here; `background_task_id` is the same value for compatibility. When a background headless shell command completes or fails, this response includes `terminal_result` with the captured payload and exit code.", serde_json::json!({
        "type": "object",
        "properties": {
            "operation_id": { "type": "string", "description": "Asynchronous operation handle returned by a non-blocking tool or daemon operation" }
        },
        "required": ["operation_id"]
    })));
    tools.push(tool_def(tool_names::GET_BACKGROUND_TASK_STATUS, "Compatibility alias for background managed-terminal commands. Prefer `get_operation_status`; this tool accepts the same ID under the older `background_task_id` name.", serde_json::json!({
        "type": "object",
        "properties": {
            "background_task_id": { "type": "string", "description": "Execution handle returned as background_task_id by a non-blocking managed terminal command" }
        },
        "required": ["background_task_id"]
    })));
    tools.push(tool_def(tool_names::ALLOCATE_TERMINAL, "Allocate another daemon-managed terminal lane in the same workspace as the current session. Use this when your chosen session is occupied by a blocking or long-running command and you need another terminal to continue working.", serde_json::json!({
        "type": "object",
        "properties": {
            "session": { "type": "string", "description": "Optional source session ID or unique substring. Defaults to the preferred/current session." },
            "pane_name": { "type": "string", "description": "Optional name for the new terminal pane" },
            "cwd": { "type": "string", "description": "Optional working directory hint to show in the workspace metadata" }
        }
    })));
    tools.push(tool_def(tool_names::FETCH_AUTHENTICATED_PROVIDERS, "List the currently authenticated providers that are ready for agent execution, including auth source, configured/default model, and base URL. Legacy alias for `list_providers`.", serde_json::json!({
        "type": "object",
        "properties": {}
    })));
    tools.push(tool_def(tool_names::LIST_PROVIDERS, "List configured providers with authentication state, auth source, configured/default model, and base URL. Use this before selecting a provider for subagents or model switching.", serde_json::json!({
        "type": "object",
        "properties": {}
    })));
    tools.push(tool_def(tool_names::FETCH_PROVIDER_MODELS, "Fetch the remotely available models for one authenticated provider using its configured credentials and base URL. Legacy alias for `list_models`.", serde_json::json!({
        "type": "object",
        "properties": {
            "provider": { "type": "string", "description": "Authenticated provider ID to inspect, such as `openai` or `github_copilot`." }
        },
        "required": ["provider"]
    })));
    tools.push(tool_def(tool_names::LIST_MODELS, "List the remotely available models for one authenticated provider using its configured credentials and base URL. Use this before setting a model override or calling `switch_model`.", serde_json::json!({
        "type": "object",
        "properties": {
            "provider": { "type": "string", "description": "Authenticated provider ID to inspect, such as `openai` or `github_copilot`." }
        },
        "required": ["provider"]
    })));
    tools.push(tool_def(tool_names::LIST_AGENTS, "List agent runtime targets and the provider/model each one currently uses as its LLM access point. This does not report spawned child-task progress; use `list_subagents` for that.", serde_json::json!({
        "type": "object",
        "properties": {}
    })));
    tools.push(tool_def(tool_names::LIST_PARTICIPANTS, "List the current thread participants, including active/inactive status and instructions. On participant-managed threads, use this instead of `list_agents` when choosing who can own the thread.", serde_json::json!({
        "type": "object",
        "properties": {}
    })));
    if current_agent_scope_id() == MAIN_AGENT_ID {
        tools.push(tool_def(tool_names::SWITCH_MODEL, "Update which provider and model a target agent uses as its LLM access point. This writes the same persisted settings the Settings UI edits. Only svarog can call this tool.", serde_json::json!({
            "type": "object",
            "properties": {
                "agent": { "type": "string", "description": "Target agent id or name, such as `svarog`, `rarog`, `weles`, or a user subagent id from `list_agents`." },
                "provider": { "type": "string", "description": "Provider id to assign to the target agent." },
                "model": { "type": "string", "description": "Model id to assign to the target agent." }
            },
            "required": ["agent", "provider", "model"]
        })));
    }
    tools.push(tool_def(tool_names::SPAWN_SUBAGENT, "Spawn a bounded child task under the current task or thread. Use this to split a large task into parallel subagents with dedicated runtime/session metadata. Keep each child narrowly scoped and monitor it with list_subagents. Do not busy-wait after spawning; if nothing else is actionable, stop and let zorai resume you when children finish. If you want a specific provider/model, call `list_providers` first and `list_models` for the chosen provider before setting the optional override fields.", serde_json::json!({
        "type": "object",
        "properties": {
            "title": { "type": "string", "description": "Short subagent title" },
            "description": { "type": "string", "description": "Detailed instructions for the child task" },
            "runtime": { "type": "string", "enum": ["daemon", "hermes", "openclaw"], "description": "Preferred runtime for the child agent (default: daemon)" },
            "provider": { "type": "string", "description": "Optional authenticated provider override. Use `list_providers` first." },
            "model": { "type": "string", "description": "Optional model override for the chosen provider. Requires `provider`; use `list_models` first." },
            "reasoning_effort": { "type": "string", "description": "Optional reasoning-effort override for the spawned child, such as `off`, `low`, `medium`, or `high` when supported by the selected provider/model." },
            "priority": { "type": "string", "enum": ["low", "normal", "high", "urgent"], "description": "Child task priority" },
            "command": { "type": "string", "description": "Optional preferred entrypoint or command" },
            "session": { "type": "string", "description": "Optional explicit session ID or unique substring. If omitted, zorai allocates a fresh lane in the same workspace when possible." },
            "cwd": { "type": "string", "description": "Optional working directory hint for any newly allocated lane" },
            "dependencies": { "type": "array", "items": { "type": "string" }, "description": "Optional additional task dependencies" },
            "max_depth": { "type": "integer", "description": "Optional maximum recursive delegation depth for this child subtree. Default: 1 (flat delegation). Hard cap: 3." },
            "budget": {
                "type": "object",
                "properties": {
                    "max_tokens": { "type": "integer", "description": "Optional explicit context token budget for the spawned child" },
                    "max_wall_time_secs": { "type": "integer", "description": "Optional explicit wall-clock time budget in seconds" },
                    "max_tool_calls": { "type": "integer", "description": "Optional explicit tool-call budget; enforced via termination conditions" }
                },
                "description": "Optional explicit child budget. When omitted, zorai derives a stricter budget from delegation depth."
            }
        },
        "required": ["title", "description"]
    })));
    tools.push(tool_def(tool_names::LIST_SUBAGENTS, "List child tasks spawned under the current parent task or thread, including runtime, status, thread/session metadata, delegation depth, and remaining budget info when available.", serde_json::json!({
        "type": "object",
        "properties": {
            "status": { "type": "string", "enum": ["queued", "in_progress", "awaiting_approval", "blocked", "failed_analyzing", "budget_exceeded", "completed", "failed", "cancelled"], "description": "Optional status filter" },
            "parent_task_id": { "type": "string", "description": "Override parent task scope" },
            "parent_thread_id": { "type": "string", "description": "Override parent thread scope" },
            "limit": { "type": "integer", "description": "Maximum subagents to return (default: 20)" }
        }
    })));
    tools.push(tool_def(tool_names::MESSAGE_AGENT, &format!("Send a concise private internal DM to another zorai agent and get the reply. This is for behind-the-scenes coordination only: it does not switch the active responder for the current operator thread, and future operator turns do not route to the target agent. If the target is an active participant on the current visible operator thread and `request_visible_thread_continuation` is omitted, zorai treats the message as a request for that participant to continue visibly on the thread. If the operator should talk directly to another agent, use `handoff_thread_agent` instead. You can coordinate with {} (concierge), {} (main agent), or any other built-in persona without asking the operator to relay messages.", CONCIERGE_AGENT_NAME, MAIN_AGENT_NAME), serde_json::json!({
        "type": "object",
        "properties": {
            "target": { "type": "string", "description": "Which agent should receive the internal message. Use a built-in agent id or persona name such as `svarog`, `rarog`, or `weles`." },
            "message": { "type": "string", "description": "Message to send" },
            "request_visible_thread_continuation": { "type": "boolean", "description": "When true, the internal DM stays discussion-only and the target agent is asked to continue the current visible operator thread after this turn finishes. When omitted while targeting an active participant on a visible operator thread, this defaults to true." }
        },
        "required": ["target", "message"]
    })));
    tools.push(tool_def(tool_names::HANDOFF_THREAD_AGENT, "Switch the active responder for the current thread. Use this when the operator wants to talk directly to another agent persona or when another agent should own future replies. push_handoff moves responsibility to another agent with a structured summary; return_handoff returns responsibility to the previous responder on the thread handoff stack. On participant-managed threads, push_handoff may target only active thread participants. Agent-initiated push handoffs require approval outside yolo mode.", serde_json::json!({
        "type": "object",
        "properties": {
            "action": { "type": "string", "enum": ["push_handoff", "return_handoff"], "description": "push_handoff moves the thread to another agent; return_handoff pops back to the previous responder." },
            "target_agent_id": { "type": "string", "description": "Required for push_handoff. Agent id or persona name that should take over the thread. On participant-managed threads, this must be an active participant from `list_participants`." },
            "reason": { "type": "string", "description": "Why the handoff is happening." },
            "summary": { "type": "string", "description": "Compact summary the receiving agent should use to continue." },
            "requested_by": { "type": "string", "enum": ["user", "agent"], "description": "Whether this handoff reflects an operator request or the current agent's own judgment." }
        },
        "required": ["action", "reason", "summary", "requested_by"]
    })));
    tools.push(tool_def(tool_names::ROUTE_TO_SPECIALIST, "Route a task to a specialist subagent with structured handoff. The broker matches capability tags to specialist profiles, assembles a context bundle with episodic refs and negative constraints, and dispatches the work.", serde_json::json!({
        "type": "object",
        "properties": {
            "task_description": { "type": "string", "description": "Detailed description of the work to hand off to a specialist" },
            "capability_tags": { "type": "array", "items": { "type": "string" }, "description": "Required capability tags for specialist matching (e.g., [\"rust\", \"backend\", \"api-design\"])" },
            "acceptance_criteria": { "type": "string", "description": "Structural checks for output validation (e.g., \"non_empty\", \"min_length:100\"). Defaults to \"non_empty\"." }
        },
        "required": ["task_description", "capability_tags"]
    })));
    tools.push(tool_def(tool_names::RUN_DIVERGENT, "Start a divergent session: spawn 2-3 parallel framings of the same problem with different perspectives, detect disagreements, and surface tensions as the valuable output. Returns a session ID and framing labels. Use this when a problem benefits from multiple viewpoints (e.g., architectural decisions, tradeoff analysis).", serde_json::json!({
        "type": "object",
        "properties": {
            "problem_statement": { "type": "string", "description": "The problem to analyze from multiple perspectives" },
            "mode": { "type": "string", "enum": ["divergent", "debate"], "description": "Execution mode. `divergent` (default) runs tension-mapping. `debate` runs the multi-round debate protocol on the same statement." },
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
    tools.push(tool_def(tool_names::GET_DIVERGENT_SESSION, "Fetch divergent session status and output payload (framing progress, tensions markdown, mediator prompt, optional mediation result). Use this after run_divergent to retrieve completion artifacts.", serde_json::json!({
        "type": "object",
        "properties": {
            "session_id": { "type": "string", "description": "Divergent session ID returned by run_divergent" }
        },
        "required": ["session_id"]
    })));
    tools.push(tool_def(tool_names::RUN_DEBATE, "Start a structured debate session with 2-3 framings and rotating roles. Returns a session ID for follow-up retrieval and lifecycle actions.", serde_json::json!({
        "type": "object",
        "properties": {
            "topic": { "type": "string", "description": "The topic to debate" },
            "custom_framings": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "label": { "type": "string", "description": "Short name for this perspective" },
                        "system_prompt_override": { "type": "string", "description": "System prompt directing this framing's perspective" }
                    },
                    "required": ["label", "system_prompt_override"]
                },
                "description": "Optional custom framings (2-3). If omitted, default analytical + pragmatic lenses are used."
            }
        },
        "required": ["topic"]
    })));
    tools.push(tool_def(tool_names::GET_DEBATE_SESSION, "Fetch debate session status and payload, including roles, arguments, and verdict when available.", serde_json::json!({
        "type": "object",
        "properties": {
            "session_id": { "type": "string", "description": "Debate session ID returned by run_debate" }
        },
        "required": ["session_id"]
    })));
    tools.push(tool_def(tool_names::APPEND_DEBATE_ARGUMENT, "Append one structured argument to a debate session for the current round.", serde_json::json!({
        "type": "object",
        "properties": {
            "session_id": { "type": "string", "description": "Debate session ID" },
            "role": { "type": "string", "enum": ["proponent", "skeptic", "synthesizer"], "description": "Debate role for this argument" },
            "agent_id": { "type": "string", "description": "Framing/agent label submitting the argument" },
            "content": { "type": "string", "description": "Argument content" },
            "evidence_refs": { "type": "array", "items": { "type": "string" }, "description": "Evidence references supporting the argument" },
            "responds_to": { "type": "string", "description": "Optional prior argument ID this argument responds to" }
        },
        "required": ["session_id", "role", "agent_id", "content"]
    })));
    tools.push(tool_def(
        tool_names::ADVANCE_DEBATE_ROUND,
        "Advance a debate session to the next round and rotate roles when configured.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "session_id": { "type": "string", "description": "Debate session ID" }
            },
            "required": ["session_id"]
        }),
    ));
    tools.push(tool_def(
        tool_names::COMPLETE_DEBATE_SESSION,
        "Finalize a debate session and synthesize a verdict from the accumulated arguments.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "session_id": { "type": "string", "description": "Debate session ID" }
            },
            "required": ["session_id"]
        }),
    ));
    tools.push(tool_def(tool_names::GET_CRITIQUE_SESSION, "Fetch an auto-generated critique preflight session, including advocate/critic arguments and the arbiter resolution.", serde_json::json!({
        "type": "object",
        "properties": {
            "session_id": { "type": "string", "description": "Critique session ID returned in critique preflight notices or blocking messages" }
        },
        "required": ["session_id"]
    })));
    tools.push(tool_def(tool_names::LOOKUP_EMERGENT_PROTOCOL, "Look up an accepted emergent protocol registry entry for the current thread by token and optionally record a usage or fallback outcome.", serde_json::json!({
        "type": "object",
        "properties": {
            "token": { "type": "string", "description": "Protocol token to resolve, such as @proto_deadbeef" },
            "record_usage": { "type": "boolean", "description": "When true, record usage/fallback against the registry entry" },
            "success": { "type": "boolean", "description": "Usage outcome when record_usage=true" },
            "fallback_reason": { "type": "string", "description": "Fallback reason when lookup succeeded but the protocol could not be applied cleanly" },
            "execution_time_ms": { "type": "integer", "description": "Optional execution time for the attempted protocol application" }
        },
        "required": ["token"]
    })));
    tools.push(tool_def(tool_names::LIST_EMERGENT_PROTOCOL_PROPOSALS, "List pending emergent protocol proposals for the current thread that require explicit acceptance or rejection before activation.", serde_json::json!({
        "type": "object",
        "properties": {}
    })));
    tools.push(tool_def(tool_names::RESPOND_EMERGENT_PROTOCOL_PROPOSAL, "Accept or reject a pending emergent protocol proposal in the current thread. Accepted proposals activate into the registry; rejected proposals remain suppressed.", serde_json::json!({
        "type": "object",
        "properties": {
            "candidate_id": { "type": "string", "description": "Pending protocol candidate ID to respond to" },
            "accept": { "type": "boolean", "description": "True to accept and activate the proposal; false to reject it" }
        },
        "required": ["candidate_id", "accept"]
    })));
    tools.push(tool_def(tool_names::RELOAD_EMERGENT_PROTOCOL_REGISTRY, "Reload and return accepted emergent protocol registry entries for the current thread from durable storage.", serde_json::json!({
        "type": "object",
        "properties": {}
    })));
    tools.push(tool_def(tool_names::DECODE_EMERGENT_PROTOCOL, "Resolve an accepted emergent protocol token in the current thread, validate the stored context signature, and return either a structured fallback or expanded intent payload. This does not execute the decoded steps automatically.", serde_json::json!({
        "type": "object",
        "properties": {
            "token": { "type": "string", "description": "Protocol token to decode, such as @proto_deadbeef" },
            "current_role": { "type": "string", "description": "Current sender role for context validation (for example: user or assistant)" },
            "target_role": { "type": "string", "description": "Expected receiver role for context validation (for example: assistant or user)" },
            "normalized_pattern": { "type": "string", "description": "Observed normalized pattern in the current context to validate against the stored signature" }
        },
        "required": ["token"]
    })));
    tools.push(tool_def(
        tool_names::GET_EMERGENT_PROTOCOL_USAGE_LOG,
        "Fetch recorded usage/fallback entries for an accepted emergent protocol.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "protocol_id": { "type": "string", "description": "Accepted emergent protocol ID" }
            },
            "required": ["protocol_id"]
        }),
    ));
}
