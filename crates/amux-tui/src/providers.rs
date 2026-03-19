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
}

pub const PROVIDERS: &[ProviderDef] = &[
    ProviderDef { id: "openai", name: "OpenAI", default_base_url: "https://api.openai.com/v1", default_model: "gpt-5.4" },
    ProviderDef { id: "anthropic", name: "Anthropic", default_base_url: "https://api.anthropic.com", default_model: "claude-sonnet-4-20250514" },
    ProviderDef { id: "groq", name: "Groq", default_base_url: "https://api.groq.com/openai/v1", default_model: "llama-3.3-70b-versatile" },
    ProviderDef { id: "ollama", name: "Ollama", default_base_url: "http://localhost:11434/v1", default_model: "llama3.1" },
    ProviderDef { id: "together", name: "Together", default_base_url: "https://api.together.xyz/v1", default_model: "meta-llama/Llama-3.3-70B-Instruct-Turbo" },
    ProviderDef { id: "openrouter", name: "OpenRouter", default_base_url: "https://openrouter.ai/api/v1", default_model: "anthropic/claude-sonnet-4" },
    ProviderDef { id: "cerebras", name: "Cerebras", default_base_url: "https://api.cerebras.ai/v1", default_model: "llama-3.3-70b" },
    ProviderDef { id: "qwen", name: "Qwen", default_base_url: "https://api.qwen.com/v1", default_model: "qwen-max" },
    ProviderDef { id: "kimi", name: "Kimi (Moonshot)", default_base_url: "https://api.moonshot.ai/v1", default_model: "moonshot-v1-32k" },
    ProviderDef { id: "z.ai", name: "Z.AI (GLM)", default_base_url: "https://api.z.ai/api/paas/v4", default_model: "glm-4-plus" },
    ProviderDef { id: "z.ai-coding-plan", name: "Z.AI Coding Plan", default_base_url: "https://api.z.ai/api/coding/paas/v4", default_model: "glm-5" },
    ProviderDef { id: "kimi-coding-plan", name: "Kimi Coding Plan", default_base_url: "https://api.kimi.com/coding/v1", default_model: "kimi-for-coding" },
    ProviderDef { id: "minimax", name: "MiniMax", default_base_url: "https://api.minimax.io/anthropic", default_model: "MiniMax-M1-80k" },
    ProviderDef { id: "deepinfra", name: "DeepInfra", default_base_url: "https://api.deepinfra.com/v1/openai", default_model: "Qwen/Qwen2.5-72B-Instruct" },
    ProviderDef { id: "huggingface", name: "Hugging Face", default_base_url: "https://api-inference.huggingface.co/v1", default_model: "meta-llama/Llama-3.3-70B-Instruct" },
    ProviderDef { id: "chutes", name: "Chutes", default_base_url: "https://llm.chutes.ai/v1", default_model: "deepseek-ai/DeepSeek-V3" },
    ProviderDef { id: "custom", name: "Custom", default_base_url: "", default_model: "" },
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

/// Return a hardcoded list of known models for the given provider so the model
/// picker works without a live daemon fetch.
pub fn known_models_for_provider(provider: &str) -> Vec<FetchedModel> {
    let models: &[(&str, &str, u32)] = match provider {
        "openai" => &[
            ("gpt-5.4", "GPT-5.4", 1_000_000),
            ("gpt-5.4-mini", "GPT-5.4 Mini", 400_000),
            ("gpt-5.4-nano", "GPT-5.4 Nano", 400_000),
            ("o4-mini", "o4 Mini", 200_000),
            ("o3", "o3", 200_000),
            ("gpt-4.1", "GPT-4.1", 1_000_000),
            ("gpt-4.1-mini", "GPT-4.1 Mini", 1_000_000),
            ("gpt-4.1-nano", "GPT-4.1 Nano", 1_000_000),
            ("gpt-4o", "GPT-4o", 128_000),
            ("gpt-4o-mini", "GPT-4o Mini", 128_000),
        ],
        "anthropic" => &[
            ("claude-opus-4-6", "Claude Opus 4.6", 1_000_000),
            ("claude-sonnet-4-6", "Claude Sonnet 4.6", 200_000),
            ("claude-haiku-4-5-20251001", "Claude Haiku 4.5", 200_000),
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
            ("meta-llama/Llama-3.3-70B-Instruct-Turbo", "Llama 3.3 70B", 128_000),
            ("deepseek-ai/DeepSeek-R1", "DeepSeek R1", 64_000),
            ("Qwen/Qwen2.5-72B-Instruct-Turbo", "Qwen 2.5 72B", 32_768),
        ],
        "deepinfra" => &[
            ("meta-llama/Llama-3.3-70B-Instruct", "Llama 3.3 70B", 128_000),
            ("Qwen/Qwen2.5-Coder-32B-Instruct", "Qwen 2.5 Coder 32B", 32_768),
        ],
        "z.ai" | "z.ai-coding-plan" => &[
            ("glm-4-plus", "GLM-4 Plus", 128_000),
            ("glm-4-air", "GLM-4 Air", 128_000),
            ("glm-4-flash", "GLM-4 Flash", 128_000),
            ("glm-5", "GLM-5", 128_000),
        ],
        "kimi" => &[
            ("moonshot-v1-128k", "Moonshot v1 128K", 128_000),
            ("moonshot-v1-32k", "Moonshot v1 32K", 32_768),
        ],
        "kimi-coding-plan" => &[
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
            ("meta-llama/llama-3.3-70b-instruct", "Llama 3.3 70B", 128_000),
        ],
        "cerebras" => &[
            ("llama-3.3-70b", "Llama 3.3 70B", 128_000),
        ],
        "minimax" => &[
            ("MiniMax-M1-80k", "MiniMax M1 80K", 80_000),
        ],
        "huggingface" => &[
            ("meta-llama/Llama-3.3-70B-Instruct", "Llama 3.3 70B", 128_000),
        ],
        "chutes" => &[
            ("deepseek-ai/DeepSeek-V3", "DeepSeek V3", 128_000),
        ],
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provider_count_is_17() {
        assert_eq!(PROVIDERS.len(), 17);
    }

    #[test]
    fn find_by_id_works() {
        let p = find_by_id("anthropic").unwrap();
        assert_eq!(p.name, "Anthropic");
        assert_eq!(p.default_base_url, "https://api.anthropic.com");
    }

    #[test]
    fn find_by_name_works() {
        let p = find_by_name("OpenAI").unwrap();
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
    fn known_models_unknown_returns_empty() {
        let models = known_models_for_provider("nonexistent");
        assert!(models.is_empty());
    }
}
