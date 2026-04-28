use crate::agent::{CompletionUpstreamContentBlock, CompletionUpstreamMessage};

#[derive(Debug, Clone, Default)]
struct AnthropicStreamUpstreamMessage {
    message: CompletionUpstreamMessage,
    current_text_block: Option<usize>,
    current_thinking_block: Option<usize>,
    current_tool_use: Option<(usize, String)>,
}

impl AnthropicStreamUpstreamMessage {
    fn capture_message_start(&mut self, parsed: &serde_json::Value) {
        self.message.id = parsed
            .pointer("/message/id")
            .and_then(|value| value.as_str())
            .map(str::to_string);
        self.message.message_type = parsed
            .pointer("/message/type")
            .and_then(|value| value.as_str())
            .map(str::to_string);
        self.message.role = parsed
            .pointer("/message/role")
            .and_then(|value| value.as_str())
            .map(str::to_string);
        self.message.model = parsed
            .pointer("/message/model")
            .and_then(|value| value.as_str())
            .map(str::to_string);
        self.message.container = parsed
            .pointer("/message/container")
            .and_then(extract_completion_container_info);
    }

    fn capture_content_block_start(&mut self, content_block: &serde_json::Value) {
        self.current_text_block = None;
        self.current_thinking_block = None;
        self.current_tool_use = None;
        match content_block.get("type").and_then(|value| value.as_str()) {
            Some("text") => {
                self.message
                    .content_blocks
                    .push(CompletionUpstreamContentBlock {
                        block_type: "text".to_string(),
                        id: None,
                        name: None,
                        text: Some(String::new()),
                        thinking: None,
                        signature: None,
                        input_json: None,
                    });
                self.current_text_block = Some(self.message.content_blocks.len() - 1);
            }
            Some("thinking") => {
                self.message
                    .content_blocks
                    .push(CompletionUpstreamContentBlock {
                        block_type: "thinking".to_string(),
                        id: None,
                        name: None,
                        text: None,
                        thinking: Some(String::new()),
                        signature: content_block
                            .get("signature")
                            .and_then(|value| value.as_str())
                            .map(str::to_string),
                        input_json: None,
                    });
                self.current_thinking_block = Some(self.message.content_blocks.len() - 1);
            }
            Some("tool_use") => {
                self.message
                    .content_blocks
                    .push(CompletionUpstreamContentBlock {
                        block_type: "tool_use".to_string(),
                        id: content_block
                            .get("id")
                            .and_then(|value| value.as_str())
                            .map(str::to_string),
                        name: content_block
                            .get("name")
                            .and_then(|value| value.as_str())
                            .map(str::to_string),
                        text: None,
                        thinking: None,
                        signature: None,
                        input_json: None,
                    });
                self.current_tool_use = Some((self.message.content_blocks.len() - 1, String::new()));
            }
            _ => {}
        }
    }

    fn capture_content_block_delta(&mut self, delta: &serde_json::Value) {
        if let Some(index) = self.current_text_block {
            if let Some(text) = delta.get("text").and_then(|value| value.as_str()) {
                if let Some(current) = self
                    .message
                    .content_blocks
                    .get_mut(index)
                    .and_then(|value| value.text.as_mut())
                {
                    current.push_str(text);
                }
            }
        }
        if let Some(index) = self.current_thinking_block {
            if let Some(thinking) = delta.get("thinking").and_then(|value| value.as_str()) {
                if let Some(current) = self
                    .message
                    .content_blocks
                    .get_mut(index)
                    .and_then(|value| value.thinking.as_mut())
                {
                    current.push_str(thinking);
                }
            }
        }
        if let Some((_, partial_json)) = self.current_tool_use.as_mut() {
            if let Some(fragment) = delta.get("partial_json").and_then(|value| value.as_str()) {
                partial_json.push_str(fragment);
            }
        }
    }

    fn finish_content_block(&mut self) {
        self.current_text_block = None;
        self.current_thinking_block = None;
        let Some((index, partial_json)) = self.current_tool_use.take() else {
            return;
        };
        let Ok(input_json) = serde_json::from_str::<serde_json::Value>(&partial_json) else {
            return;
        };
        if let Some(block) = self.message.content_blocks.get_mut(index) {
            block.input_json = Some(input_json);
        }
    }

    fn build(&self, stop_metadata: &AnthropicStreamStopMetadata) -> Option<CompletionUpstreamMessage> {
        if self.message.id.is_none()
            && self.message.message_type.is_none()
            && self.message.role.is_none()
            && self.message.model.is_none()
            && self.message.container.is_none()
            && self.message.content_blocks.is_empty()
        {
            return None;
        }

        let mut message = self.message.clone();
        message.stop_reason = stop_metadata.stop_reason.clone();
        message.stop_sequence = stop_metadata.stop_sequence.clone();
        Some(message)
    }
}