use super::*;
use amux_shared::providers::{
    MINIMAX_PROVIDER, PROVIDER_ID_ALIBABA_CODING_PLAN, PROVIDER_ID_ARCEE,
    PROVIDER_ID_GITHUB_COPILOT, PROVIDER_ID_NVIDIA, PROVIDER_ID_OPENAI, QWEN_PROVIDER,
};

#[test]
fn provider_count_is_23() {
    assert_eq!(PROVIDERS.len(), 23);
}

#[test]
fn shared_provider_refs_match_tui_catalog() {
    let provider = find_by_id(QWEN_PROVIDER.id).unwrap();
    assert_eq!(provider.id, QWEN_PROVIDER.id);
    assert!(uses_fixed_anthropic_messages(
        MINIMAX_PROVIDER.id,
        "MiniMax-M2.7"
    ));
}

#[test]
fn find_by_id_works() {
    let p = find_by_id(QWEN_PROVIDER.id).unwrap();
    assert_eq!(p.name, "Qwen");
    assert_eq!(
        p.default_base_url,
        "https://dashscope-intl.aliyuncs.com/compatible-mode/v1"
    );
}

#[test]
fn alibaba_coding_plan_uses_openai_compatible_base_url() {
    let p = find_by_id(PROVIDER_ID_ALIBABA_CODING_PLAN).unwrap();
    assert_eq!(
        p.default_base_url,
        "https://coding-intl.dashscope.aliyuncs.com/v1"
    );
    assert_eq!(p.default_model, "qwen3.6-plus");
}

#[test]
fn anthropic_message_providers_are_detected() {
    assert!(uses_fixed_anthropic_messages(
        MINIMAX_PROVIDER.id,
        "MiniMax-M2.7"
    ));
    assert!(!uses_fixed_anthropic_messages(QWEN_PROVIDER.id, "qwen-max"));
    assert!(!uses_fixed_anthropic_messages(
        PROVIDER_ID_ALIBABA_CODING_PLAN,
        "qwen3.6-plus"
    ));
}

#[test]
fn find_by_name_works() {
    let p = find_by_name("OpenAI / ChatGPT").unwrap();
    assert_eq!(p.id, PROVIDER_ID_OPENAI);
}

#[test]
fn is_known_default_url_returns_true() {
    assert!(is_known_default_url("https://api.openai.com/v1"));
    assert!(!is_known_default_url("https://custom.example.com/v1"));
}

#[test]
fn known_models_openai_non_empty() {
    let models = known_models_for_provider(PROVIDER_ID_OPENAI);
    assert!(!models.is_empty());
    assert!(models.iter().any(|m| m.id == "gpt-5.4"));
}

#[test]
fn known_models_openai_chatgpt_subscription_is_restricted() {
    let models = known_models_for_provider_auth(PROVIDER_ID_OPENAI, "chatgpt_subscription");
    assert!(models.iter().any(|m| m.id == "gpt-5.4"));
    assert!(!models.iter().any(|m| m.id == "gpt-4o"));
    assert!(!models.iter().any(|m| m.id == "o3"));
}

#[test]
fn known_models_unknown_returns_empty() {
    let models = known_models_for_provider("nonexistent");
    assert!(models.is_empty());
}

#[test]
fn github_copilot_supports_browser_and_token_auth() {
    let provider = find_by_id(PROVIDER_ID_GITHUB_COPILOT).unwrap();
    assert_eq!(provider.default_auth_source, "github_copilot");
    assert_eq!(provider.supported_auth_sources, GITHUB_COPILOT_AUTH_SOURCES);
    assert_eq!(provider.default_transport, "responses");
    assert_eq!(provider.supported_transports, RESPONSES_AND_CHAT_TRANSPORTS);
}

#[test]
fn known_models_github_copilot_matches_static_catalog() {
    let models = known_models_for_provider_auth(PROVIDER_ID_GITHUB_COPILOT, "github_copilot");
    assert!(models.len() > 10);
    assert!(models.iter().any(|model| model.id == "gpt-4.1"));
    assert!(models.iter().any(|model| model.id == "gpt-5.4-mini"));
    assert!(models.iter().any(|model| model.id == "claude-sonnet-4.6"));
    assert!(models.iter().any(|model| model.id == "raptor-mini"));
    assert!(models.iter().any(|model| model.id == "goldeneye"));
}

#[test]
fn custom_model_name_can_resolve_known_provider_context_window() {
    assert_eq!(
        resolve_context_window_for_provider_auth(
            PROVIDER_ID_GITHUB_COPILOT,
            "github_copilot",
            "totally-custom-runtime-id",
            "Raptor mini (Preview)",
        ),
        Some(264_000)
    );
}

#[test]
fn unknown_custom_model_uses_264k_default_context_window() {
    assert_eq!(default_custom_model_context_window(), 264_000);
}

#[test]
fn arcee_provider_uses_expected_defaults() {
    let provider = find_by_id(PROVIDER_ID_ARCEE).unwrap();
    assert_eq!(provider.name, "Arcee");
    assert_eq!(provider.default_base_url, "https://api.arcee.ai/api/v1");
    assert_eq!(provider.default_model, "trinity-large-thinking");
    assert_eq!(provider.default_auth_source, "api_key");
    assert_eq!(provider.supported_auth_sources, API_KEY_ONLY_AUTH_SOURCES);
    assert_eq!(provider.default_transport, "chat_completions");
    assert_eq!(provider.supported_transports, CHAT_ONLY_TRANSPORTS);
    assert_eq!(
        known_context_window_for(PROVIDER_ID_ARCEE, "trinity-large-thinking"),
        Some(256_000)
    );
    assert!(supports_model_fetch_for(PROVIDER_ID_ARCEE));
}

#[test]
fn nvidia_provider_uses_expected_defaults() {
    let provider = find_by_id(PROVIDER_ID_NVIDIA).unwrap();
    assert_eq!(provider.name, "NVIDIA");
    assert_eq!(provider.default_base_url, "https://integrate.api.nvidia.com/v1");
    assert_eq!(provider.default_model, "minimaxai/minimax-m2.7");
    assert_eq!(provider.default_auth_source, "api_key");
    assert_eq!(provider.supported_auth_sources, API_KEY_ONLY_AUTH_SOURCES);
    assert_eq!(provider.default_transport, "chat_completions");
    assert_eq!(provider.supported_transports, CHAT_ONLY_TRANSPORTS);
    assert_eq!(
        known_context_window_for(PROVIDER_ID_NVIDIA, "minimaxai/minimax-m2.7"),
        Some(205_000)
    );
    assert!(supports_model_fetch_for(PROVIDER_ID_NVIDIA));
}
