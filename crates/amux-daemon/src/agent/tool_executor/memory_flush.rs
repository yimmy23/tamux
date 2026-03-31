pub fn get_memory_flush_tools() -> Vec<ToolDefinition> {
    vec![tool_def(
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
    )]
}

fn tool_def(name: &str, description: &str, parameters: serde_json::Value) -> ToolDefinition {
    ToolDefinition {
        tool_type: "function".into(),
        function: ToolFunctionDef {
            name: name.into(),
            description: description.into(),
            parameters,
        },
    }
}

// ---------------------------------------------------------------------------
// Tool execution
// ---------------------------------------------------------------------------

