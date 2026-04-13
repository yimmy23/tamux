fn add_available_tools_part_a(
    tools: &mut Vec<ToolDefinition>,
    config: &AgentConfig,
    _agent_data_dir: &std::path::Path,
    _has_workspace_topology: bool,
) {
    if config.tools.bash {
        tools.push(tool_def(
            "bash_command",
            "Execute a shell command. TUI-originated turns run headless by default; Electron-originated turns may use a managed terminal when the command needs terminal state or interactivity. Omit `session` in normal TUI/chat turns unless you intentionally target a known live terminal. For long-running managed-terminal work, prefer non-blocking execution and poll the returned `operation_id` with `get_operation_status`. For large or awkward file writes, prefer a minimal Python writer over fragile shell escaping, but inspect the Python carefully so it only performs the intended write.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "Shell command to execute in a managed terminal session" },
                    "rationale": { "type": "string", "description": "Why this command should run" },
                    "session": { "type": "string", "description": "Optional terminal session ID or unique substring. Leave this unset in normal TUI/chat turns unless you deliberately target a currently live session." },
                    "cwd": { "type": "string", "description": "Optional working directory" },
                    "allow_network": { "type": "boolean", "description": "Whether network access is expected" },
                    "sandbox_enabled": { "type": "boolean", "description": "Whether sandboxing should be requested" },
                    "security_level": { "type": "string", "enum": ["highest", "moderate", "lowest", "yolo"], "description": "Approval strictness level" },
                    "language_hint": { "type": "string", "description": "Optional language hint for validation" },
                    "wait_for_completion": { "type": "boolean", "description": "Wait for completion and return exit status/output summary (default: true)" },
                    "timeout_seconds": { "type": "integer", "description": "Wait timeout (default: 30, max: 600). If you set a value above 600, the command auto-runs in background, returns an `operation_id`, and can be polled with `get_operation_status`." }
                },
                "required": ["command"]
            }),
        ));
    }

    if config.tools.file_operations {
        tools.push(tool_def(
            "list_files",
            "List files and directories at a given path through a tamux-managed terminal session.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "Directory path to list" },
                    "session": { "type": "string", "description": "Optional terminal session ID or unique substring" },
                    "timeout_seconds": { "type": "integer", "description": "Max time to wait for completion (default: 30, max: 600)" }
                },
                "required": ["path"]
            }),
        ));

        tools.push(tool_def(
            "read_file",
            "Read the contents of a file. Always prefer bounded reads with offset/limit windows instead of dumping entire files.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path to read" },
                    "offset": { "type": "integer", "description": "0-based starting line offset (default: 0)" },
                    "limit": { "type": "integer", "description": "Maximum lines to read in this window (default: 250)" },
                    "max_lines": { "type": "integer", "description": "Deprecated alias for limit" }
                },
                "required": ["path"]
            }),
        ));

        tools.push(tool_def(
            "get_git_line_statuses",
            "Report git statuses for the current file lines in a bounded window. Use this when you need to know which current lines are unchanged, modified, or added without parsing a full diff.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path to inspect inside a git repository" },
                    "start_line": { "type": "integer", "description": "1-based starting line number for the window (default: 1)" },
                    "limit": { "type": "integer", "description": "Maximum number of current lines to inspect (default: 250, max: 500)" }
                },
                "required": ["path"]
            }),
        ));

        tools.push(tool_def(
            "write_file",
            "Write content to a file. Supports JSON args or a multipart-style payload with path/file parts so larger content does not have to fit inside one giant JSON string.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path to write" },
                    "content": { "type": "string", "description": "File content to write" },
                    "session": { "type": "string", "description": "Optional terminal session ID or unique substring. Leave this unset in normal TUI/chat turns unless you deliberately target a currently live session." },
                    "timeout_seconds": { "type": "integer", "description": "Max time to wait for completion (default: 30, max: 600)" }
                },
                "required": ["path", "content"]
            }),
        ));

        tools.push(tool_def(
            "create_file",
            "Create a new file directly from the daemon filesystem context. Supports JSON args or a multipart-style payload with filename/cwd/file parts. Fails if the file already exists unless overwrite=true. Prefer multipart-style payloads for larger content instead of giant JSON strings.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path to create" },
                    "filename": { "type": "string", "description": "Filename to create when combined with cwd" },
                    "cwd": { "type": "string", "description": "Base directory used with filename/path for daemon-native writes" },
                    "content": { "type": "string", "description": "Initial file content" },
                    "overwrite": { "type": "boolean", "description": "Allow replacing an existing file (default: false)" }
                },
                "required": ["content"]
            }),
        ));

        tools.push(tool_def(
            "append_to_file",
            "Append text to the end of an existing file without rewriting the whole file.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path to append to" },
                    "content": { "type": "string", "description": "Text to append" },
                    "create_if_missing": { "type": "boolean", "description": "Create the file if it does not exist (default: false)" }
                },
                "required": ["path", "content"]
            }),
        ));

        tools.push(tool_def(
            "replace_in_file",
            "Replace a specific fragment inside a file. Use this for targeted edits instead of rewriting the full file.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path to edit" },
                    "old_text": { "type": "string", "description": "Exact text to replace" },
                    "new_text": { "type": "string", "description": "Replacement text" },
                    "replace_all": { "type": "boolean", "description": "Replace every occurrence instead of exactly one (default: false)" }
                },
                "required": ["path", "old_text", "new_text"]
            }),
        ));

        tools.push(tool_def(
            "apply_file_patch",
            "Apply one or more exact text replacements to a file in order. Use this for multi-hunk targeted edits. Patch must start with '*** Begin Patch' and end with '*** End Patch'",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string", "description": "File path to patch" },
                    "edits": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "old_text": { "type": "string", "description": "Exact existing text to replace" },
                                "new_text": { "type": "string", "description": "Replacement text" },
                                "replace_all": { "type": "boolean", "description": "Replace all occurrences for this edit (default: false)" }
                            },
                            "required": ["old_text", "new_text"]
                        }
                    }
                },
                "required": ["path", "edits"]
            }),
        ));

        tools.push(tool_def(
            "apply_patch",
            "Apply a harness-style patch with `*** Begin Patch` / `*** End Patch` markers, supporting Add/Update/Delete file actions. Update hunks must include `@@` and at least one `-old` / `+new` line pair. Use `input` or legacy alias `patch` for the full patch text. For exact text replacements, use `apply_file_patch` instead.",
            serde_json::json!({
                "type": "object",
                "minProperties": 1,
                "properties": {
                    "input": { "type": "string", "description": "Harness-style patch text in the apply_patch format. Prefer this form for provider compatibility." },
                    "patch": { "type": "string", "description": "Legacy alias for `input` containing the same harness-style patch text." },
                    "explanation": { "type": "string", "description": "Optional short explanation for why the patch is being applied." }
                }
            }),
        ));

        tools.push(tool_def(
            "search_files",
            "Search for a pattern in files using ripgrep. Returns matching lines with file paths and line numbers.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "pattern": { "type": "string", "description": "Regex pattern to search for" },
                    "path": { "type": "string", "description": "Directory to search in (default: current directory)" },
                    "file_pattern": { "type": "string", "description": "Glob pattern to filter files (e.g. '*.rs', '*.ts')" },
                    "max_results": { "type": "integer", "description": "Max results to return (default: 50, max: 200)" },
                    "timeout_seconds": { "type": "integer", "minimum": 0, "maximum": 600, "description": "Max time to wait for completion (default: 120, max: 600)" }
                },
                "required": ["pattern"]
            }),
        ));
    }

    if config.tools.system_info {
        tools.push(tool_def(
            "get_system_info",
            "Get system information: CPU, memory, disk, load average, hostname.",
            serde_json::json!({ "type": "object", "properties": {} }),
        ));

        tools.push(tool_def(
            "list_processes",
            "List running processes sorted by CPU usage.",
            serde_json::json!({
                "type": "object",
                "properties": {
                    "limit": { "type": "integer", "description": "Max processes to show (default: 20)" }
                }
            }),
        ));
    }

}
