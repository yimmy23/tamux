fn add_available_tools_part_d(
    tools: &mut Vec<ToolDefinition>,
    config: &AgentConfig,
    agent_data_dir: &std::path::Path,
    _has_workspace_topology: bool,
) {
    if config.collaboration.enabled {
        tools.push(tool_def("broadcast_contribution", "Publish a structured subagent contribution into the shared collaboration session for the current parent task.", serde_json::json!({
            "type": "object",
            "properties": {
                "parent_task_id": { "type": "string", "description": "Optional explicit parent task scope for parent/operator-originated contributions" },
                "topic": { "type": "string", "description": "Short topic under discussion" },
                "position": { "type": "string", "description": "Your current stance or recommendation" },
                "evidence": { "type": "array", "items": { "type": "string" }, "description": "Supporting evidence bullets" },
                "confidence": { "type": "number", "description": "Confidence in the range 0.0-1.0" }
            },
            "required": ["topic", "position"]
        })));
        tools.push(tool_def("read_peer_memory", "Read sibling subagent contributions, shared context, disagreements, and consensus for the current parent task.", serde_json::json!({
            "type": "object",
            "properties": {
                "parent_task_id": { "type": "string", "description": "Optional explicit parent task scope" }
            }
        })));
        tools.push(tool_def("vote_on_disagreement", "Cast a weighted vote on a live subagent disagreement for the current collaboration session.", serde_json::json!({
            "type": "object",
            "properties": {
                "disagreement_id": { "type": "string", "description": "Disagreement ID from read_peer_memory or list_collaboration_sessions" },
                "position": { "type": "string", "description": "Position you vote for" },
                "confidence": { "type": "number", "description": "Optional explicit confidence override in the range 0.0-1.0" }
            },
            "required": ["disagreement_id", "position"]
        })));
        tools.push(tool_def("dispatch_via_bid_protocol", "Dispatch a collaboration task through the minimal bid protocol and return the resolved primary/reviewer assignment.", serde_json::json!({
            "type": "object",
            "properties": {
                "parent_task_id": { "type": "string", "description": "Parent collaboration task scope" },
                "bids": {
                    "type": "array",
                    "description": "Typed bid submissions to resolve",
                    "items": {
                        "type": "object",
                        "properties": {
                            "task_id": { "type": "string", "description": "Collaborative task submitting the bid" },
                            "confidence": { "type": "number", "description": "Bid confidence in the range 0.0-1.0" },
                            "availability": { "type": "string", "enum": ["available", "busy", "unavailable"], "description": "Availability state for ranking" }
                        },
                        "required": ["task_id", "confidence", "availability"]
                    }
                }
            },
            "required": ["parent_task_id", "bids"]
        })));
        tools.push(tool_def("list_collaboration_sessions", "Inspect live collaboration sessions, contributions, disagreements, and consensus built from subagent work.", serde_json::json!({
            "type": "object",
            "properties": {
                "parent_task_id": { "type": "string", "description": "Optional parent task scope" }
            }
        })));
    }
    tools.push(tool_def("list_threads", "List existing agent threads as lightweight summaries with optional deterministic filters.", serde_json::json!({
        "type": "object",
        "properties": {
            "created_after": { "type": "integer", "minimum": 0, "description": "Include threads created at or after this Unix timestamp in milliseconds" },
            "created_before": { "type": "integer", "minimum": 0, "description": "Include threads created at or before this Unix timestamp in milliseconds" },
            "updated_after": { "type": "integer", "minimum": 0, "description": "Include threads updated at or after this Unix timestamp in milliseconds" },
            "updated_before": { "type": "integer", "minimum": 0, "description": "Include threads updated at or before this Unix timestamp in milliseconds" },
            "agent_name": { "type": "string", "description": "Optional canonical or alias agent filter (case-insensitive)" },
            "title_query": { "type": "string", "description": "Optional case-insensitive substring match against the thread title" },
            "pinned": { "type": "boolean", "description": "Optional pinned-state filter" },
            "include_internal": { "type": "boolean", "description": "Include otherwise hidden WELES and handoff threads when true" },
            "limit": { "type": "integer", "minimum": 0, "description": "Optional maximum number of matching thread summaries to return" },
            "offset": { "type": "integer", "minimum": 0, "description": "Optional number of matching thread summaries to skip before returning results" }
        }
    })));
    tools.push(tool_def("get_thread", "Fetch one agent thread and a paged slice of its messages by thread ID, with optional internal-thread access.", serde_json::json!({
        "type": "object",
        "properties": {
            "thread_id": { "type": "string", "description": "Thread ID to fetch" },
            "limit": { "type": "integer", "minimum": 0, "description": "Optional maximum number of messages to return from the most recent end of the thread. Defaults to 5." },
            "offset": { "type": "integer", "minimum": 0, "description": "Optional number of newest messages to skip before applying the limit. Defaults to 0." },
            "include_internal": { "type": "boolean", "description": "Allow access to otherwise hidden WELES and handoff threads when true" }
        },
        "required": ["thread_id"]
    })));
    tools.push(tool_def("read_offloaded_payload", "Read an offloaded tool-result payload by payload ID and return the exact raw text content.", serde_json::json!({
        "type": "object",
        "properties": {
            "payload_id": { "type": "string", "description": "Payload ID from an offloaded tool-result thread message" }
        },
        "required": ["payload_id"]
    })));
    tools.push(tool_def("enqueue_task", "Create a daemon-managed background task. Use this for work that should run later, survive disconnects, wait on dependencies, or schedule follow-up actions like reminders and gateway messages.", serde_json::json!({
        "type": "object",
        "properties": {
            "title": { "type": "string", "description": "Short task title" },
            "description": { "type": "string", "description": "Detailed task instructions for the daemon agent" },
            "priority": { "type": "string", "enum": ["low", "normal", "high", "urgent"], "description": "Task priority" },
            "command": { "type": "string", "description": "Optional preferred command or entrypoint" },
            "session": { "type": "string", "description": "Optional preferred terminal session ID or substring" },
            "dependencies": { "type": "array", "items": { "type": "string" }, "description": "Task IDs that must complete first" },
            "scheduled_at": { "type": "integer", "description": "Optional Unix timestamp in milliseconds for when the task may start" },
            "schedule_at": { "type": "string", "description": "Optional RFC3339 timestamp for when the task may start" },
            "delay_seconds": { "type": "integer", "description": "Optional relative delay before the task may start" }
        },
        "required": ["description"]
    })));
    tools.push(tool_def("list_tasks", "List daemon-managed background tasks and their status, dependencies, schedule, and recent execution metadata.", serde_json::json!({
        "type": "object",
        "properties": {
            "status": { "type": "string", "enum": ["queued", "in_progress", "awaiting_approval", "blocked", "failed_analyzing", "budget_exceeded", "completed", "failed", "cancelled"] },
            "limit": { "type": "integer", "description": "Maximum number of tasks to return" }
        }
    })));
    tools.push(tool_def("start_goal_run", "Start a durable goal run for a long-running objective. The goal always executes on a dedicated thread; thread_id only contributes source-context lineage for the new goal thread.", serde_json::json!({
        "type": "object",
        "properties": {
            "goal": { "type": "string", "description": "The durable objective to pursue" },
            "title": { "type": "string", "description": "Optional short title for the goal run" },
            "thread_id": { "type": "string", "description": "Optional source thread id for lineage/context; the goal still gets a fresh dedicated execution thread" },
            "session_id": { "type": "string", "description": "Optional explicit session id override; defaults to the current session when available" },
            "priority": { "type": "string", "enum": ["low", "normal", "high", "urgent"], "description": "Goal priority" },
            "autonomy_level": { "type": "string", "enum": ["supervised", "aware", "autonomous"], "description": "Optional autonomy level override for the goal run" },
            "requires_approval": { "type": "boolean", "default": false, "description": "Whether this agent-created goal should wait for normal operator approval gates. Defaults to false so the responsible agent can auto-approve its own goal." },
            "launch_assignments": {
                "type": "array",
                "description": "Optional visible goal-local assignment snapshot. Include every role/persona the goal runner should be able to choose, such as swarog, reviewer, researcher, mokosh, or a configured subagent role.",
                "items": {
                    "type": "object",
                    "properties": {
                        "role_id": { "type": "string", "description": "Visible role or persona id, for example swarog, reviewer, researcher, mokosh, or a subagent role id" },
                        "enabled": { "type": "boolean", "description": "Whether this assignment is available to the goal runner" },
                        "provider": { "type": "string", "description": "Provider id for this role" },
                        "model": { "type": "string", "description": "Model id for this role" },
                        "reasoning_effort": { "type": "string", "description": "Optional reasoning effort for this role" },
                        "inherit_from_main": { "type": "boolean", "description": "Whether the row semantically inherits from the main assignment" }
                    },
                    "required": ["role_id", "provider", "model"]
                }
            }
        },
        "required": ["goal"]
    })));
    tools.push(tool_def("list_goal_runs", "List durable goal runs with their current status, active step metadata, and recent execution state.", serde_json::json!({
        "type": "object",
        "properties": {}
    })));
    tools.push(tool_def("submit_goal_step_verdict", "Submit the structured pass/fail verdict for the current goal-step verification task. This is the authoritative gate used to advance or requeue the current goal step.", serde_json::json!({
        "type": "object",
        "properties": {
            "verdict": { "type": "string", "enum": ["pass", "fail"], "description": "Use pass only when the current step satisfies all instructions, success criteria, todos, artifacts, and proof checks." },
            "explanation": { "type": "string", "description": "Concrete verdict explanation. For fail, describe the fixes required before the step can advance." },
            "task_id": { "type": "string", "description": "Optional current verification task ID. Use this when the prompt provides a Current task ID and hidden task context is unavailable." },
            "goal_run_id": { "type": "string", "description": "Optional guard; if provided it must match the current verification task's goal_run_id." },
            "goal_step_id": { "type": "string", "description": "Optional guard; if provided it must match the current verification task's goal_step_id." }
        },
        "required": ["verdict", "explanation"]
    })));
    tools.push(tool_def("list_triggers", "List configured event triggers with status, cooldown, last-fired metadata, and whether each trigger comes from packaged defaults or a custom entry.", serde_json::json!({
        "type": "object",
        "properties": {}
    })));
    tools.push(tool_def("ingest_webhook_event", "Validate a webhook-style event payload and route it through the trigger engine. This is the narrow ingest foundation for Pack 1 webhook/event flows.", serde_json::json!({
        "type": "object",
        "properties": {
            "event_family": { "type": "string", "description": "High-level event family, e.g. filesystem or system" },
            "event_kind": { "type": "string", "description": "Specific event kind within the family, e.g. file_changed or disk_pressure" },
            "state": { "type": "string", "description": "Optional state filter such as detected or critical" },
            "thread_id": { "type": "string", "description": "Optional thread scope for the ingested event" },
            "payload": { "type": "object", "description": "Optional webhook payload forwarded into trigger template rendering and event logging" }
        },
        "required": ["event_family", "event_kind"]
    })));
    tools.push(tool_def("add_trigger", "Create a new runtime event trigger, validate it, and persist it to the trigger registry. Pack 1 defaults already cover health/weles_health, health/subagent_health, filesystem/file_changed, and system/disk_pressure.", serde_json::json!({
        "type": "object",
        "properties": {
            "id": { "type": "string", "description": "Optional explicit trigger id" },
            "event_family": { "type": "string", "description": "High-level event family, e.g. health, filesystem, or system" },
            "event_kind": { "type": "string", "description": "Specific event kind within the family, e.g. weles_health, subagent_health, file_changed, or disk_pressure" },
            "agent_id": { "type": "string", "description": "Background agent/subagent that should handle the trigger. Defaults to weles." },
            "target_state": { "type": "string", "description": "Optional state filter, e.g. degraded, stuck, detected, or critical" },
            "thread_id": { "type": "string", "description": "Optional thread scope filter" },
            "enabled": { "type": "boolean", "description": "Whether the trigger starts enabled (default: true)" },
            "cooldown_secs": { "type": "integer", "description": "Per-trigger cooldown in seconds" },
            "risk_label": { "type": "string", "enum": ["low", "medium", "high"], "description": "Risk label used for routing/approval posture" },
            "notification_kind": { "type": "string", "description": "WorkflowNotice kind emitted when the trigger fires" },
            "prompt_template": { "type": "string", "description": "Optional background task prompt template. When set, the daemon queues real work instead of only emitting a notice." },
            "title_template": { "type": "string", "description": "Rendered notice title template" },
            "body_template": { "type": "string", "description": "Rendered notice body/details template" }
        },
        "required": ["event_family", "event_kind", "notification_kind", "title_template", "body_template"]
    })));
    tools.push(tool_def("show_dreams", "Show recent dream-state cycles, counterfactual evaluations, and persisted [dream] strategy hints.", serde_json::json!({
        "type": "object",
        "properties": {
            "limit": { "type": "integer", "description": "Maximum number of recent dream hints/cycles to return" }
        }
    })));
    tools.push(tool_def("show_harness_state", "Show the persisted state-transition harness projection for a thread/goal/task scope, including beliefs, tensions, commitments, effects, verification results, and learned procedures.", serde_json::json!({
        "type": "object",
        "properties": {
            "thread_id": { "type": "string", "description": "Optional thread scope; defaults to the current thread" },
            "goal_run_id": { "type": "string", "description": "Optional goal-run scope" },
            "task_id": { "type": "string", "description": "Optional task scope; defaults to the current task when available" },
            "limit": { "type": "integer", "description": "Maximum number of recent items per harness section to include" }
        }
    })));
    tools.push(tool_def(
        "cancel_task",
        "Cancel a queued, blocked, running, approval-pending, or retrying background task by ID.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "task_id": { "type": "string", "description": "Task ID to cancel" }
            },
            "required": ["task_id"]
        }),
    ));
    tools.push(tool_def("type_in_terminal", "Type text into an existing terminal session as raw keyboard input. Use this for: interactive TUI programs (codex, vim, htop), REPLs (python, node), typing commands in running shells, or any program that needs a real TTY. Text and Enter are sent with a small delay between them so TUIs process correctly. You can also send special keys like ctrl+c, escape, tab, arrow keys, etc.", serde_json::json!({
        "type": "object",
        "properties": {
            "text": { "type": "string", "description": "Text to type into the terminal" },
            "press_enter": { "type": "boolean", "description": "Whether to press Enter after typing (default: true)" },
            "key": { "type": "string", "description": "Send a special key instead of text. Options: enter, ctrl+c, ctrl+d, ctrl+z, ctrl+l, ctrl+a, ctrl+e, ctrl+u, ctrl+k, escape, tab, up, down, left, right, backspace, delete, home, end, page_up, page_down. When 'key' is set, 'text' is ignored." },
            "pane": { "type": "string", "description": "Pane ID or name (optional, defaults to first active session)" }
        },
        "required": []
    })));

    // Workspace tools — executed via WorkspaceCommand event on the frontend
    tools.push(tool_def(
        "list_workspaces",
        "List workspaces, surfaces, and panes (with names and IDs).",
        serde_json::json!({"type":"object","properties":{}}),
    ));
    tools.push(tool_def(
        "create_workspace",
        "Create a new workspace and make it active.",
        serde_json::json!({
            "type": "object",
            "properties": { "name": { "type": "string", "description": "Optional workspace name" } }
        }),
    ));
    tools.push(tool_def("set_active_workspace", "Set the active workspace by ID or name.", serde_json::json!({
        "type": "object",
        "properties": { "workspace": { "type": "string", "description": "Workspace ID or name" } },
        "required": ["workspace"]
    })));
    tools.push(tool_def(
        "create_surface",
        "Create a new surface (tab) in a workspace.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "workspace": { "type": "string", "description": "Optional workspace ID or name" },
                "name": { "type": "string", "description": "Optional surface name" }
            }
        }),
    ));
    tools.push(tool_def(
        "set_active_surface",
        "Set active surface by ID or name.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "surface": { "type": "string", "description": "Surface ID or name" },
                "workspace": { "type": "string", "description": "Optional workspace scope" }
            },
            "required": ["surface"]
        }),
    ));
    tools.push(tool_def("split_pane", "Split a pane horizontally or vertically. Works in BSP layout mode. In canvas mode, creates a new panel instead.", serde_json::json!({
        "type": "object",
        "properties": {
            "direction": { "type": "string", "enum": ["horizontal", "vertical"] },
            "pane": { "type": "string", "description": "Optional pane ID or name" },
            "new_pane_name": { "type": "string", "description": "Optional name for new pane" }
        },
        "required": ["direction"]
    })));
    tools.push(tool_def(
        "rename_pane",
        "Rename a pane.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "pane": { "type": "string", "description": "Optional pane ID or name" },
                "name": { "type": "string", "description": "New pane name" }
            },
            "required": ["name"]
        }),
    ));
    tools.push(tool_def("set_layout_preset", "Apply a layout preset to a surface.", serde_json::json!({
        "type": "object",
        "properties": {
            "preset": { "type": "string", "enum": ["single", "2-columns", "3-columns", "grid-2x2", "main-stack"] },
            "surface": { "type": "string", "description": "Optional surface ID or name" },
            "workspace": { "type": "string", "description": "Optional workspace scope" }
        },
        "required": ["preset"]
    })));
    tools.push(tool_def(
        "equalize_layout",
        "Equalize all split ratios in a surface.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "surface": { "type": "string", "description": "Optional surface ID or name" },
                "workspace": { "type": "string", "description": "Optional workspace scope" }
            }
        }),
    ));
    tools.push(tool_def(
        "list_snippets",
        "List saved snippets with names and content previews.",
        serde_json::json!({
            "type": "object",
            "properties": { "owner": { "type": "string", "enum": ["user", "assistant", "both"] } }
        }),
    ));
    tools.push(tool_def(
        "create_snippet",
        "Create a new snippet.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "content": { "type": "string" },
                "category": { "type": "string" },
                "description": { "type": "string" },
                "tags": { "type": "array", "items": { "type": "string" } }
            },
            "required": ["name", "content"]
        }),
    ));
    tools.push(tool_def("run_snippet", "Execute a snippet by ID or name in a pane.", serde_json::json!({
        "type": "object",
        "properties": {
            "snippet": { "type": "string", "description": "Snippet ID or name" },
            "pane": { "type": "string", "description": "Optional pane ID or name" },
            "params": { "type": "object", "additionalProperties": { "type": "string" } },
            "execute": { "type": "boolean", "description": "Append Enter after inserting (default: true)" }
        },
        "required": ["snippet"]
    })));

    if config.tool_synthesis.enabled {
        tools.push(tool_def("synthesize_tool", "Generate a guarded runtime tool from a conservative CLI --help surface or a GET OpenAPI operation, then register it in the local generated-tool registry.", serde_json::json!({
            "type": "object",
            "properties": {
                "kind": { "type": "string", "enum": ["cli", "openapi"], "description": "Generation source kind (default: cli)" },
                "target": { "type": "string", "description": "CLI invocation or OpenAPI spec URL" },
                "name": { "type": "string", "description": "Optional generated tool name override" },
                "operation_id": { "type": "string", "description": "Optional OpenAPI operationId to select" },
                "activate": { "type": "boolean", "description": "Activate immediately when policy allows it" }
            },
            "required": ["target"]
        })));
        tools.push(tool_def(
            "list_generated_tools",
            "List generated runtime tools with status, effectiveness, and promotion metadata.",
            serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        ));
        tools.push(tool_def("promote_generated_tool", "Promote a generated runtime tool into the generated skills library when it proves useful.", serde_json::json!({
            "type": "object",
            "properties": {
                "tool": { "type": "string", "description": "Generated tool ID" }
            },
            "required": ["tool"]
        })));
        tools.push(tool_def("activate_generated_tool", "Activate a newly synthesized runtime tool after review so it can appear in the callable tool surface on the next turn.", serde_json::json!({
            "type": "object",
            "properties": {
                "tool": { "type": "string", "description": "Generated tool ID" }
            },
            "required": ["tool"]
        })));
        tools.push(tool_def("restore_generated_tool", "Restore an archived generated runtime tool back to active status without promoting it.", serde_json::json!({
            "type": "object",
            "properties": {
                "tool": { "type": "string", "description": "Generated tool ID" }
            },
            "required": ["tool"]
        })));
        tools.extend(generated_tool_definitions(config, agent_data_dir));
    }

    // Plugin API proxy tool -- always available (PluginManager handles disabled/missing checks)
    tools.push(tool_def(
        "plugin_api_call",
        "Call a plugin API endpoint. The daemon proxies the HTTP request, handles auth, rate limiting, and returns the response as text.",
        serde_json::json!({
            "type": "object",
            "properties": {
                "plugin_name": { "type": "string", "description": "Name of the installed plugin" },
                "endpoint_name": { "type": "string", "description": "Name of the API endpoint from the plugin manifest" },
                "params": { "type": "object", "description": "Parameters passed to the endpoint template (optional)" }
            },
            "required": ["plugin_name", "endpoint_name"]
        }),
    ));
}
