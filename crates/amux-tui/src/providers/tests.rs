use super::*;
use amux_shared::providers::{
    PROVIDER_ID_ALIBABA_CODING_PLAN, PROVIDER_ID_GITHUB_COPILOT, PROVIDER_ID_OPENAI,
    MINIMAX_PROVIDER, QWEN_PROVIDER,
};

#[test]
fn provider_count_is_21() {
    assert_eq!(PROVIDERS.len(), 21);
}

#[test]
fn shared_provider_refs_match_tui_catalog() {
    let provider = find_by_id(QWEN_PROVIDER.id).unwrap();
    assert_eq!(provider.id, QWEN_PROVIDER.id);
    assert!(uses_fixed_anthropic_messages(MINIMAX_PROVIDER.id, "MiniMax-M2.7"));
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
    assert_eq!(p.default_model, "qwen3.5-plus");
}

#[test]
fn anthropic_message_providers_are_detected() {
    assert!(uses_fixed_anthropic_messages(MINIMAX_PROVIDER.id, "MiniMax-M2.7"));
    assert!(!uses_fixed_anthropic_messages(QWEN_PROVIDER.id, "qwen-max"));
    assert!(!uses_fixed_anthropic_messages(
        PROVIDER_ID_ALIBABA_CODING_PLAN,
        "qwen3.5-plus"
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
