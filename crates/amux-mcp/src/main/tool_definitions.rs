use serde_json::Value;

pub(super) fn tool_definitions() -> Value {
    serde_json::json!([
        {
            "name": "execute_command",
            "description": "Execute a managed command inside a tamux terminal session. The command runs in a sandboxed lane with approval gating, automatic snapshots, and telemetry.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": {
                        "type": "string",
                        "description": "UUID of the target terminal session"
                    },
                    "command": {
                        "type": "string",
                        "description": "The shell command to execute"
                    },
                    "rationale": {
                        "type": "string",
                        "description": "Human-readable explanation of why this command is being run"
                    }
                },
                "required": ["session_id", "command", "rationale"]
            }
        },
        {
            "name": "search_history",
            "description": "Search the daemon's command and transcript history using full-text search.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Full-text search query"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results to return"
                    }
                },
                "required": ["query"]
            }
        },
        {
            "name": "find_symbol",
            "description": "Search for code symbols (functions, types, variables) using the daemon's semantic indexing.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace_root": {
                        "type": "string",
                        "description": "Absolute path to the workspace root directory"
                    },
                    "symbol": {
                        "type": "string",
                        "description": "Symbol name or pattern to search for"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results to return"
                    }
                },
                "required": ["workspace_root", "symbol"]
            }
        },
        {
            "name": "list_snapshots",
            "description": "List recorded workspace snapshots and checkpoints. Snapshots are created automatically before managed commands.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "workspace_id": {
                        "type": "string",
                        "description": "Optional workspace ID to filter snapshots"
                    }
                },
                "required": []
            }
        },
        {
            "name": "restore_snapshot",
            "description": "Restore a previously recorded workspace snapshot, reverting the workspace to that point in time.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "snapshot_id": {
                        "type": "string",
                        "description": "ID of the snapshot to restore"
                    }
                },
                "required": ["snapshot_id"]
            }
        },
        {
            "name": "scrub_sensitive",
            "description": "Scrub sensitive data (secrets, tokens, passwords) from a text string using the daemon's detection engine.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "text": {
                        "type": "string",
                        "description": "Text to scrub for sensitive data"
                    }
                },
                "required": ["text"]
            }
        },
        {
            "name": "list_sessions",
            "description": "List all active terminal sessions and browser panels. Includes workspace/surface hierarchy, pane types (terminal/browser), URLs for browser panes, session IDs, CWD, and active commands.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "required": []
            }
        },
        {
            "name": "verify_integrity",
            "description": "Verify the integrity of WORM (Write-Once-Read-Many) telemetry ledgers to detect tampering.",
            "inputSchema": {
                "type": "object",
                "properties": {},
                "required": []
            }
        },
        {
            "name": "get_terminal_content",
            "description": "Read the scrollback buffer (visible content) of a terminal session. Use this to see what is currently displayed in a user's terminal pane.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": {
                        "type": "string",
                        "description": "UUID of the terminal session to read"
                    },
                    "max_lines": {
                        "type": "integer",
                        "description": "Maximum number of lines to return from the tail (default: 100)"
                    }
                },
                "required": ["session_id"]
            }
        },
        {
            "name": "type_in_terminal",
            "description": "Send keystrokes/input to a terminal session. Use this to type commands or interact with running programs in the user's terminal.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "session_id": {
                        "type": "string",
                        "description": "UUID of the terminal session"
                    },
                    "input": {
                        "type": "string",
                        "description": "Text to type into the terminal. Use \\n for Enter key."
                    }
                },
                "required": ["session_id", "input"]
            }
        },
        {
            "name": "enqueue_task",
            "description": "Queue a daemon-managed background task. Supports dependencies and scheduled execution for reminders, follow-up gateway messages, and deferred automation.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "title": { "type": "string", "description": "Optional short title for the task" },
                    "description": { "type": "string", "description": "Detailed task instructions for the daemon agent" },
                    "priority": { "type": "string", "enum": ["low", "normal", "high", "urgent"], "description": "Task priority" },
                    "command": { "type": "string", "description": "Optional preferred command or entrypoint" },
                    "session_id": { "type": "string", "description": "Optional target terminal session" },
                    "dependencies": { "type": "array", "items": { "type": "string" }, "description": "Task IDs that must complete first" },
                    "scheduled_at": { "type": "integer", "description": "Optional Unix timestamp in milliseconds" },
                    "schedule_at": { "type": "string", "description": "Optional RFC3339 timestamp" },
                    "delay_seconds": { "type": "integer", "description": "Optional relative delay before task start" }
                },
                "required": ["description"]
            }
        },
        {
            "name": "list_tasks",
            "description": "List daemon-managed background tasks with status, schedule, and dependency metadata.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "cancel_task",
            "description": "Cancel a queued, blocked, running, or approval-pending daemon task by ID.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "task_id": { "type": "string", "description": "Task ID to cancel" }
                },
                "required": ["task_id"]
            }
        },
        {
            "name": "list_todos",
            "description": "List daemon-managed planner todos for all agent threads.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "get_todos",
            "description": "Fetch daemon-managed planner todos for a specific agent thread.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "thread_id": { "type": "string", "description": "Agent thread ID" }
                },
                "required": ["thread_id"]
            }
        },
        {
            "name": "list_skills",
            "description": "List reusable local tamux skills from the platform-specific ~/.tamux/skills directory.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": { "type": "string", "description": "Optional skill name or path filter" },
                    "limit": { "type": "integer", "description": "Maximum skills to return" },
                    "cursor": { "type": "string", "description": "Opaque cursor returned by a previous list_skills call" }
                }
            }
        },
        {
            "name": "discover_skills",
            "description": "Rank installed tamux skills for a task and return the recommended next action.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {
                        "type": "string",
                        "description": "Task or problem description to rank skills against"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of ranked skill candidates to return"
                    },
                    "cursor": {
                        "type": "string",
                        "description": "Opaque cursor returned by a previous discover_skills call for the same query"
                    },
                    "session_id": {
                        "type": "string",
                        "description": "Optional terminal session UUID for workspace-aware ranking"
                    }
                },
                "required": ["query"]
            }
        },
        {
            "name": "ask_questions",
            "description": "Show a blocking multiple-choice question in tamux clients and wait for one compact token answer. Put the full question and answer text in content; keep buttons/options limited to short ordered tokens like A/B/C/D or 1/2/3/4.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "content": {
                        "type": "string",
                        "description": "Full question content including the detailed answer text for each option"
                    },
                    "options": {
                        "type": "array",
                        "description": "Compact button tokens only, such as [\"A\", \"B\", \"C\"] or [\"1\", \"2\"]",
                        "items": {
                            "type": "string"
                        }
                    },
                    "session_id": {
                        "type": "string",
                        "description": "Optional terminal session UUID for workspace-aware prompting"
                    }
                },
                "required": ["content", "options"]
            }
        },
        {
            "name": "read_skill",
            "description": "Read a local tamux skill document by name, stem, or relative path under the skills directory.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "skill": { "type": "string", "description": "Skill name, file stem, or relative path" },
                    "max_lines": { "type": "integer", "description": "Maximum lines to read" }
                },
                "required": ["skill"]
            }
        },
        {
            "name": "list_skill_variants",
            "description": "List daemon-tracked skill variants with lifecycle status, usage counters, and context tags.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "status": { "type": "string", "description": "Optional lifecycle status filter such as active, archived, merged, or promoted-to-canonical" },
                    "limit": { "type": "integer", "description": "Maximum number of variants to return" },
                    "cursor": { "type": "string", "description": "Opaque cursor returned by a previous list_skill_variants call" }
                }
            }
        },
        {
            "name": "inspect_skill_variant",
            "description": "Inspect a daemon-tracked skill variant by variant ID or skill name, including lifecycle inspection notes and current skill content.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "identifier": { "type": "string", "description": "Variant ID or skill identifier to inspect" }
                },
                "required": ["identifier"]
            }
        },
        {
            "name": "start_goal_run",
            "description": "Start a durable goal run that plans, executes child tasks, handles approvals, and reflects on outcomes.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "goal": { "type": "string", "description": "The long-running objective to pursue" },
                    "title": { "type": "string", "description": "Optional short title for the goal run" },
                    "thread_id": { "type": "string", "description": "Optional existing agent thread to attach to" },
                    "session_id": { "type": "string", "description": "Optional preferred terminal session" },
                    "priority": { "type": "string", "enum": ["low", "normal", "high", "urgent"], "description": "Goal priority" },
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
            }
        },
        {
            "name": "list_goal_runs",
            "description": "List durable goal runs with status, current step, metrics, and history metadata.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "get_goal_run",
            "description": "Fetch a specific goal run with full plan, events, and derived metrics.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "goal_run_id": { "type": "string", "description": "Goal run ID" }
                },
                "required": ["goal_run_id"]
            }
        },
        {
            "name": "control_goal_run",
            "description": "Control a goal run lifecycle or rerun a specific step. Supported actions: pause, resume, cancel, retry_step, rerun_from_step.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "goal_run_id": { "type": "string", "description": "Goal run ID" },
                    "action": { "type": "string", "enum": ["pause", "resume", "cancel", "retry_step", "rerun_from_step"], "description": "Control action" },
                    "step_index": { "type": "integer", "description": "Optional zero-based step index for retry_step or rerun_from_step" }
                },
                "required": ["goal_run_id", "action"]
            }
        },
        {
            "name": "get_operator_model",
            "description": "Fetch the daemon's aggregate operator model, including learned cognitive style, risk tolerance, session rhythm, and attention topology.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "reset_operator_model",
            "description": "Reset the daemon's aggregate operator model, clearing learned shortcuts and accumulated operator telemetry.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "get_causal_trace_report",
            "description": "Summarize causal trace outcomes for a tool or decision option, including success/failure counts and recent reasons.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "option_type": { "type": "string", "description": "Tool or option type, such as bash_command, execute_managed_command, goal_plan, or replan_after_failure" },
                    "limit": { "type": "integer", "description": "Maximum number of recent causal traces to aggregate" }
                },
                "required": ["option_type"]
            }
        },
        {
            "name": "get_counterfactual_report",
            "description": "Estimate likely risk for a candidate command family using recent causal history for the given tool or option type.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "option_type": { "type": "string", "description": "Tool or option type, such as bash_command or execute_managed_command" },
                    "command_family": { "type": "string", "description": "Representative command or family hint to compare against recent causal history" },
                    "limit": { "type": "integer", "description": "Maximum number of recent causal traces to inspect" }
                },
                "required": ["option_type", "command_family"]
            }
        },
        {
            "name": "get_memory_provenance_report",
            "description": "Inspect durable tamux memory provenance with source, age-based confidence, and active/uncertain/retracted status.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "target": { "type": "string", "description": "Optional target memory file filter, such as MEMORY.md, USER.md, or SOUL.md" },
                    "limit": { "type": "integer", "description": "Maximum number of recent provenance entries to include" }
                }
            }
        },
        {
            "name": "read_memory",
            "description": "Read persistent MEMORY.md plus related structured memory layers. This response is injection-aware and avoids re-sending already injected fresh base markdown unless explicitly requested.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "thread_id": { "type": "string", "description": "Optional agent thread ID for thread-scoped injection-state and structural-memory lookups" },
                    "task_id": { "type": "string", "description": "Optional task ID for task-scoped memory lookup; overrides thread_id when both are provided" },
                    "include_already_injected": { "type": "boolean", "description": "Force inclusion of base markdown even when a fresh copy is already injected in the prompt (default: false)" },
                    "include_base_markdown": { "type": "boolean", "description": "Include MEMORY.md content when not skipped by injection state (default: true)" },
                    "include_operator_profile_json": { "type": "boolean", "description": "Include structured operator profile JSON (default: true)" },
                    "include_operator_model_summary": { "type": "boolean", "description": "Include operator-model prompt summary when available (default: true)" },
                    "include_thread_structural_memory": { "type": "boolean", "description": "Include thread structural memory summaries when available (default: true)" },
                    "limit_per_layer": { "type": "integer", "description": "Maximum items returned for list-style layers (default: 5, max: 25)" }
                }
            }
        },
        {
            "name": "read_user",
            "description": "Read persistent USER.md plus related structured operator layers. This response is injection-aware and avoids re-sending already injected fresh base markdown unless explicitly requested.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "thread_id": { "type": "string", "description": "Optional agent thread ID for thread-scoped injection-state and structural-memory lookups" },
                    "task_id": { "type": "string", "description": "Optional task ID for task-scoped memory lookup; overrides thread_id when both are provided" },
                    "include_already_injected": { "type": "boolean", "description": "Force inclusion of base markdown even when a fresh copy is already injected in the prompt (default: false)" },
                    "include_base_markdown": { "type": "boolean", "description": "Include USER.md content when not skipped by injection state (default: true)" },
                    "include_operator_profile_json": { "type": "boolean", "description": "Include structured operator profile JSON (default: true)" },
                    "include_operator_model_summary": { "type": "boolean", "description": "Include operator-model prompt summary when available (default: true)" },
                    "include_thread_structural_memory": { "type": "boolean", "description": "Include thread structural memory summaries when available (default: false)" },
                    "limit_per_layer": { "type": "integer", "description": "Maximum items returned for list-style layers (default: 5, max: 25)" }
                }
            }
        },
        {
            "name": "read_soul",
            "description": "Read persistent SOUL.md plus related structured context layers. This response is injection-aware and avoids re-sending already injected fresh base markdown unless explicitly requested.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "thread_id": { "type": "string", "description": "Optional agent thread ID for thread-scoped injection-state and structural-memory lookups" },
                    "task_id": { "type": "string", "description": "Optional task ID for task-scoped memory lookup; overrides thread_id when both are provided" },
                    "include_already_injected": { "type": "boolean", "description": "Force inclusion of base markdown even when a fresh copy is already injected in the prompt (default: false)" },
                    "include_base_markdown": { "type": "boolean", "description": "Include SOUL.md content when not skipped by injection state (default: true)" },
                    "include_operator_profile_json": { "type": "boolean", "description": "Include structured operator profile JSON (default: true)" },
                    "include_operator_model_summary": { "type": "boolean", "description": "Include operator-model prompt summary when available (default: true)" },
                    "include_thread_structural_memory": { "type": "boolean", "description": "Include thread structural memory summaries when available (default: false)" },
                    "limit_per_layer": { "type": "integer", "description": "Maximum items returned for list-style layers (default: 5, max: 25)" }
                }
            }
        },
        {
            "name": "search_memory",
            "description": "Search MEMORY.md plus related structured memory layers. This response is injection-aware and skips already injected fresh base markdown unless explicitly requested.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "thread_id": { "type": "string", "description": "Optional agent thread ID for thread-scoped injection-state and structural-memory lookups" },
                    "task_id": { "type": "string", "description": "Optional task ID for task-scoped memory lookup; overrides thread_id when both are provided" },
                    "query": { "type": "string", "description": "Search query" },
                    "limit": { "type": "integer", "description": "Maximum matches to return (default: 5, max: 25)" },
                    "include_already_injected": { "type": "boolean", "description": "Allow fresh injected MEMORY.md content to appear in search matches (default: false)" },
                    "include_base_markdown": { "type": "boolean", "description": "Search MEMORY.md when not skipped by injection state (default: true)" },
                    "include_operator_profile_json": { "type": "boolean", "description": "Search structured operator profile JSON (default: true)" },
                    "include_operator_model_summary": { "type": "boolean", "description": "Search operator-model prompt summary when available (default: true)" },
                    "include_thread_structural_memory": { "type": "boolean", "description": "Search thread structural memory summaries when available (default: true)" }
                },
                "required": ["query"]
            }
        },
        {
            "name": "search_user",
            "description": "Search USER.md plus related structured operator layers. This response is injection-aware and skips already injected fresh base markdown unless explicitly requested.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "thread_id": { "type": "string", "description": "Optional agent thread ID for thread-scoped injection-state and structural-memory lookups" },
                    "task_id": { "type": "string", "description": "Optional task ID for task-scoped memory lookup; overrides thread_id when both are provided" },
                    "query": { "type": "string", "description": "Search query" },
                    "limit": { "type": "integer", "description": "Maximum matches to return (default: 5, max: 25)" },
                    "include_already_injected": { "type": "boolean", "description": "Allow fresh injected USER.md content to appear in search matches (default: false)" },
                    "include_base_markdown": { "type": "boolean", "description": "Search USER.md when not skipped by injection state (default: true)" },
                    "include_operator_profile_json": { "type": "boolean", "description": "Search structured operator profile JSON (default: true)" },
                    "include_operator_model_summary": { "type": "boolean", "description": "Search operator-model prompt summary when available (default: true)" },
                    "include_thread_structural_memory": { "type": "boolean", "description": "Search thread structural memory summaries when available (default: false)" }
                },
                "required": ["query"]
            }
        },
        {
            "name": "search_soul",
            "description": "Search SOUL.md plus related structured context layers. This response is injection-aware and skips already injected fresh base markdown unless explicitly requested.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "thread_id": { "type": "string", "description": "Optional agent thread ID for thread-scoped injection-state and structural-memory lookups" },
                    "task_id": { "type": "string", "description": "Optional task ID for task-scoped memory lookup; overrides thread_id when both are provided" },
                    "query": { "type": "string", "description": "Search query" },
                    "limit": { "type": "integer", "description": "Maximum matches to return (default: 5, max: 25)" },
                    "include_already_injected": { "type": "boolean", "description": "Allow fresh injected SOUL.md content to appear in search matches (default: false)" },
                    "include_base_markdown": { "type": "boolean", "description": "Search SOUL.md when not skipped by injection state (default: true)" },
                    "include_operator_profile_json": { "type": "boolean", "description": "Search structured operator profile JSON (default: true)" },
                    "include_operator_model_summary": { "type": "boolean", "description": "Search operator-model prompt summary when available (default: true)" },
                    "include_thread_structural_memory": { "type": "boolean", "description": "Search thread structural memory summaries when available (default: false)" }
                },
                "required": ["query"]
            }
        },
        {
            "name": "get_provenance_report",
            "description": "Inspect trusted execution provenance, including hash/signature validity and recent event summaries.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "limit": { "type": "integer", "description": "Maximum number of recent provenance entries to include" }
                }
            }
        },
        {
            "name": "query_audits",
            "description": "Query daemon action-audit entries by action type and time window.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "action_types": {
                        "type": "array",
                        "items": { "type": "string" },
                        "description": "Optional action type filters such as tool, goal, task, or audit"
                    },
                    "since": {
                        "type": "integer",
                        "description": "Optional Unix timestamp in milliseconds to filter recent entries"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of audit entries to return"
                    }
                }
            }
        },
        {
            "name": "generate_soc2_artifact",
            "description": "Generate a SOC2-style audit artifact from recent provenance events.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "period_days": { "type": "integer", "description": "How many recent days of provenance to include" }
                }
            }
        },
        {
            "name": "get_collaboration_sessions",
            "description": "Inspect active subagent collaboration sessions, contributions, disagreements, and consensus state.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "parent_task_id": { "type": "string", "description": "Optional parent task ID to narrow to one collaboration session" }
                }
            }
        },
        {
            "name": "list_generated_tools",
            "description": "List runtime-generated tools with status, effectiveness, and promotion metadata.",
            "inputSchema": { "type": "object", "properties": {} }
        },
        {
            "name": "synthesize_tool",
            "description": "Generate a guarded runtime tool from a conservative CLI or GET OpenAPI operation.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "kind": { "type": "string", "enum": ["cli", "openapi"] },
                    "target": { "type": "string", "description": "CLI invocation or OpenAPI spec URL" },
                    "name": { "type": "string", "description": "Optional generated tool name override" },
                    "operation_id": { "type": "string", "description": "Optional OpenAPI operationId" },
                    "activate": { "type": "boolean", "description": "Request immediate activation when policy allows it" }
                },
                "required": ["target"]
            }
        },
        {
            "name": "run_generated_tool",
            "description": "Execute a generated runtime tool by name.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "tool_name": { "type": "string", "description": "Generated tool ID" },
                    "args": { "type": "object", "description": "Arguments object for the generated tool" }
                },
                "required": ["tool_name"]
            }
        },
        {
            "name": "promote_generated_tool",
            "description": "Promote a generated runtime tool into the generated skills library.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "tool_name": { "type": "string", "description": "Generated tool ID" }
                },
                "required": ["tool_name"]
            }
        },
        {
            "name": "activate_generated_tool",
            "description": "Activate a newly synthesized runtime tool after review so it can be called on subsequent turns.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "tool_name": { "type": "string", "description": "Generated tool ID" }
                },
                "required": ["tool_name"]
            }
        },
        {
            "name": "get_git_status",
            "description": "Get git status for a working directory, showing modified, staged, and untracked files.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Absolute path to the git repository" }
                },
                "required": ["path"]
            }
        },
        {
            "name": "semantic_query",
            "description": "Query the daemon semantic environment for packages, dependencies, services, imports, conventions, or temporal workspace history.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "kind": {
                        "type": "string",
                        "enum": ["summary", "packages", "dependencies", "dependents", "services", "service_dependencies", "service_dependents", "imports", "imported_by", "conventions", "temporal"],
                        "description": "Semantic query kind"
                    },
                    "target": {
                        "type": "string",
                        "description": "Optional package, service, file fragment, or convention target"
                    },
                    "limit": {
                        "type": "integer",
                        "description": "Maximum number of results to render"
                    },
                    "root": {
                        "type": "string",
                        "description": "Optional explicit workspace root override when no session context exists"
                    }
                }
            }
        }
    ])
}

#[cfg(test)]
mod tests {
    use super::tool_definitions;

    #[test]
    fn tool_definitions_include_query_audits() {
        let defs = tool_definitions();
        let tools = defs
            .as_array()
            .expect("tool definitions should be an array");
        assert!(
            tools.iter().any(|tool| tool["name"] == "query_audits"),
            "query_audits tool definition should be present"
        );
    }

    #[test]
    fn tool_definitions_include_semantic_query() {
        let defs = tool_definitions();
        let tools = defs
            .as_array()
            .expect("tool definitions should be an array");
        assert!(
            tools.iter().any(|tool| tool["name"] == "semantic_query"),
            "semantic_query tool definition should be present"
        );
    }

    #[test]
    fn tool_definitions_include_discover_skills() {
        let defs = tool_definitions();
        let tools = defs
            .as_array()
            .expect("tool definitions should be an array");
        let discover_skills = tools
            .iter()
            .find(|tool| tool["name"] == "discover_skills")
            .expect("discover_skills tool definition should be present");
        assert!(
            discover_skills["description"]
                .as_str()
                .unwrap_or_default()
                .contains("recommended next action"),
            "discover_skills definition should describe the richer DIDSM action-oriented payload"
        );
    }

    #[test]
    fn tool_definitions_include_ask_questions() {
        let defs = tool_definitions();
        let tools = defs
            .as_array()
            .expect("tool definitions should be an array");
        let ask_questions = tools
            .iter()
            .find(|tool| tool["name"] == "ask_questions")
            .expect("ask_questions tool definition should be present");

        let properties = ask_questions["inputSchema"]["properties"]
            .as_object()
            .expect("ask_questions should expose an object schema");
        assert!(properties.contains_key("content"));
        assert!(properties.contains_key("options"));
        assert!(properties.contains_key("session_id"));

        let required = ask_questions["inputSchema"]["required"]
            .as_array()
            .expect("ask_questions required fields should be present")
            .iter()
            .filter_map(|item| item.as_str())
            .collect::<Vec<_>>();
        assert_eq!(required, vec!["content", "options"]);
    }

    #[test]
    fn tool_definitions_include_read_memory() {
        let defs = tool_definitions();
        let tools = defs
            .as_array()
            .expect("tool definitions should be an array");
        let tool = tools
            .iter()
            .find(|tool| tool["name"] == "read_memory")
            .expect("read_memory tool definition should be present");
        assert!(
            tool["inputSchema"]["properties"]
                .as_object()
                .is_some_and(|properties| properties.contains_key("thread_id")),
            "read_memory should expose thread_id for thread-scoped lookup"
        );
    }

    #[test]
    fn tool_definitions_include_search_memory() {
        let defs = tool_definitions();
        let tools = defs
            .as_array()
            .expect("tool definitions should be an array");
        let tool = tools
            .iter()
            .find(|tool| tool["name"] == "search_memory")
            .expect("search_memory tool definition should be present");
        assert!(
            tool["inputSchema"]["properties"]
                .as_object()
                .is_some_and(|properties| properties.contains_key("thread_id")),
            "search_memory should expose thread_id for thread-scoped lookup"
        );
    }

    #[test]
    fn tool_definitions_start_goal_run_exposes_launch_assignments() {
        let defs = tool_definitions();
        let tools = defs
            .as_array()
            .expect("tool definitions should be an array");
        let tool = tools
            .iter()
            .find(|tool| tool["name"] == "start_goal_run")
            .expect("start_goal_run tool definition should be present");
        let properties = tool["inputSchema"]["properties"]
            .as_object()
            .expect("start_goal_run should expose an object schema");
        assert!(
            properties.contains_key("launch_assignments"),
            "start_goal_run should let callers provide the visible assignment snapshot"
        );
        assert!(
            properties.contains_key("requires_approval"),
            "start_goal_run should expose the approval policy for agent-owned goals"
        );
        assert_eq!(
            properties["requires_approval"]["default"], false,
            "agent-owned goal approval should default to false"
        );
    }
}
