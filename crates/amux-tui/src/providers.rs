//! Predefined LLM provider definitions.
//!
//! This mirrors the `PROVIDER_DEFINITIONS` from the frontend's `agentStore.ts`
//! so that the TUI has the same provider list with default base URLs and models.

use crate::state::config::FetchedModel;

pub struct ProviderDef {
    pub id: &'static str,
    pub name: &'static str,
    pub default_base_url: &'static str,
    pub default_model: &'static str,
    pub supported_transports: &'static [&'static str],
    pub default_transport: &'static str,
    pub supported_auth_sources: &'static [&'static str],
    pub default_auth_source: &'static str,
    pub native_base_url: Option<&'static str>,
}

pub const CHAT_ONLY_TRANSPORTS: &[&str] = &["chat_completions"];
pub const RESPONSES_AND_CHAT_TRANSPORTS: &[&str] = &["responses", "chat_completions"];
pub const NATIVE_AND_CHAT_TRANSPORTS: &[&str] = &["native_assistant", "chat_completions"];
pub const API_KEY_ONLY_AUTH_SOURCES: &[&str] = &["api_key"];
pub const OPENAI_AUTH_SOURCES: &[&str] = &["chatgpt_subscription", "api_key"];

pub const PROVIDERS: &[ProviderDef] = &[
    ProviderDef {
        id: "openai",
        name: "OpenAI / ChatGPT",
        default_base_url: "https://api.openai.com/v1",
        default_model: "gpt-5.4",
        supported_transports: RESPONSES_AND_CHAT_TRANSPORTS,
        default_transport: "responses",
        supported_auth_sources: OPENAI_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: "groq",
        name: "Groq",
        default_base_url: "https://api.groq.com/openai/v1",
        default_model: "llama-3.3-70b-versatile",
        supported_transports: RESPONSES_AND_CHAT_TRANSPORTS,
        default_transport: "responses",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: "ollama",
        name: "Ollama",
        default_base_url: "http://localhost:11434/v1",
        default_model: "llama3.1",
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: "chat_completions",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: "together",
        name: "Together",
        default_base_url: "https://api.together.xyz/v1",
        default_model: "meta-llama/Llama-3.3-70B-Instruct-Turbo",
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: "chat_completions",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: "openrouter",
        name: "OpenRouter",
        default_base_url: "https://openrouter.ai/api/v1",
        default_model: "anthropic/claude-sonnet-4",
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: "chat_completions",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: "cerebras",
        name: "Cerebras",
        default_base_url: "https://api.cerebras.ai/v1",
        default_model: "llama-3.3-70b",
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: "chat_completions",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: "qwen",
        name: "Qwen",
        default_base_url: "https://dashscope-intl.aliyuncs.com/compatible-mode/v1",
        default_model: "qwen-max",
        supported_transports: NATIVE_AND_CHAT_TRANSPORTS,
        default_transport: "native_assistant",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: Some("https://dashscope-intl.aliyuncs.com/api/v1"),
    },
    ProviderDef {
        id: "kimi",
        name: "Kimi (Moonshot)",
        default_base_url: "https://api.moonshot.ai/v1",
        default_model: "moonshot-v1-32k",
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: "chat_completions",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: "z.ai",
        name: "Z.AI (GLM)",
        default_base_url: "https://api.z.ai/api/paas/v4",
        default_model: "glm-4-plus",
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: "chat_completions",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: "z.ai-coding-plan",
        name: "Z.AI Coding Plan",
        default_base_url: "https://api.z.ai/api/coding/paas/v4",
        default_model: "glm-5",
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: "chat_completions",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: "kimi-coding-plan",
        name: "Kimi Coding Plan",
        default_base_url: "https://api.kimi.com/coding/v1",
        default_model: "kimi-for-coding",
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: "chat_completions",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: "minimax",
        name: "MiniMax",
        default_base_url: "https://api.minimax.io/anthropic",
        default_model: "MiniMax-M2.7",
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: "chat_completions",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: "minimax-coding-plan",
        name: "MiniMax Coding Plan",
        default_base_url: "https://api.minimax.io/anthropic",
        default_model: "MiniMax-M2.7",
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: "chat_completions",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: "alibaba-coding-plan",
        name: "Alibaba Coding Plan",
        default_base_url: "https://coding-intl.dashscope.aliyuncs.com/v1",
        default_model: "qwen3.5-plus",
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: "chat_completions",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: "qwen-deepinfra",
        name: "Qwen (DeepInfra)",
        default_base_url: "https://api.deepinfra.com/v1/openai",
        default_model: "Qwen/Qwen2.5-72B-Instruct",
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: "chat_completions",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: "huggingface",
        name: "Hugging Face",
        default_base_url: "https://api-inference.huggingface.co/v1",
        default_model: "meta-llama/Llama-3.3-70B-Instruct",
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: "chat_completions",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: "chutes",
        name: "Chutes",
        default_base_url: "https://llm.chutes.ai/v1",
        default_model: "deepseek-ai/DeepSeek-V3",
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: "chat_completions",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: "featherless",
        name: "Featherless",
        default_base_url: "https://api.featherless.ai/v1",
        default_model: "meta-llama/Llama-3.3-70B-Instruct",
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: "chat_completions",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: "opencode-zen",
        name: "OpenCode Zen",
        default_base_url: "https://opencode.ai/zen/v1",
        default_model: "claude-sonnet-4-5",
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: "chat_completions",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
    ProviderDef {
        id: "custom",
        name: "Custom",
        default_base_url: "",
        default_model: "",
        supported_transports: RESPONSES_AND_CHAT_TRANSPORTS,
        default_transport: "responses",
        supported_auth_sources: API_KEY_ONLY_AUTH_SOURCES,
        default_auth_source: "api_key",
        native_base_url: None,
    },
];

/// Find a provider definition by its id.
pub fn find_by_id(id: &str) -> Option<&'static ProviderDef> {
    PROVIDERS.iter().find(|p| p.id == id)
}

/// Find a provider definition by its display name.
#[allow(dead_code)]
pub fn find_by_name(name: &str) -> Option<&'static ProviderDef> {
    PROVIDERS.iter().find(|p| p.name == name)
}

/// Check whether a base URL matches any known provider default.
pub fn is_known_default_url(url: &str) -> bool {
    PROVIDERS.iter().any(|p| p.default_base_url == url)
}

pub fn supported_transports_for(provider: &str) -> &'static [&'static str] {
    find_by_id(provider)
        .map(|def| def.supported_transports)
        .unwrap_or(CHAT_ONLY_TRANSPORTS)
}

pub fn default_transport_for(provider: &str) -> &'static str {
    find_by_id(provider)
        .map(|def| def.default_transport)
        .unwrap_or("chat_completions")
}

pub fn uses_fixed_anthropic_messages(provider: &str, model: &str) -> bool {
    matches!(provider, "minimax" | "minimax-coding-plan")
        || (provider == "opencode-zen" && model.starts_with("claude"))
}

pub fn supported_auth_sources_for(provider: &str) -> &'static [&'static str] {
    find_by_id(provider)
        .map(|def| def.supported_auth_sources)
        .unwrap_or(API_KEY_ONLY_AUTH_SOURCES)
}

pub fn default_auth_source_for(provider: &str) -> &'static str {
    find_by_id(provider)
        .map(|def| def.default_auth_source)
        .unwrap_or("api_key")
}

pub fn default_model_for_provider_auth(provider: &str, auth_source: &str) -> String {
    known_models_for_provider_auth(provider, auth_source)
        .first()
        .map(|model| model.id.clone())
        .unwrap_or_else(|| {
            find_by_id(provider)
                .map(|def| def.default_model.to_string())
                .unwrap_or_default()
        })
}

/// Return a hardcoded list of known models for the given provider so the model
/// picker works without a live daemon fetch.
pub fn known_models_for_provider(provider: &str) -> Vec<FetchedModel> {
    known_models_for_provider_auth(provider, "api_key")
}

pub fn known_models_for_provider_auth(provider: &str, auth_source: &str) -> Vec<FetchedModel> {
    let models: &[(&str, &str, u32)] = match provider {
        "openai" if auth_source == "chatgpt_subscription" => &[
            ("gpt-5.4", "GPT-5.4", 1_000_000),
            ("gpt-5.4-mini", "GPT-5.4 Mini", 400_000),
            ("gpt-5.3-codex", "GPT-5.3 Codex", 400_000),
            ("gpt-5.2-codex", "GPT-5.2 Codex", 400_000),
            ("gpt-5.2", "GPT-5.2", 400_000),
            ("gpt-5.1-codex-max", "GPT-5.1 Codex Max", 400_000),
            ("gpt-5.1-codex-mini", "GPT-5.1 Codex Mini", 400_000),
        ],
        "openai" => &[
            ("gpt-5.4", "GPT-5.4", 1_000_000),
            ("gpt-5.4-mini", "GPT-5.4 Mini", 400_000),
            ("gpt-5.4-nano", "GPT-5.4 Nano", 400_000),
            ("gpt-5.3-codex", "GPT-5.3 Codex", 400_000),
            ("gpt-5.2-codex", "GPT-5.2 Codex", 400_000),
            ("gpt-5.2", "GPT-5.2", 400_000),
            ("gpt-5.1-codex-max", "GPT-5.1 Codex Max", 400_000),
            ("gpt-5.1-codex", "GPT-5.1 Codex", 400_000),
            ("gpt-5.1-codex-mini", "GPT-5.1 Codex Mini", 400_000),
            ("gpt-5.1", "GPT-5.1", 400_000),
            ("gpt-5-codex", "GPT-5 Codex", 400_000),
            ("gpt-5-codex-mini", "GPT-5 Codex Mini", 200_000),
            ("gpt-5", "GPT-5", 400_000),
            ("codex-mini-latest", "Codex Mini Latest", 200_000),
            ("o4-mini", "o4 Mini", 200_000),
            ("o3", "o3", 200_000),
            ("gpt-4.1", "GPT-4.1", 1_000_000),
            ("gpt-4.1-mini", "GPT-4.1 Mini", 1_000_000),
            ("gpt-4.1-nano", "GPT-4.1 Nano", 1_000_000),
            ("gpt-4o", "GPT-4o", 128_000),
            ("gpt-4o-mini", "GPT-4o Mini", 128_000),
        ],
        "groq" => &[
            ("llama-3.3-70b-versatile", "Llama 3.3 70B", 128_000),
            ("llama-3.1-8b-instant", "Llama 3.1 8B", 131_072),
            ("gemma2-9b-it", "Gemma 2 9B", 8_192),
        ],
        "ollama" => &[
            ("llama3.3", "Llama 3.3", 128_000),
            ("qwen2.5-coder", "Qwen 2.5 Coder", 32_768),
            ("deepseek-r1", "DeepSeek R1", 64_000),
            ("mistral", "Mistral", 32_768),
        ],
        "together" => &[
            (
                "meta-llama/Llama-3.3-70B-Instruct-Turbo",
                "Llama 3.3 70B",
                128_000,
            ),
            ("deepseek-ai/DeepSeek-R1", "DeepSeek R1", 64_000),
            ("Qwen/Qwen2.5-72B-Instruct-Turbo", "Qwen 2.5 72B", 32_768),
        ],
        "deepinfra" => &[
            (
                "meta-llama/Llama-3.3-70B-Instruct",
                "Llama 3.3 70B",
                128_000,
            ),
            (
                "Qwen/Qwen2.5-Coder-32B-Instruct",
                "Qwen 2.5 Coder 32B",
                32_768,
            ),
        ],
        "z.ai" | "z.ai-coding-plan" => &[
            ("glm-4.7", "GLM-4.7", 128_000),
            ("glm-4.7-air", "GLM-4.7 Air", 128_000),
            ("glm-4.7-flash", "GLM-4.7 Flash", 128_000),
            ("glm-5", "GLM-5", 128_000),
        ],
        "kimi" => &[
            ("kimi-k2.5", "Kimi K2.5", 262_144),
            ("kimi-for-coding", "Kimi for Coding", 128_000),
        ],
        "kimi-coding-plan" => &[
            ("kimi-k2.5", "Kimi K2.5", 262_144),
            ("kimi-for-coding", "Kimi for Coding", 128_000),
        ],
        "qwen" => &[
            ("qwen-max", "Qwen Max", 32_768),
            ("qwen-plus", "Qwen Plus", 131_072),
            ("qwen-turbo", "Qwen Turbo", 131_072),
        ],
        "openrouter" => &[
            ("anthropic/claude-opus-4-6", "Claude Opus 4.6", 1_000_000),
            ("openai/gpt-4.1", "GPT-4.1", 1_000_000),
            ("google/gemini-2.5-pro", "Gemini 2.5 Pro", 1_000_000),
            (
                "meta-llama/llama-3.3-70b-instruct",
                "Llama 3.3 70B",
                128_000,
            ),
        ],
        "cerebras" => &[("llama-3.3-70b", "Llama 3.3 70B", 128_000)],
        "minimax" => &[
            ("MiniMax-M2.7", "MiniMax M2.7", 205_000),
            ("MiniMax-M2.5", "MiniMax M2.5", 205_000),
        ],
        "minimax-coding-plan" => &[
            ("MiniMax-M2.7", "MiniMax M2.7", 205_000),
            ("MiniMax-M2.5", "MiniMax M2.5", 205_000),
        ],
        "alibaba-coding-plan" => &[
            ("qwen3-coder-plus", "Qwen3 Coder Plus", 997_952),
            ("qwen3-coder-next", "Qwen3 Coder Next", 204_800),
            ("qwen3.5-plus", "Qwen3.5 Plus", 983_616),
            ("glm-5", "GLM-5", 202_752),
            ("kimi-k2.5", "Kimi K2.5", 262_144),
            ("MiniMax-M2.5", "MiniMax M2.5", 205_000),
        ],
        "huggingface" => &[(
            "meta-llama/Llama-3.3-70B-Instruct",
            "Llama 3.3 70B",
            128_000,
        )],
        "chutes" => &[("deepseek-ai/DeepSeek-V3", "DeepSeek V3", 128_000)],
        _ => &[],
    };
    models
        .iter()
        .map(|(id, name, ctx)| FetchedModel {
            id: id.to_string(),
            name: Some(name.to_string()),
            context_window: Some(*ctx),
        })
        .collect()
}

pub fn known_context_window_for(provider: &str, model: &str) -> Option<u32> {
    known_models_for_provider_auth(provider, "api_key")
        .into_iter()
        .find(|entry| entry.id == model)
        .and_then(|entry| entry.context_window)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_count_is_20() {
        assert_eq!(PROVIDERS.len(), 20);
    }

    #[test]
    fn find_by_id_works() {
        let p = find_by_id("qwen").unwrap();
        assert_eq!(p.name, "Qwen");
        assert_eq!(
            p.default_base_url,
            "https://dashscope-intl.aliyuncs.com/compatible-mode/v1"
        );
    }

    #[test]
    fn alibaba_coding_plan_uses_openai_compatible_base_url() {
        let p = find_by_id("alibaba-coding-plan").unwrap();
        assert_eq!(
            p.default_base_url,
            "https://coding-intl.dashscope.aliyuncs.com/v1"
        );
        assert_eq!(p.default_model, "qwen3.5-plus");
    }

    #[test]
    fn anthropic_message_providers_are_detected() {
        assert!(uses_fixed_anthropic_messages("minimax", "MiniMax-M2.7"));
        assert!(!uses_fixed_anthropic_messages("qwen", "qwen-max"));
        assert!(!uses_fixed_anthropic_messages(
            "alibaba-coding-plan",
            "qwen3.5-plus"
        ));
    }

    #[test]
    fn find_by_name_works() {
        let p = find_by_name("OpenAI / ChatGPT").unwrap();
        assert_eq!(p.id, "openai");
    }

    #[test]
    fn is_known_default_url_returns_true() {
        assert!(is_known_default_url("https://api.openai.com/v1"));
        assert!(!is_known_default_url("https://custom.example.com/v1"));
    }

    #[test]
    fn known_models_openai_non_empty() {
        let models = known_models_for_provider("openai");
        assert!(!models.is_empty());
        assert!(models.iter().any(|m| m.id == "gpt-5.4"));
    }

    #[test]
    fn known_models_openai_chatgpt_subscription_is_restricted() {
        let models = known_models_for_provider_auth("openai", "chatgpt_subscription");
        assert!(models.iter().any(|m| m.id == "gpt-5.4"));
        assert!(!models.iter().any(|m| m.id == "gpt-4o"));
        assert!(!models.iter().any(|m| m.id == "o3"));
    }

    #[test]
    fn known_models_unknown_returns_empty() {
        let models = known_models_for_provider("nonexistent");
        assert!(models.is_empty());
    }
}
