use crate::agent::CompletionContainerInfo;

#[derive(Debug, Clone, Default)]
struct AnthropicStreamMessageStart {
    response_id: Option<String>,
    upstream_model: Option<String>,
    upstream_role: Option<String>,
    upstream_message_type: Option<String>,
    upstream_container: Option<CompletionContainerInfo>,
}

impl AnthropicStreamMessageStart {
    fn capture(&mut self, parsed: &serde_json::Value) {
        self.response_id = parsed.pointer("/message/id").and_then(|value| value.as_str()).map(str::to_string);
        self.upstream_model = parsed.pointer("/message/model").and_then(|value| value.as_str()).map(str::to_string);
        self.upstream_role = parsed.pointer("/message/role").and_then(|value| value.as_str()).map(str::to_string);
        self.upstream_message_type = parsed.pointer("/message/type").and_then(|value| value.as_str()).map(str::to_string);
        self.upstream_container = parsed.pointer("/message/container").and_then(extract_completion_container_info);
    }
}

fn extract_completion_container_info(value: &serde_json::Value) -> Option<CompletionContainerInfo> {
    Some(CompletionContainerInfo {
        id: value.get("id")?.as_str()?.to_string(),
        expires_at: value.get("expires_at")?.as_str()?.to_string(),
    })
}