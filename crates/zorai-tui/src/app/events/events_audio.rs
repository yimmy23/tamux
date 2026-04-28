pub(super) fn text_to_speech_result_path(
    name: &str,
    content: &str,
    is_error: bool,
) -> Option<String> {
    if is_error || name != "text_to_speech" {
        return None;
    }

    serde_json::from_str::<serde_json::Value>(content)
        .ok()
        .and_then(|value| {
            value
                .get("path")
                .and_then(|value| value.as_str())
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        })
}

#[cfg(test)]
mod tests {
    use super::text_to_speech_result_path;

    #[test]
    fn text_to_speech_result_path_reads_successful_tool_payloads() {
        let path = text_to_speech_result_path(
            "text_to_speech",
            r#"{"path":"/tmp/speech.mp3","mime_type":"audio/mpeg"}"#,
            false,
        );

        assert_eq!(path.as_deref(), Some("/tmp/speech.mp3"));
    }

    #[test]
    fn text_to_speech_result_path_ignores_non_tts_results_and_errors() {
        assert!(
            text_to_speech_result_path("bash_command", r#"{"path":"/tmp/speech.mp3"}"#, false)
                .is_none()
        );
        assert!(text_to_speech_result_path(
            "text_to_speech",
            r#"{"path":"/tmp/speech.mp3"}"#,
            true
        )
        .is_none());
        assert!(text_to_speech_result_path(
            "text_to_speech",
            r#"{"mime_type":"audio/mpeg"}"#,
            false
        )
        .is_none());
    }
}
