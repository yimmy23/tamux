#[derive(Debug, Clone, Default)]
struct AnthropicStreamStopMetadata {
    stop_reason: Option<String>,
    stop_sequence: Option<String>,
}

impl AnthropicStreamStopMetadata {
    fn capture_message_delta(&mut self, parsed: &serde_json::Value) {
        self.stop_reason = parsed
            .pointer("/delta/stop_reason")
            .and_then(|value| value.as_str())
            .map(str::to_string)
            .or_else(|| self.stop_reason.clone());
        self.stop_sequence = parsed
            .pointer("/delta/stop_sequence")
            .and_then(|value| value.as_str())
            .map(str::to_string)
            .or_else(|| self.stop_sequence.clone());
    }
}