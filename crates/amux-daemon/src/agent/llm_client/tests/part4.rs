#[test]
fn retry_delay_scales_with_attempt_multiplier() {
    assert_eq!(compute_retry_delay_ms_for_attempt(5_000, 1), 5_000);
    assert_eq!(compute_retry_delay_ms_for_attempt(5_000, 2), 10_000);
    assert_eq!(compute_retry_delay_ms_for_attempt(5_000, 3), 15_000);
}

#[test]
fn retry_delay_caps_at_one_minute() {
    assert_eq!(compute_retry_delay_ms_for_attempt(5_000, 20), 60_000);
}

#[test]
fn minimax_anthropic_requests_force_http11_and_connection_close() {
    let client = reqwest::Client::new();
    let config = ProviderConfig {
        base_url: "https://api.minimax.io/anthropic".to_string(),
        model: "MiniMax-M1".to_string(),
        api_key: "secret".to_string(),
        assistant_id: String::new(),
        auth_source: AuthSource::ApiKey,
        api_transport: ApiTransport::ChatCompletions,
        reasoning_effort: "medium".to_string(),
        context_window_tokens: 0,
        response_schema: None,
    };
    let request = build_anthropic_request(
        &client,
        "minimax-coding-plan",
        &config,
        "system",
        &[ApiMessage {
            role: "user".to_string(),
            content: ApiContent::Text("hello".to_string()),
            tool_call_id: None,
            name: None,
            tool_calls: None,
        }],
        &[],
    )
    .expect("request should build");

    assert_eq!(request.version(), reqwest::Version::HTTP_11);
    assert_eq!(
        request
            .headers()
            .get(reqwest::header::CONNECTION)
            .and_then(|value| value.to_str().ok()),
        Some("close")
    );
    assert_eq!(
        request
            .headers()
            .get("anthropic-beta")
            .and_then(|value| value.to_str().ok()),
        Some("fine-grained-tool-streaming-2025-05-14,interleaved-thinking-2025-05-14")
    );
}
