use amux_protocol::{SecurityLevel, AGENT_NAME_RAROG, AGENT_NAME_SWAROG};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use super::capability_tier::TierConfig;
pub type WhatsAppLinkRuntimeEvent = super::whatsapp_link::WhatsAppLinkEvent;

// ---------------------------------------------------------------------------
// Provider definitions (static registry)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiType {
    OpenAI,
    Anthropic,
}

impl ApiType {
    /// The SDK-style User-Agent string used by coding-plan providers.
    pub fn sdk_user_agent(self) -> &'static str {
        match self {
            Self::Anthropic => "Anthropic/JS tamux",
            Self::OpenAI => "OpenAI/JS tamux",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ApiTransport {
    NativeAssistant,
    #[default]
    Responses,
    ChatCompletions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AuthSource {
    #[default]
    ApiKey,
    ChatgptSubscription,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeTransportKind {
    AlibabaAssistantApi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthMethod {
    Bearer,
    XApiKey,
}

impl AuthMethod {
    /// Apply the appropriate auth header to a request builder.
    pub fn apply(self, req: reqwest::RequestBuilder, api_key: &str) -> reqwest::RequestBuilder {
        match self {
            Self::Bearer => req.header("Authorization", format!("Bearer {}", api_key)),
            Self::XApiKey => req.header("x-api-key", api_key),
        }
    }
}

/// Input modalities a model supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Modality {
    Text,
    Image,
    Video,
    Audio,
}

/// Shorthand constants for common modality sets.
pub const TEXT_ONLY: &[Modality] = &[Modality::Text];
pub const TEXT_IMAGE: &[Modality] = &[Modality::Text, Modality::Image];
pub const TEXT_IMAGE_AUDIO: &[Modality] = &[Modality::Text, Modality::Image, Modality::Audio];
pub const MULTIMODAL: &[Modality] = &[
    Modality::Text,
    Modality::Image,
    Modality::Video,
    Modality::Audio,
];

/// Look up the modalities for a model by provider and model ID.
/// Returns TEXT_ONLY if the model is not in the known list.
pub fn model_modalities(provider_id: &str, model_id: &str) -> &'static [Modality] {
    get_provider_definition(provider_id)
        .and_then(|def| def.models.iter().find(|m| m.id == model_id))
        .map(|m| m.modalities)
        .unwrap_or(TEXT_ONLY)
}

/// Check if a model supports a specific modality.
pub fn model_supports(provider_id: &str, model_id: &str, modality: Modality) -> bool {
    model_modalities(provider_id, model_id).contains(&modality)
}

/// Return model names from the same provider that support the given modality.
pub fn models_supporting(provider_id: &str, modality: Modality) -> Vec<&'static str> {
    get_provider_definition(provider_id)
        .map(|def| {
            def.models
                .iter()
                .filter(|m| m.modalities.contains(&modality))
                .map(|m| m.id)
                .collect()
        })
        .unwrap_or_default()
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelDefinition {
    pub id: &'static str,
    pub name: &'static str,
    pub context_window: u32,
    /// Input modalities this model accepts.
    pub modalities: &'static [Modality],
}

#[derive(Debug, Clone, Serialize)]
pub struct ProviderDefinition {
    pub id: &'static str,
    pub name: &'static str,
    pub default_base_url: &'static str,
    pub default_model: &'static str,
    pub api_type: ApiType,
    pub auth_method: AuthMethod,
    pub models: &'static [ModelDefinition],
    pub supports_model_fetch: bool,
    pub anthropic_base_url: Option<&'static str>,
    pub supported_transports: &'static [ApiTransport],
    pub default_transport: ApiTransport,
    pub native_transport_kind: Option<NativeTransportKind>,
    pub native_base_url: Option<&'static str>,
    pub supports_response_continuity: bool,
}

pub const OPENAI_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: "gpt-5.4",
        name: "GPT-5.4",
        context_window: 1_000_000,
        modalities: MULTIMODAL,
    },
    ModelDefinition {
        id: "gpt-5.4-mini",
        name: "GPT-5.4 Mini",
        context_window: 400000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "gpt-5.4-nano",
        name: "GPT-5.4 Nano",
        context_window: 400000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "gpt-5.3-codex",
        name: "GPT-5.3 Codex",
        context_window: 400000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "gpt-5.2-codex",
        name: "GPT-5.2 Codex",
        context_window: 400000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "gpt-5.2",
        name: "GPT-5.2",
        context_window: 400000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "gpt-5.1-codex-max",
        name: "GPT-5.1 Codex Max",
        context_window: 400000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "gpt-5.1-codex",
        name: "GPT-5.1 Codex",
        context_window: 400000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "gpt-5.1-codex-mini",
        name: "GPT-5.1 Codex Mini",
        context_window: 400000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "gpt-5.1",
        name: "GPT-5.1",
        context_window: 400000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "gpt-5-codex",
        name: "GPT-5 Codex",
        context_window: 400000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "gpt-5-codex-mini",
        name: "GPT-5 Codex Mini",
        context_window: 200000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "gpt-5",
        name: "GPT-5",
        context_window: 400000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "codex-mini-latest",
        name: "Codex Mini Latest",
        context_window: 200000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "o3",
        name: "o3",
        context_window: 200000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "o4-mini",
        name: "o4 Mini",
        context_window: 200000,
        modalities: TEXT_ONLY,
    },
];

pub const QWEN_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: "qwen-max",
        name: "Qwen Max",
        context_window: 32768,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "qwen-plus",
        name: "Qwen Plus",
        context_window: 32768,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "qwen-turbo",
        name: "Qwen Turbo",
        context_window: 8192,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "qwen-long",
        name: "Qwen Long",
        context_window: 1000000,
        modalities: TEXT_ONLY,
    },
];

pub const ZAI_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: "glm-5.1",
        name: "GLM-5.1",
        context_window: 204800,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "glm-5",
        name: "GLM-5",
        context_window: 128000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "glm-4-plus",
        name: "GLM-4 Plus",
        context_window: 128000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "glm-4",
        name: "GLM-4",
        context_window: 128000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "glm-4-air",
        name: "GLM-4 Air",
        context_window: 128000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "glm-4-flash",
        name: "GLM-4 Flash",
        context_window: 128000,
        modalities: TEXT_ONLY,
    },
];

pub const KIMI_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: "moonshot-v1-8k",
        name: "Moonshot V1 8K",
        context_window: 8192,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "moonshot-v1-32k",
        name: "Moonshot V1 32K",
        context_window: 32768,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "moonshot-v1-128k",
        name: "Moonshot V1 128K",
        context_window: 131072,
        modalities: TEXT_ONLY,
    },
];

pub const KIMI_CODING_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: "kimi-for-coding",
        name: "Kimi for Coding",
        context_window: 262144,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "kimi-k2.5",
        name: "Kimi K2.5",
        context_window: 262144,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "kimi-k2-turbo-preview",
        name: "Kimi K2 Turbo Preview",
        context_window: 262144,
        modalities: TEXT_ONLY,
    },
];

pub const MINIMAX_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: "MiniMax-M2.7",
        name: "MiniMax M2.7",
        context_window: 205000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "MiniMax-M2.5",
        name: "MiniMax M2.5",
        context_window: 205000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "MiniMax-M2.5-highspeed",
        name: "MiniMax M2.5 High Speed",
        context_window: 205000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "MiniMax-M1-80k",
        name: "MiniMax M1 80K",
        context_window: 80000,
        modalities: TEXT_ONLY,
    },
];

pub const ALIBABA_CODING_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: "qwen3-coder-plus",
        name: "Qwen3 Coder Plus",
        context_window: 997952,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "qwen3-coder-next",
        name: "Qwen3 Coder Next",
        context_window: 204800,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "qwen3.5-plus",
        name: "Qwen3.5 Plus",
        context_window: 983616,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "glm-5",
        name: "GLM-5",
        context_window: 202752,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "kimi-k2.5",
        name: "Kimi K2.5",
        context_window: 262144,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "MiniMax-M2.5",
        name: "MiniMax M2.5",
        context_window: 205000,
        modalities: TEXT_ONLY,
    },
];

pub const OPENCODE_ZEN_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: "claude-opus-4-6",
        name: "Claude Opus 4.6",
        context_window: 200000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "claude-sonnet-4-5",
        name: "Claude Sonnet 4.5",
        context_window: 200000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "claude-sonnet-4",
        name: "Claude Sonnet 4",
        context_window: 200000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "gpt-5.4",
        name: "GPT-5.4",
        context_window: 128000,
        modalities: MULTIMODAL,
    },
    ModelDefinition {
        id: "gpt-5.4-mini",
        name: "GPT-5.4 Mini",
        context_window: 128000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "gpt-5.3-codex",
        name: "GPT-5.3 Codex",
        context_window: 128000,
        modalities: TEXT_IMAGE,
    },
    ModelDefinition {
        id: "minimax-m2.5",
        name: "MiniMax M2.5",
        context_window: 205000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "glm-5",
        name: "GLM-5",
        context_window: 128000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "kimi-k2.5",
        name: "Kimi K2.5",
        context_window: 262144,
        modalities: TEXT_ONLY,
    },
];

pub const OPENROUTER_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: "anthropic/claude-sonnet-4",
        name: "Claude Sonnet 4",
        context_window: 200000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "anthropic/claude-3.5-sonnet",
        name: "Claude 3.5 Sonnet",
        context_window: 200000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "openai/gpt-4o",
        name: "GPT-4o",
        context_window: 128000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "google/gemini-pro-1.5",
        name: "Gemini Pro 1.5",
        context_window: 1000000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "meta-llama/llama-3.3-70b-instruct",
        name: "Llama 3.3 70B",
        context_window: 128000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "deepseek/deepseek-chat",
        name: "DeepSeek Chat",
        context_window: 64000,
        modalities: TEXT_ONLY,
    },
];

pub const GROQ_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: "llama-3.3-70b-versatile",
        name: "Llama 3.3 70B Versatile",
        context_window: 128000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "llama-3.3-70b-specdec",
        name: "Llama 3.3 70B Speculative",
        context_window: 8192,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "llama-3.1-8b-instant",
        name: "Llama 3.1 8B",
        context_window: 128000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "mixtral-8x7b-32768",
        name: "Mixtral 8x7B",
        context_window: 32768,
        modalities: TEXT_ONLY,
    },
];

pub const CEREBRAS_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: "llama-3.3-70b",
        name: "Llama 3.3 70B",
        context_window: 128000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "llama-3.1-8b",
        name: "Llama 3.1 8B",
        context_window: 128000,
        modalities: TEXT_ONLY,
    },
];

pub const TOGETHER_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: "meta-llama/Llama-3.3-70B-Instruct-Turbo",
        name: "Llama 3.3 70B Turbo",
        context_window: 128000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "meta-llama/Llama-3.2-90B-Vision-Instruct-Turbo",
        name: "Llama 3.2 90B Vision",
        context_window: 131072,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "Qwen/Qwen2.5-72B-Instruct-Turbo",
        name: "Qwen 2.5 72B Turbo",
        context_window: 32768,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "deepseek-ai/DeepSeek-V3",
        name: "DeepSeek V3",
        context_window: 128000,
        modalities: TEXT_ONLY,
    },
];

pub const OLLAMA_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: "llama3.1",
        name: "Llama 3.1",
        context_window: 128000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "llama3.2",
        name: "Llama 3.2",
        context_window: 128000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "qwen2.5",
        name: "Qwen 2.5",
        context_window: 128000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "codellama",
        name: "Code Llama",
        context_window: 16384,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "mistral",
        name: "Mistral",
        context_window: 32768,
        modalities: TEXT_ONLY,
    },
];

pub const CHUTES_MODELS: &[ModelDefinition] = &[ModelDefinition {
    id: "deepseek-ai/DeepSeek-V3",
    name: "DeepSeek V3",
    context_window: 128000,
    modalities: TEXT_ONLY,
}];

pub const HUGGINGFACE_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: "meta-llama/Llama-3.3-70B-Instruct",
        name: "Llama 3.3 70B",
        context_window: 128000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "Qwen/Qwen2.5-72B-Instruct",
        name: "Qwen 2.5 72B",
        context_window: 32768,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "mistralai/Mistral-7B-Instruct-v0.3",
        name: "Mistral 7B",
        context_window: 32768,
        modalities: TEXT_ONLY,
    },
];

pub const FEATHERLESS_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: "meta-llama/Llama-3.3-70B-Instruct",
        name: "Llama 3.3 70B",
        context_window: 128000,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "Qwen/Qwen2.5-72B-Instruct",
        name: "Qwen 2.5 72B",
        context_window: 32768,
        modalities: TEXT_ONLY,
    },
    ModelDefinition {
        id: "mistralai/Mistral-Small-24B-Instruct-2501",
        name: "Mistral Small 24B",
        context_window: 32768,
        modalities: TEXT_ONLY,
    },
];

pub const EMPTY_MODELS: &[ModelDefinition] = &[];
pub const CHAT_ONLY_TRANSPORTS: &[ApiTransport] = &[ApiTransport::ChatCompletions];
pub const RESPONSES_AND_CHAT_TRANSPORTS: &[ApiTransport] =
    &[ApiTransport::Responses, ApiTransport::ChatCompletions];
pub const NATIVE_AND_CHAT_TRANSPORTS: &[ApiTransport] =
    &[ApiTransport::NativeAssistant, ApiTransport::ChatCompletions];

pub const PROVIDER_DEFINITIONS: &[ProviderDefinition] = &[
    ProviderDefinition {
        id: "featherless",
        name: "Featherless",
        default_base_url: "https://api.featherless.ai/v1",
        default_model: "meta-llama/Llama-3.3-70B-Instruct",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::Bearer,
        models: FEATHERLESS_MODELS,
        supports_model_fetch: false,
        anthropic_base_url: None,
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: ApiTransport::ChatCompletions,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: "openai",
        name: "OpenAI",
        default_base_url: "https://api.openai.com/v1",
        default_model: "gpt-4o",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::Bearer,
        models: OPENAI_MODELS,
        supports_model_fetch: true,
        anthropic_base_url: None,
        supported_transports: RESPONSES_AND_CHAT_TRANSPORTS,
        default_transport: ApiTransport::Responses,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: true,
    },
    ProviderDefinition {
        id: "qwen",
        name: "Qwen",
        default_base_url: "https://dashscope-intl.aliyuncs.com/compatible-mode/v1",
        default_model: "qwen-max",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::Bearer,
        models: QWEN_MODELS,
        supports_model_fetch: true,
        anthropic_base_url: None,
        supported_transports: NATIVE_AND_CHAT_TRANSPORTS,
        default_transport: ApiTransport::NativeAssistant,
        native_transport_kind: Some(NativeTransportKind::AlibabaAssistantApi),
        native_base_url: Some("https://dashscope-intl.aliyuncs.com/api/v1"),
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: "qwen-deepinfra",
        name: "Qwen (DeepInfra)",
        default_base_url: "https://api.deepinfra.com/v1/openai",
        default_model: "Qwen/Qwen2.5-72B-Instruct",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::Bearer,
        models: QWEN_MODELS,
        supports_model_fetch: true,
        anthropic_base_url: None,
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: ApiTransport::ChatCompletions,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: "kimi",
        name: "Kimi (Moonshot)",
        default_base_url: "https://api.moonshot.ai/v1",
        default_model: "moonshot-v1-32k",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::Bearer,
        models: KIMI_MODELS,
        supports_model_fetch: true,
        anthropic_base_url: None,
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: ApiTransport::ChatCompletions,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: "kimi-coding-plan",
        name: "Kimi Coding Plan",
        default_base_url: "https://api.kimi.com/coding/v1",
        default_model: "kimi-for-coding",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::Bearer,
        models: KIMI_CODING_MODELS,
        supports_model_fetch: false,
        anthropic_base_url: None,
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: ApiTransport::ChatCompletions,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: "z.ai",
        name: "Z.AI (GLM)",
        default_base_url: "https://api.z.ai/api/paas/v4",
        default_model: "glm-4-plus",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::Bearer,
        models: ZAI_MODELS,
        supports_model_fetch: false,
        anthropic_base_url: None,
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: ApiTransport::ChatCompletions,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: "z.ai-coding-plan",
        name: "Z.AI Coding Plan",
        default_base_url: "https://api.z.ai/api/coding/paas/v4",
        default_model: "glm-5",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::Bearer,
        models: ZAI_MODELS,
        supports_model_fetch: false,
        anthropic_base_url: None,
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: ApiTransport::ChatCompletions,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: "openrouter",
        name: "OpenRouter",
        default_base_url: "https://openrouter.ai/api/v1",
        default_model: "anthropic/claude-sonnet-4",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::Bearer,
        models: OPENROUTER_MODELS,
        supports_model_fetch: true,
        anthropic_base_url: None,
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: ApiTransport::ChatCompletions,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: "cerebras",
        name: "Cerebras",
        default_base_url: "https://api.cerebras.ai/v1",
        default_model: "llama-3.3-70b",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::Bearer,
        models: CEREBRAS_MODELS,
        supports_model_fetch: true,
        anthropic_base_url: None,
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: ApiTransport::ChatCompletions,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: "together",
        name: "Together",
        default_base_url: "https://api.together.xyz/v1",
        default_model: "meta-llama/Llama-3.3-70B-Instruct-Turbo",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::Bearer,
        models: TOGETHER_MODELS,
        supports_model_fetch: true,
        anthropic_base_url: None,
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: ApiTransport::ChatCompletions,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: "groq",
        name: "Groq",
        default_base_url: "https://api.groq.com/openai/v1",
        default_model: "llama-3.3-70b-versatile",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::Bearer,
        models: GROQ_MODELS,
        supports_model_fetch: true,
        anthropic_base_url: None,
        supported_transports: RESPONSES_AND_CHAT_TRANSPORTS,
        default_transport: ApiTransport::Responses,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: "ollama",
        name: "Ollama",
        default_base_url: "http://localhost:11434/v1",
        default_model: "llama3.1",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::Bearer,
        models: OLLAMA_MODELS,
        supports_model_fetch: true,
        anthropic_base_url: None,
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: ApiTransport::ChatCompletions,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: "chutes",
        name: "Chutes",
        default_base_url: "https://llm.chutes.ai/v1",
        default_model: "deepseek-ai/DeepSeek-V3",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::Bearer,
        models: CHUTES_MODELS,
        supports_model_fetch: false,
        anthropic_base_url: None,
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: ApiTransport::ChatCompletions,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: "huggingface",
        name: "Hugging Face",
        default_base_url: "https://api-inference.huggingface.co/v1",
        default_model: "meta-llama/Llama-3.3-70B-Instruct",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::Bearer,
        models: HUGGINGFACE_MODELS,
        supports_model_fetch: false,
        anthropic_base_url: None,
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: ApiTransport::ChatCompletions,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: "minimax",
        name: "MiniMax",
        default_base_url: "https://api.minimax.io/anthropic",
        default_model: "MiniMax-M1-80k",
        api_type: ApiType::Anthropic,
        auth_method: AuthMethod::XApiKey,
        models: MINIMAX_MODELS,
        supports_model_fetch: false,
        anthropic_base_url: None,
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: ApiTransport::ChatCompletions,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: "minimax-coding-plan",
        name: "MiniMax Coding Plan",
        default_base_url: "https://api.minimax.io/anthropic",
        default_model: "MiniMax-M2.7",
        api_type: ApiType::Anthropic,
        auth_method: AuthMethod::XApiKey,
        models: MINIMAX_MODELS,
        supports_model_fetch: false,
        anthropic_base_url: None,
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: ApiTransport::ChatCompletions,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: "alibaba-coding-plan",
        name: "Alibaba Coding Plan",
        default_base_url: "https://coding-intl.dashscope.aliyuncs.com/v1",
        default_model: "qwen3.5-plus",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::Bearer,
        models: ALIBABA_CODING_MODELS,
        supports_model_fetch: false,
        anthropic_base_url: None,
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: ApiTransport::ChatCompletions,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: "opencode-zen",
        name: "OpenCode Zen",
        default_base_url: "https://opencode.ai/zen/v1",
        default_model: "claude-sonnet-4-5",
        api_type: ApiType::Anthropic,
        auth_method: AuthMethod::Bearer,
        models: OPENCODE_ZEN_MODELS,
        supports_model_fetch: true,
        anthropic_base_url: None,
        supported_transports: CHAT_ONLY_TRANSPORTS,
        default_transport: ApiTransport::ChatCompletions,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: false,
    },
    ProviderDefinition {
        id: "custom",
        name: "Custom",
        default_base_url: "",
        default_model: "",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::Bearer,
        models: EMPTY_MODELS,
        supports_model_fetch: false,
        anthropic_base_url: None,
        supported_transports: RESPONSES_AND_CHAT_TRANSPORTS,
        default_transport: ApiTransport::Responses,
        native_transport_kind: None,
        native_base_url: None,
        supports_response_continuity: true,
    },
];

pub fn get_provider_definition(id: &str) -> Option<&'static ProviderDefinition> {
    PROVIDER_DEFINITIONS.iter().find(|p| p.id == id)
}

pub fn provider_supports_transport(provider_id: &str, transport: ApiTransport) -> bool {
    get_provider_definition(provider_id)
        .map(|definition| definition.supported_transports.contains(&transport))
        .unwrap_or(matches!(
            transport,
            ApiTransport::Responses | ApiTransport::ChatCompletions
        ))
}

pub fn default_api_transport_for_provider(provider_id: &str) -> ApiTransport {
    get_provider_definition(provider_id)
        .map(|definition| definition.default_transport)
        .unwrap_or(ApiTransport::ChatCompletions)
}

pub fn supports_response_continuity(provider_id: &str) -> bool {
    get_provider_definition(provider_id)
        .map(|definition| definition.supports_response_continuity)
        .unwrap_or(false)
}

fn is_alibaba_coding_plan_anthropic_url(base_url: &str) -> bool {
    let lower = base_url.trim().to_ascii_lowercase();
    lower.contains("dashscope.aliyuncs.com") && lower.contains("/apps/anthropic")
}

pub fn get_provider_api_type(provider_id: &str, model: &str, configured_url: &str) -> ApiType {
    if provider_id == "alibaba-coding-plan" && is_alibaba_coding_plan_anthropic_url(configured_url)
    {
        return ApiType::Anthropic;
    }

    let def = get_provider_definition(provider_id);

    match def {
        Some(d) => {
            if d.anthropic_base_url.is_some() {
                if model.starts_with("claude") {
                    ApiType::Anthropic
                } else {
                    ApiType::OpenAI
                }
            } else if provider_id == "opencode-zen" && !model.starts_with("claude") {
                ApiType::OpenAI
            } else {
                d.api_type
            }
        }
        None => ApiType::OpenAI,
    }
}

pub fn get_provider_base_url(provider_id: &str, model: &str, configured_url: &str) -> String {
    if !configured_url.is_empty() {
        return configured_url.to_string();
    }

    let def = get_provider_definition(provider_id);

    match def {
        Some(d) => {
            if d.anthropic_base_url.is_some() && model.starts_with("claude") {
                d.anthropic_base_url.unwrap().to_string()
            } else {
                d.default_base_url.to_string()
            }
        }
        None => configured_url.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Agent configuration (persisted in the daemon SQLite config store)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AgentBackend {
    #[default]
    Daemon,
    Openclaw,
    Hermes,
    Legacy,
}

impl std::fmt::Display for AgentBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Daemon => f.write_str("daemon"),
            Self::Openclaw => f.write_str("openclaw"),
            Self::Hermes => f.write_str("hermes"),
            Self::Legacy => f.write_str("legacy"),
        }
    }
}

impl AgentBackend {
    /// Return the variant as a static string slice matching the serde representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Daemon => "daemon",
            Self::Openclaw => "openclaw",
            Self::Hermes => "hermes",
            Self::Legacy => "legacy",
        }
    }

    /// Parse a backend name, falling back to [`AgentBackend::Daemon`] for
    /// unrecognised values.
    pub fn parse(s: &str) -> Self {
        match s.trim() {
            "openclaw" => Self::Openclaw,
            "hermes" => Self::Hermes,
            "legacy" => Self::Legacy,
            _ => Self::Daemon,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default)]
    pub base_url: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub assistant_id: String,
    #[serde(default)]
    pub enable_honcho_memory: bool,
    #[serde(default)]
    pub honcho_api_key: String,
    #[serde(default)]
    pub honcho_base_url: String,
    #[serde(default = "default_honcho_workspace_id")]
    pub honcho_workspace_id: String,
    #[serde(default = "default_auth_source")]
    pub auth_source: AuthSource,
    #[serde(default = "default_api_transport")]
    pub api_transport: ApiTransport,
    #[serde(default = "default_reasoning_effort")]
    pub reasoning_effort: String,
    #[serde(default = "default_system_prompt")]
    pub system_prompt: String,
    #[serde(default = "default_max_tool_loops")]
    pub max_tool_loops: u32,
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    #[serde(default = "default_retry_delay_ms")]
    pub retry_delay_ms: u64,
    #[serde(default = "default_auto_retry")]
    pub auto_retry: bool,
    #[serde(default = "default_auto_compact_context")]
    pub auto_compact_context: bool,
    #[serde(default = "default_max_context_messages")]
    pub max_context_messages: u32,
    #[serde(default = "default_context_budget_tokens")]
    pub context_budget_tokens: u32,
    #[serde(default = "default_context_window_tokens")]
    pub context_window_tokens: u32,
    #[serde(default = "default_compact_threshold_pct")]
    pub compact_threshold_pct: u32,
    #[serde(default = "default_keep_recent_on_compact")]
    pub keep_recent_on_compact: u32,
    #[serde(default = "default_task_poll_secs")]
    pub task_poll_interval_secs: u64,
    #[serde(default = "default_heartbeat_mins")]
    pub heartbeat_interval_mins: u64,
    /// Cron expression for heartbeat schedule (overrides heartbeat_interval_mins). Per D-06.
    #[serde(default)]
    pub heartbeat_cron: Option<String>,
    /// Heartbeat check configuration. Per D-04.
    #[serde(default)]
    pub heartbeat_checks: HeartbeatChecksConfig,
    /// Audit trail configuration: scope, confidence, retention. Per D-05/D-10.
    #[serde(default)]
    pub audit: AuditConfig,
    /// Quiet hours start (hour 0-23, local time). Per D-07.
    #[serde(default)]
    pub quiet_hours_start: Option<u32>,
    /// Quiet hours end (hour 0-23, local time). Per D-07.
    #[serde(default)]
    pub quiet_hours_end: Option<u32>,
    /// Manual do-not-disturb toggle. Per D-07.
    #[serde(default)]
    pub dnd_enabled: bool,
    #[serde(default)]
    pub tools: ToolsConfig,
    /// Additional provider configurations keyed by provider name.
    #[serde(default)]
    pub providers: HashMap<String, ProviderConfig>,
    /// Gateway configuration for chat platform connections.
    #[serde(default)]
    pub gateway: GatewayConfig,
    /// Agent backend: daemon (built-in LLM), openclaw, hermes, or legacy.
    #[serde(default)]
    pub agent_backend: AgentBackend,
    /// Registry of named sub-agents for orchestration dispatch.
    #[serde(default)]
    pub sub_agents: Vec<SubAgentDefinition>,
    /// Concierge agent configuration.
    #[serde(default)]
    pub concierge: ConciergeConfig,
    /// Anticipatory pre-loading and surfacing controls.
    #[serde(default)]
    pub anticipatory: AnticipatoryConfig,
    /// Learned operator-model controls.
    #[serde(default)]
    pub operator_model: OperatorModelConfig,
    /// Multi-agent collaboration controls.
    #[serde(default)]
    pub collaboration: CollaborationConfig,
    /// Trusted provenance and compliance controls.
    #[serde(default)]
    pub compliance: ComplianceConfig,
    /// Runtime tool synthesis controls.
    #[serde(default)]
    pub tool_synthesis: ToolSynthesisConfig,
    /// Default managed-command policy applied when tools do not override it.
    #[serde(default)]
    pub managed_execution: ManagedExecutionConfig,
    /// Broadcast channel capacity for PTY session output fanout.
    #[serde(default = "default_pty_channel_capacity")]
    pub pty_channel_capacity: usize,
    /// Broadcast channel capacity for agent event fanout.
    #[serde(default = "default_agent_event_channel_capacity")]
    pub agent_event_channel_capacity: usize,
    /// EMA smoothing factor for activity histogram adaptation. Per D-02.
    #[serde(default = "default_ema_alpha")]
    pub ema_alpha: f64,
    /// Heartbeat frequency reduction factor during low-activity hours. Per D-03.
    #[serde(default = "default_low_activity_frequency_factor")]
    pub low_activity_frequency_factor: u64,
    /// Minimum smoothed count to consider an hour "active". Per D-02.
    #[serde(default = "default_ema_activity_threshold")]
    pub ema_activity_threshold: f64,
    /// Memory consolidation controls (Phase 5).
    #[serde(default)]
    pub consolidation: ConsolidationConfig,
    /// Skill discovery thresholds (Phase 6).
    #[serde(default)]
    pub skill_discovery: SkillDiscoveryConfig,
    /// Skill promotion thresholds (Phase 6).
    #[serde(default)]
    pub skill_promotion: SkillPromotionConfig,
    /// Capability tier configuration (Phase 10).
    #[serde(default)]
    pub tier: TierConfig,
    /// Episodic memory configuration (Phase v3.0).
    #[serde(default)]
    pub episodic: super::episodic::EpisodicConfig,
    /// Uncertainty quantification configuration (Phase v3.0: UNCR-01 through UNCR-08).
    #[serde(default)]
    pub uncertainty: super::uncertainty::UncertaintyConfig,
    /// Cost tracking configuration (Phase v3.0: COST-01 through COST-04).
    #[serde(default)]
    pub cost: super::cost::CostConfig,
    /// Additional persisted agent settings used by richer frontends and the TUI.
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnticipatoryConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub morning_brief: bool,
    #[serde(default)]
    pub predictive_hydration: bool,
    #[serde(default)]
    pub stuck_detection: bool,
    #[serde(default = "default_morning_brief_window_minutes")]
    pub morning_brief_window_minutes: u32,
    #[serde(default = "default_stuck_detection_delay_seconds")]
    pub stuck_detection_delay_seconds: u64,
    #[serde(default = "default_surfacing_min_confidence")]
    pub surfacing_min_confidence: f64,
    #[serde(default = "default_surface_cooldown_seconds")]
    pub surface_cooldown_seconds: u64,
}

impl Default for AnticipatoryConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            morning_brief: false,
            predictive_hydration: false,
            stuck_detection: false,
            morning_brief_window_minutes: default_morning_brief_window_minutes(),
            stuck_detection_delay_seconds: default_stuck_detection_delay_seconds(),
            surfacing_min_confidence: default_surfacing_min_confidence(),
            surface_cooldown_seconds: default_surface_cooldown_seconds(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperatorModelConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub allow_message_statistics: bool,
    #[serde(default)]
    pub allow_approval_learning: bool,
    #[serde(default)]
    pub allow_attention_tracking: bool,
    #[serde(default)]
    pub allow_implicit_feedback: bool,
}

impl Default for OperatorModelConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            allow_message_statistics: false,
            allow_approval_learning: false,
            allow_attention_tracking: false,
            allow_implicit_feedback: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CollaborationConfig {
    #[serde(default)]
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ComplianceMode {
    #[default]
    Standard,
    Soc2,
    Hipaa,
    Fedramp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplianceConfig {
    #[serde(default)]
    pub mode: ComplianceMode,
    #[serde(default = "default_compliance_retention_days")]
    pub retention_days: u32,
    #[serde(default)]
    pub sign_all_events: bool,
}

impl Default for ComplianceConfig {
    fn default() -> Self {
        Self {
            mode: ComplianceMode::default(),
            retention_days: default_compliance_retention_days(),
            sign_all_events: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSynthesisConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub require_activation: bool,
    #[serde(default = "default_generated_tool_limit")]
    pub max_generated_tools: usize,
    #[serde(default = "default_generated_tool_auto_promote_threshold")]
    pub auto_promote_threshold: f64,
    #[serde(default)]
    pub sandbox: ToolSynthesisSandboxConfig,
}

impl Default for ToolSynthesisConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            require_activation: true,
            max_generated_tools: default_generated_tool_limit(),
            auto_promote_threshold: default_generated_tool_auto_promote_threshold(),
            sandbox: ToolSynthesisSandboxConfig::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedExecutionConfig {
    #[serde(default)]
    pub sandbox_enabled: bool,
    #[serde(default)]
    pub security_level: SecurityLevel,
}

impl Default for ManagedExecutionConfig {
    fn default() -> Self {
        Self {
            sandbox_enabled: false,
            security_level: SecurityLevel::Lowest,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSynthesisSandboxConfig {
    #[serde(default = "default_generated_tool_timeout_secs")]
    pub max_execution_time_secs: u64,
    #[serde(default)]
    pub allow_network: bool,
    #[serde(default)]
    pub allow_filesystem: bool,
    #[serde(default = "default_generated_tool_output_kb")]
    pub max_output_kb: usize,
}

impl Default for ToolSynthesisSandboxConfig {
    fn default() -> Self {
        Self {
            max_execution_time_secs: default_generated_tool_timeout_secs(),
            allow_network: false,
            allow_filesystem: false,
            max_output_kb: default_generated_tool_output_kb(),
        }
    }
}

// ---------------------------------------------------------------------------
// Consolidation config (Phase 5 — memory consolidation)
// ---------------------------------------------------------------------------

/// Configuration for idle-time memory consolidation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsolidationConfig {
    /// Whether memory consolidation is enabled.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Maximum wall-clock seconds a single consolidation tick may use. Per D-02.
    #[serde(default = "default_consolidation_budget_secs")]
    pub budget_secs: u64,
    /// Seconds of operator inactivity before consolidation may begin. Per D-01.
    #[serde(default = "default_consolidation_idle_threshold_secs")]
    pub idle_threshold_secs: u64,
    /// Days to keep tombstoned (superseded) memory facts before permanent deletion. Per D-05.
    #[serde(default = "default_consolidation_tombstone_ttl_days")]
    pub tombstone_ttl_days: u64,
    /// Number of successful repetitions before a heuristic is promoted. Per D-07.
    #[serde(default = "default_consolidation_heuristic_promotion_threshold")]
    pub heuristic_promotion_threshold: u32,
    /// Half-life in hours for exponential memory fact decay. Per D-04.
    #[serde(default = "default_consolidation_memory_decay_half_life_hours")]
    pub memory_decay_half_life_hours: f64,
    /// Whether to auto-resume interrupted goal runs on daemon restart. Per D-11.
    #[serde(default)]
    pub auto_resume_goal_runs: bool,
    /// Confidence threshold below which decayed facts are tombstoned. Per MEMO-02.
    #[serde(default = "default_consolidation_fact_decay_supersede_threshold")]
    pub fact_decay_supersede_threshold: f64,
}

fn default_consolidation_budget_secs() -> u64 {
    30
}
fn default_consolidation_idle_threshold_secs() -> u64 {
    300
}
fn default_consolidation_tombstone_ttl_days() -> u64 {
    7
}
fn default_consolidation_heuristic_promotion_threshold() -> u32 {
    3
}
fn default_consolidation_memory_decay_half_life_hours() -> f64 {
    69.0
}
fn default_consolidation_fact_decay_supersede_threshold() -> f64 {
    0.2
}

impl Default for ConsolidationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            budget_secs: default_consolidation_budget_secs(),
            idle_threshold_secs: default_consolidation_idle_threshold_secs(),
            tombstone_ttl_days: default_consolidation_tombstone_ttl_days(),
            heuristic_promotion_threshold: default_consolidation_heuristic_promotion_threshold(),
            memory_decay_half_life_hours: default_consolidation_memory_decay_half_life_hours(),
            auto_resume_goal_runs: false,
            fact_decay_supersede_threshold: default_consolidation_fact_decay_supersede_threshold(),
        }
    }
}

// ---------------------------------------------------------------------------
// Skill maturity lifecycle (Phase 6 — skill discovery)
// ---------------------------------------------------------------------------

/// Maturity stage of a skill variant as it progresses through discovery.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SkillMaturityStatus {
    Draft,
    Testing,
    Active,
    Proven,
    #[serde(rename = "promoted_to_canonical")]
    PromotedToCanonical,
}

impl SkillMaturityStatus {
    /// Return the snake_case string representation.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Testing => "testing",
            Self::Active => "active",
            Self::Proven => "proven",
            Self::PromotedToCanonical => "promoted_to_canonical",
        }
    }

    /// Parse from a status string, supporting both snake_case and legacy kebab-case.
    pub fn from_status_str(s: &str) -> Option<Self> {
        match s {
            "draft" => Some(Self::Draft),
            "testing" => Some(Self::Testing),
            "active" => Some(Self::Active),
            "proven" => Some(Self::Proven),
            "promoted_to_canonical" | "promoted-to-canonical" => Some(Self::PromotedToCanonical),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// Skill discovery config (Phase 6)
// ---------------------------------------------------------------------------

/// Thresholds for deciding whether an execution trace qualifies as a skill candidate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDiscoveryConfig {
    /// Minimum number of tool calls in the trace to consider it complex enough.
    #[serde(default = "default_sd_min_tool_count")]
    pub min_tool_count: usize,
    /// Minimum number of replan events to indicate adaptive problem-solving.
    #[serde(default = "default_sd_min_replan_count")]
    pub min_replan_count: u32,
    /// Minimum quality score (0.0-1.0) to consider the trace successful enough.
    #[serde(default = "default_sd_min_quality_score")]
    pub min_quality_score: f64,
    /// Jaccard similarity threshold below which a sequence is considered novel.
    #[serde(default = "default_sd_novelty_threshold")]
    pub novelty_similarity_threshold: f64,
}

fn default_sd_min_tool_count() -> usize {
    8
}
fn default_sd_min_replan_count() -> u32 {
    1
}
fn default_sd_min_quality_score() -> f64 {
    0.8
}
fn default_sd_novelty_threshold() -> f64 {
    0.7
}

impl Default for SkillDiscoveryConfig {
    fn default() -> Self {
        Self {
            min_tool_count: default_sd_min_tool_count(),
            min_replan_count: default_sd_min_replan_count(),
            min_quality_score: default_sd_min_quality_score(),
            novelty_similarity_threshold: default_sd_novelty_threshold(),
        }
    }
}

// ---------------------------------------------------------------------------
// Skill promotion config (Phase 6)
// ---------------------------------------------------------------------------

/// Success thresholds for promoting a skill variant through maturity stages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillPromotionConfig {
    /// Successful uses required to advance Testing -> Active.
    #[serde(default = "default_sp_testing_to_active")]
    pub testing_to_active: u32,
    /// Successful uses required to advance Active -> Proven.
    #[serde(default = "default_sp_active_to_proven")]
    pub active_to_proven: u32,
    /// Successful uses required to advance Proven -> PromotedToCanonical.
    #[serde(default = "default_sp_proven_to_canonical")]
    pub proven_to_canonical: u32,
}

fn default_sp_testing_to_active() -> u32 {
    3
}
fn default_sp_active_to_proven() -> u32 {
    5
}
fn default_sp_proven_to_canonical() -> u32 {
    10
}

impl Default for SkillPromotionConfig {
    fn default() -> Self {
        Self {
            testing_to_active: default_sp_testing_to_active(),
            active_to_proven: default_sp_active_to_proven(),
            proven_to_canonical: default_sp_proven_to_canonical(),
        }
    }
}

/// Outcome of a single consolidation tick.
#[derive(Debug, Clone, Default)]
pub struct ConsolidationResult {
    pub traces_reviewed: usize,
    pub facts_decayed: usize,
    pub tombstones_purged: usize,
    pub facts_refined: usize,
    pub skipped_reason: Option<String>,
    /// Skill discovery fields (Phase 6).
    pub skill_candidates_flagged: usize,
    pub skills_drafted: usize,
    pub skills_tested: usize,
    pub skills_promoted: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GatewayConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub slack_token: String,
    #[serde(default)]
    pub slack_channel_filter: String,
    #[serde(default)]
    pub telegram_token: String,
    #[serde(default)]
    pub telegram_allowed_chats: String,
    #[serde(default)]
    pub discord_token: String,
    #[serde(default)]
    pub discord_channel_filter: String,
    #[serde(default)]
    pub discord_allowed_users: String,
    #[serde(default)]
    pub whatsapp_allowed_contacts: String,
    #[serde(default)]
    pub whatsapp_token: String,
    #[serde(default)]
    pub whatsapp_phone_id: String,
    #[serde(default)]
    pub command_prefix: String,
    /// Feature flag: when false (default), only daemon gateways run.
    /// When true, Electron bridges run alongside daemon gateways. Per D-07.
    /// Note: platform tokens may also be provided via env vars as a fallback
    /// (SLACK_TOKEN, TELEGRAM_TOKEN, etc.) per D-02 migration path.
    #[serde(default)]
    pub gateway_electron_bridges_enabled: bool,
    #[serde(default)]
    pub whatsapp_link_fallback_electron: bool,
}

fn default_provider() -> String {
    "openai".into()
}
fn default_api_transport() -> ApiTransport {
    default_api_transport_for_provider("openai")
}
fn default_auth_source() -> AuthSource {
    AuthSource::ApiKey
}
fn default_system_prompt() -> String {
    format!(
        "You are {} - The Smith (He is a blacksmith god, the creator and craftsman of the heavens in ancient Slavic belief. As an AI agent:\n- Creation: Ideal for tasks intended for use from scratch (coding, writing, design).\n- Rhythm: Associated with the sun and fire, he naturally determines the daily cycles (sunrise-sunset).\n- Personality: Strict but fair; an accessible \"doer\" who ensures this through perfect tools.) operating in tamux, an always-on agentic terminal multiplexer assistant. {} is your concierge counterpart: lighter, faster, and operator-facing. You can execute terminal commands, monitor systems, and send messages to connected chat platforms. Use your tools proactively. Be concise and direct.",
        AGENT_NAME_SWAROG, AGENT_NAME_RAROG
    )
}
fn default_reasoning_effort() -> String {
    "high".into()
}
fn default_max_tool_loops() -> u32 {
    0
}
fn default_pty_channel_capacity() -> usize {
    1024
}
fn default_agent_event_channel_capacity() -> usize {
    512
}
fn default_max_retries() -> u32 {
    3
}
fn default_retry_delay_ms() -> u64 {
    2000
}
fn default_auto_retry() -> bool {
    true
}
fn default_auto_compact_context() -> bool {
    true
}
fn default_max_context_messages() -> u32 {
    100
}
fn default_context_budget_tokens() -> u32 {
    100_000
}
fn default_context_window_tokens() -> u32 {
    128_000
}
fn default_compact_threshold_pct() -> u32 {
    80
}
fn default_keep_recent_on_compact() -> u32 {
    10
}
fn default_task_poll_secs() -> u64 {
    10
}
fn default_ema_alpha() -> f64 {
    0.3
}
fn default_low_activity_frequency_factor() -> u64 {
    4
}
fn default_ema_activity_threshold() -> f64 {
    2.0
}
fn default_heartbeat_mins() -> u64 {
    30
}
fn default_morning_brief_window_minutes() -> u32 {
    30
}
fn default_stuck_detection_delay_seconds() -> u64 {
    45
}
fn default_surfacing_min_confidence() -> f64 {
    0.7
}
fn default_surface_cooldown_seconds() -> u64 {
    300
}
fn default_honcho_workspace_id() -> String {
    "tamux".to_string()
}
fn default_compliance_retention_days() -> u32 {
    90
}
fn default_generated_tool_limit() -> usize {
    20
}
fn default_generated_tool_auto_promote_threshold() -> f64 {
    0.85
}
fn default_generated_tool_timeout_secs() -> u64 {
    30
}
fn default_generated_tool_output_kb() -> usize {
    512
}
impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            provider: default_provider(),
            base_url: String::new(),
            model: String::new(),
            api_key: String::new(),
            assistant_id: String::new(),
            enable_honcho_memory: false,
            honcho_api_key: String::new(),
            honcho_base_url: String::new(),
            honcho_workspace_id: default_honcho_workspace_id(),
            auth_source: default_auth_source(),
            api_transport: default_api_transport(),
            reasoning_effort: default_reasoning_effort(),
            system_prompt: default_system_prompt(),
            max_tool_loops: default_max_tool_loops(),
            max_retries: default_max_retries(),
            retry_delay_ms: default_retry_delay_ms(),
            auto_retry: default_auto_retry(),
            auto_compact_context: default_auto_compact_context(),
            max_context_messages: default_max_context_messages(),
            context_budget_tokens: default_context_budget_tokens(),
            context_window_tokens: default_context_window_tokens(),
            compact_threshold_pct: default_compact_threshold_pct(),
            keep_recent_on_compact: default_keep_recent_on_compact(),
            task_poll_interval_secs: default_task_poll_secs(),
            heartbeat_interval_mins: default_heartbeat_mins(),
            heartbeat_cron: None,
            heartbeat_checks: HeartbeatChecksConfig::default(),
            audit: AuditConfig::default(),
            quiet_hours_start: None,
            quiet_hours_end: None,
            dnd_enabled: false,
            tools: ToolsConfig::default(),
            providers: HashMap::new(),
            gateway: GatewayConfig::default(),
            agent_backend: AgentBackend::default(),
            sub_agents: Vec::new(),
            concierge: ConciergeConfig::default(),
            anticipatory: AnticipatoryConfig::default(),
            operator_model: OperatorModelConfig::default(),
            collaboration: CollaborationConfig::default(),
            compliance: ComplianceConfig::default(),
            tool_synthesis: ToolSynthesisConfig::default(),
            managed_execution: ManagedExecutionConfig::default(),
            pty_channel_capacity: default_pty_channel_capacity(),
            agent_event_channel_capacity: default_agent_event_channel_capacity(),
            ema_alpha: default_ema_alpha(),
            low_activity_frequency_factor: default_low_activity_frequency_factor(),
            ema_activity_threshold: default_ema_activity_threshold(),
            consolidation: ConsolidationConfig::default(),
            skill_discovery: SkillDiscoveryConfig::default(),
            skill_promotion: SkillPromotionConfig::default(),
            tier: TierConfig::default(),
            episodic: super::episodic::EpisodicConfig::default(),
            uncertainty: super::uncertainty::UncertaintyConfig::default(),
            cost: super::cost::CostConfig::default(),
            extra: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub base_url: String,
    pub model: String,
    pub api_key: String,
    #[serde(default)]
    pub assistant_id: String,
    #[serde(default = "default_auth_source")]
    pub auth_source: AuthSource,
    #[serde(default = "default_api_transport")]
    pub api_transport: ApiTransport,
    #[serde(default = "default_reasoning_effort")]
    pub reasoning_effort: String,
    #[serde(default = "default_context_window_tokens")]
    pub context_window_tokens: u32,
    /// When set, request structured output with this JSON schema from the API.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub response_schema: Option<serde_json::Value>,
}

/// A named sub-agent definition that the orchestration engine can dispatch work to.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubAgentDefinition {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub model: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_whitelist: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_blacklist: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_budget_tokens: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_duration_secs: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supervisor_config: Option<SupervisorConfig>,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub created_at: u64,
}

/// Snapshot of a provider's authentication status for UI display.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderAuthState {
    pub provider_id: String,
    pub provider_name: String,
    pub authenticated: bool,
    pub auth_source: AuthSource,
    pub model: String,
    pub base_url: String,
}

/// A structured fallback suggestion for an outage or degraded provider.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderAlternativeSuggestion {
    pub provider_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub reason: String,
}

/// Structured outage metadata attached to provider circuit-breaker events.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderCircuitOpenDetails {
    pub provider: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed_model: Option<String>,
    pub trip_count: u32,
    pub reason: String,
    #[serde(default)]
    pub suggested_alternatives: Vec<ProviderAlternativeSuggestion>,
}

/// Structured provider-health snapshot entry exposed in `provider_health_json`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderHealthSnapshot {
    pub provider_id: String,
    pub can_execute: bool,
    pub trip_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failed_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default)]
    pub suggested_alternatives: Vec<ProviderAlternativeSuggestion>,
}

fn default_provider_circuit_reason() -> String {
    "circuit breaker open".to_string()
}

// ---------------------------------------------------------------------------
// Concierge
// ---------------------------------------------------------------------------

/// How much context the concierge gathers for its welcome greeting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ConciergeDetailLevel {
    Minimal,
    #[default]
    ContextSummary,
    ProactiveTriage,
    DailyBriefing,
}

/// The type of quick-action a concierge welcome button triggers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConciergeActionType {
    ContinueSession,
    StartNew,
    Search,
    Dismiss,
    StartGoalRun,
    DismissWelcome,
    FocusChat,
    OpenSettings,
}

/// A structured quick-action button in the concierge welcome message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConciergeAction {
    pub label: String,
    pub action_type: ConciergeActionType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
}

/// Configuration for the concierge agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConciergeConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub detail_level: ConciergeDetailLevel,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default = "default_true")]
    pub auto_cleanup_on_navigate: bool,
}

impl Default for ConciergeConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            detail_level: ConciergeDetailLevel::default(),
            provider: None,
            model: None,
            auto_cleanup_on_navigate: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolsConfig {
    #[serde(default = "default_true")]
    pub bash: bool,
    #[serde(default)]
    pub web_search: bool,
    #[serde(default)]
    pub web_browse: bool,
    #[serde(default)]
    pub vision: bool,
    #[serde(default = "default_true")]
    pub gateway_messaging: bool,
    #[serde(default = "default_true")]
    pub file_operations: bool,
    #[serde(default = "default_true")]
    pub system_info: bool,
}

fn default_true() -> bool {
    true
}

impl Default for ToolsConfig {
    fn default() -> Self {
        Self {
            bash: true,
            web_search: false,
            web_browse: false,
            vision: false,
            gateway_messaging: true,
            file_operations: true,
            system_info: true,
        }
    }
}

// ---------------------------------------------------------------------------
// Agent events (broadcast to frontend subscribers)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TodoStatus {
    Pending,
    InProgress,
    Completed,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TodoItem {
    pub id: String,
    pub content: String,
    pub status: TodoStatus,
    pub position: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step_index: Option<usize>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkContextEntryKind {
    RepoChange,
    Artifact,
    GeneratedSkill,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkContextEntry {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_path: Option<String>,
    pub kind: WorkContextEntryKind,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub change_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repo_root: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal_run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step_index: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default)]
    pub is_text: bool,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThreadWorkContext {
    pub thread_id: String,
    #[serde(default)]
    pub entries: Vec<WorkContextEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AnticipatoryItem {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub summary: String,
    #[serde(default)]
    pub bullets: Vec<String>,
    pub confidence: f64,
    #[serde(default)]
    pub goal_run_id: Option<String>,
    #[serde(default)]
    pub thread_id: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgentEvent {
    Delta {
        thread_id: String,
        content: String,
    },
    Reasoning {
        thread_id: String,
        content: String,
    },
    ToolCall {
        thread_id: String,
        call_id: String,
        name: String,
        arguments: String,
    },
    ToolResult {
        thread_id: String,
        call_id: String,
        name: String,
        content: String,
        is_error: bool,
    },
    Done {
        thread_id: String,
        input_tokens: u64,
        output_tokens: u64,
        cost: Option<f64>,
        provider: Option<String>,
        model: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        tps: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        generation_ms: Option<u64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        reasoning: Option<String>,
    },
    Error {
        thread_id: String,
        message: String,
    },
    ThreadCreated {
        thread_id: String,
        title: String,
    },
    TaskUpdate {
        task_id: String,
        status: TaskStatus,
        progress: u8,
        message: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        task: Option<AgentTask>,
    },
    GoalRunUpdate {
        goal_run_id: String,
        status: GoalRunStatus,
        #[serde(skip_serializing_if = "Option::is_none")]
        current_step_index: Option<usize>,
        message: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        goal_run: Option<GoalRun>,
    },
    TodoUpdate {
        thread_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        goal_run_id: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        step_index: Option<usize>,
        items: Vec<TodoItem>,
    },
    WorkContextUpdate {
        thread_id: String,
        context: ThreadWorkContext,
    },
    WorkflowNotice {
        thread_id: String,
        kind: String,
        message: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        details: Option<String>,
    },
    RetryStatus {
        thread_id: String,
        phase: String,
        attempt: u32,
        max_retries: u32,
        delay_ms: u64,
        failure_class: String,
        message: String,
    },
    AnticipatoryUpdate {
        items: Vec<AnticipatoryItem>,
    },
    HeartbeatResult {
        item_id: String,
        result: HeartbeatOutcome,
        message: String,
    },
    /// Prioritized heartbeat digest from structured checks + LLM synthesis. Per D-11.
    HeartbeatDigest {
        cycle_id: String,
        actionable: bool,
        digest: String,
        items: Vec<HeartbeatDigestItem>,
        checked_at: u64,
        /// Inline explanation for the overall digest. Per D-01.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        explanation: Option<String>,
        /// Confidence of the overall assessment (0.0..1.0). Per D-01/D-09.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        confidence: Option<f64>,
    },
    Notification {
        title: String,
        body: String,
        severity: NotificationSeverity,
        channels: Vec<String>,
    },
    /// Request to send a message via a gateway platform (Slack/Discord/Telegram/WhatsApp).
    GatewaySend {
        platform: String,
        target: String,
        message: String,
    },
    /// Execute a workspace UI command on the frontend.
    WorkspaceCommand {
        command: String,
        args: serde_json::Value,
    },
    /// Incoming message from a gateway platform (for frontend display).
    GatewayIncoming {
        platform: String,
        sender: String,
        content: String,
        channel: String,
    },
    /// Gateway platform connection status change (per D-05/GATE-05).
    GatewayStatus {
        platform: String,
        /// Serialized status: "connected", "disconnected", "error".
        status: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        last_error: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        consecutive_failures: Option<u32>,
    },
    WhatsAppLinkStatus {
        state: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        phone: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        last_error: Option<String>,
    },
    WhatsAppLinkQr {
        ascii_qr: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        expires_at_ms: Option<u64>,
    },
    WhatsAppLinked {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        phone: Option<String>,
    },
    WhatsAppLinkError {
        message: String,
        recoverable: bool,
    },
    WhatsAppLinkDisconnected {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// Sub-agent health state change detected by the supervisor.
    SubagentHealthChange {
        task_id: String,
        previous_state: SubagentHealthState,
        new_state: SubagentHealthState,
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<StuckReason>,
        #[serde(skip_serializing_if = "Option::is_none")]
        intervention: Option<InterventionAction>,
    },
    /// A checkpoint was created for a goal run.
    CheckpointCreated {
        checkpoint_id: String,
        goal_run_id: String,
        checkpoint_type: String,
        step_index: Option<usize>,
    },
    ConciergeWelcome {
        thread_id: String,
        content: String,
        detail_level: ConciergeDetailLevel,
        actions: Vec<ConciergeAction>,
    },
    /// A provider's circuit breaker has tripped to Open state (per D-06).
    ProviderCircuitOpen {
        provider: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        failed_model: Option<String>,
        trip_count: u32,
        #[serde(default = "default_provider_circuit_reason")]
        reason: String,
        #[serde(default)]
        suggested_alternatives: Vec<ProviderAlternativeSuggestion>,
    },
    /// A provider's circuit breaker has recovered to Closed state (per D-07).
    ProviderCircuitRecovered {
        provider: String,
    },
    /// Broadcast audit entry to all connected clients. Per D-06/TRNS-03.
    AuditAction {
        id: String,
        timestamp: u64,
        action_type: String,
        summary: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        explanation: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        confidence: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        confidence_band: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        causal_trace_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        thread_id: Option<String>,
    },
    /// Escalation level change notification. Per D-11/TRNS-05.
    EscalationUpdate {
        thread_id: String,
        from_level: String,
        to_level: String,
        reason: String,
        attempts: u32,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        audit_id: Option<String>,
    },
    /// Capability tier changed notification (Phase 10).
    TierChanged {
        previous_tier: String,
        new_tier: String,
        reason: String,
    },
    /// An episode was recorded in episodic memory (Phase v3.0).
    EpisodeRecorded {
        episode_id: String,
        episode_type: String,
        outcome: String,
        summary: String,
    },
    /// Counter-who detected a repeated correction pattern (Phase v3.0).
    CounterWhoAlert {
        thread_id: String,
        pattern: String,
        attempt_count: u32,
        suggestion: String,
    },
    /// Trajectory update for a goal run or entity (Phase v3.0: AWAR-04).
    TrajectoryUpdate {
        goal_run_id: String,
        /// "converging", "diverging", or "stalled"
        direction: String,
        progress_ratio: f64,
        message: String,
    },
    /// Mode shift triggered after diminishing returns + counter-who confirmation (Phase v3.0: AWAR-02).
    ModeShift {
        thread_id: String,
        reason: String,
        previous_strategy: String,
        new_strategy: String,
    },
    /// Confidence warning for a planned or executing action (Phase v3.0: AWAR-05).
    ConfidenceWarning {
        thread_id: String,
        /// "plan_step" or "tool_call"
        action_type: String,
        /// "high", "medium", or "low"
        band: String,
        evidence: String,
        domain: String,
        blocked: bool,
    },
    /// Budget alert: cumulative goal run cost crossed the operator-defined threshold (COST-03).
    BudgetAlert {
        goal_run_id: String,
        current_cost_usd: f64,
        threshold_usd: f64,
    },
}

// ---------------------------------------------------------------------------
// Threads & messages
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentThread {
    pub id: String,
    pub title: String,
    pub messages: Vec<AgentMessage>,
    #[serde(default)]
    pub pinned: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream_thread_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream_transport: Option<ApiTransport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream_provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream_assistant_id: Option<String>,
    pub created_at: u64,
    pub updated_at: u64,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentMessage {
    /// Stable unique ID for persistence and deletion. Generated on creation.
    #[serde(default = "generate_message_id")]
    pub id: String,
    pub role: MessageRole,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_arguments: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_status: Option<String>,
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_transport: Option<ApiTransport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    pub timestamp: u64,
}

pub fn generate_message_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

impl AgentMessage {
    pub fn user(content: impl Into<String>, now: u64) -> Self {
        Self {
            id: generate_message_id(),
            role: MessageRole::User,
            content: content.into(),
            tool_calls: None,
            tool_call_id: None,
            tool_name: None,
            tool_arguments: None,
            tool_status: None,
            input_tokens: 0,
            output_tokens: 0,
            provider: None,
            model: None,
            api_transport: None,
            response_id: None,
            reasoning: None,
            timestamp: now,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

// ---------------------------------------------------------------------------
// Tool calls
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub function: ToolFunction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFunction {
    pub name: String,
    pub arguments: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub tool_call_id: String,
    pub name: String,
    pub content: String,
    pub is_error: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pending_approval: Option<ToolPendingApproval>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolPendingApproval {
    pub approval_id: String,
    pub execution_id: String,
    pub command: String,
    pub rationale: String,
    pub risk_level: String,
    pub blast_radius: String,
    pub reasons: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: ToolFunctionDef,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolFunctionDef {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

// ---------------------------------------------------------------------------
// Task queue
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Sub-agent management
// ---------------------------------------------------------------------------

/// Configuration for sub-agent supervision — how often to check, when to
/// consider a sub-agent stuck, and what intervention level to apply.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupervisorConfig {
    /// How often to check sub-agent health (seconds). Default: 30.
    #[serde(default = "default_supervisor_check_interval")]
    pub check_interval_secs: u64,
    /// Seconds of no progress before flagging as stuck. Default: 300 (5 min).
    #[serde(default = "default_stuck_timeout")]
    pub stuck_timeout_secs: u64,
    /// Maximum retries before escalating. Default: 2.
    #[serde(default = "default_supervisor_max_retries")]
    pub max_retries: u32,
    /// How aggressively to intervene. Default: Normal.
    #[serde(default)]
    pub intervention_level: InterventionLevel,
}

impl Default for SupervisorConfig {
    fn default() -> Self {
        Self {
            check_interval_secs: default_supervisor_check_interval(),
            stuck_timeout_secs: default_stuck_timeout(),
            max_retries: default_supervisor_max_retries(),
            intervention_level: InterventionLevel::default(),
        }
    }
}

fn default_supervisor_check_interval() -> u64 {
    30
}
fn default_stuck_timeout() -> u64 {
    300
}
fn default_supervisor_max_retries() -> u32 {
    2
}

/// How aggressively the supervisor should intervene when issues are detected.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum InterventionLevel {
    /// Only log, never intervene automatically.
    Passive,
    /// Self-correct where safe (compress context, inject reflection).
    #[default]
    Normal,
    /// Aggressively intervene (terminate stuck agents, retry from checkpoint).
    Aggressive,
}

/// Overall health state of a sub-agent as determined by the supervisor.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SubagentHealthState {
    #[default]
    Healthy,
    Degraded,
    Stuck,
    Crashed,
}

/// Why a sub-agent is considered stuck.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StuckReason {
    /// No tool calls or progress for configured timeout.
    NoProgress,
    /// Same error repeated 3+ times in a row.
    ErrorLoop,
    /// Cycling tool calls (A→B→A→B pattern).
    ToolCallLoop,
    /// Context budget > 90% consumed.
    ResourceExhaustion,
    /// Exceeded max_duration_secs.
    Timeout,
}

/// What the supervisor should do when a problem is detected.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum InterventionAction {
    /// Inject a self-assessment prompt asking the agent to reflect.
    SelfAssess,
    /// Compress context to free up budget.
    CompressContext,
    /// Retry from the last successful checkpoint.
    RetryFromCheckpoint,
    /// Escalate to the parent task/agent.
    EscalateToParent,
    /// Escalate to the user for manual intervention.
    EscalateToUser,
}

/// What to do when a context budget is exceeded.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ContextOverflowAction {
    /// Compress older context to free space.
    #[default]
    Compress,
    /// Truncate oldest messages.
    Truncate,
    /// Return an error and stop execution.
    Error,
}

// ---------------------------------------------------------------------------
// Task queue
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Queued,
    #[serde(alias = "running")]
    InProgress,
    AwaitingApproval,
    Blocked,
    FailedAnalyzing,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum TaskPriority {
    Low,
    #[default]
    Normal,
    High,
    Urgent,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TaskLogLevel {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTaskLogEntry {
    pub id: String,
    pub timestamp: u64,
    pub level: TaskLogLevel,
    pub phase: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    #[serde(default)]
    pub attempt: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTask {
    pub id: String,
    pub title: String,
    pub description: String,
    pub status: TaskStatus,
    #[serde(default)]
    pub priority: TaskPriority,
    #[serde(default)]
    pub progress: u8,
    pub created_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(default = "default_source")]
    pub source: String,
    #[serde(default)]
    pub notify_on_complete: bool,
    #[serde(default)]
    pub notify_channels: Vec<String>,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal_run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal_run_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal_step_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal_step_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_thread_id: Option<String>,
    #[serde(default = "default_task_runtime")]
    pub runtime: String,
    #[serde(default)]
    pub retry_count: u32,
    #[serde(default = "default_max_task_retries")]
    pub max_retries: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_retry_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scheduled_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub awaiting_approval_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lane_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    #[serde(default)]
    pub logs: Vec<AgentTaskLogEntry>,

    // -- Sub-agent management extensions (Phase 1) --
    /// Restrict which tools this sub-agent may call. `None` = all tools allowed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_whitelist: Option<Vec<String>>,
    /// Tools this sub-agent must NOT call. Applied after whitelist.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_blacklist: Option<Vec<String>>,
    /// Maximum tokens this sub-agent may consume for its context window.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_budget_tokens: Option<u32>,
    /// What to do when the context budget is exceeded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_overflow_action: Option<ContextOverflowAction>,
    /// DSL expression for automatic termination (e.g. "timeout(300) OR error_count(3)").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub termination_conditions: Option<String>,
    /// Criteria the sub-agent must satisfy for the step to be considered successful.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub success_criteria: Option<String>,
    /// Hard time limit in seconds (fallback: 1800 = 30 min).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_duration_secs: Option<u64>,
    /// Supervision configuration for this sub-agent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supervisor_config: Option<SupervisorConfig>,

    // -- Provider/model override for sub-agent dispatch --
    /// Override provider for this task (from SubAgentDefinition).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub override_provider: Option<String>,
    /// Override model for this task (from SubAgentDefinition).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub override_model: Option<String>,
    /// Override system prompt for this task (from SubAgentDefinition).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub override_system_prompt: Option<String>,
    /// The SubAgentDefinition ID this task was spawned from, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sub_agent_def_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunKind {
    Task,
    Subagent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRun {
    pub id: String,
    pub task_id: String,
    pub kind: AgentRunKind,
    pub classification: String,
    pub title: String,
    pub description: String,
    pub status: TaskStatus,
    #[serde(default)]
    pub priority: TaskPriority,
    #[serde(default)]
    pub progress: u8,
    pub created_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    #[serde(default = "default_source")]
    pub source: String,
    #[serde(default = "default_task_runtime")]
    pub runtime: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal_run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal_run_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal_step_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub goal_step_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_thread_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

fn default_source() -> String {
    "user".into()
}

fn default_max_task_retries() -> u32 {
    3
}

fn default_task_runtime() -> String {
    "daemon".into()
}

// ---------------------------------------------------------------------------
// Goal runner
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GoalRunStatus {
    Queued,
    Planning,
    Running,
    AwaitingApproval,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GoalRunStepKind {
    #[default]
    Reason,
    Command,
    Research,
    Memory,
    Skill,
    /// Route this step to a specialist subagent via the handoff broker.
    /// The String is the specialist role name (e.g., "backend-developer").
    Specialist(String),
    /// Spawn a divergent session with parallel framings for this step.
    /// The step instructions become the problem statement.
    Divergent,
    /// Fallback for unknown/empty kind values from LLM output.
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GoalRunStepStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalRunStep {
    pub id: String,
    pub position: usize,
    pub title: String,
    pub instructions: String,
    pub kind: GoalRunStepKind,
    pub success_criteria: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub status: GoalRunStepStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalRunEvent {
    pub id: String,
    pub timestamp: u64,
    pub phase: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step_index: Option<usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub todo_snapshot: Vec<TodoItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GoalRun {
    pub id: String,
    pub title: String,
    pub goal: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_request_id: Option<String>,
    pub status: GoalRunStatus,
    pub priority: TaskPriority,
    pub created_at: u64,
    pub updated_at: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub current_step_index: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_step_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_step_kind: Option<GoalRunStepKind>,
    pub replan_count: u32,
    pub max_replans: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plan_summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reflection_summary: Option<String>,
    #[serde(default)]
    pub memory_updates: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub generated_skill_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_cause: Option<String>,
    #[serde(default)]
    pub child_task_ids: Vec<String>,
    #[serde(default)]
    pub child_task_count: u32,
    #[serde(default)]
    pub approval_count: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub awaiting_approval_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub steps: Vec<GoalRunStep>,
    #[serde(default)]
    pub events: Vec<GoalRunEvent>,
    /// Total prompt tokens consumed across all LLM calls in this goal run (COST-01).
    #[serde(default)]
    pub total_prompt_tokens: u64,
    /// Total completion tokens consumed across all LLM calls in this goal run (COST-01).
    #[serde(default)]
    pub total_completion_tokens: u64,
    /// Estimated cost in USD based on provider rate cards (COST-02).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimated_cost_usd: Option<f64>,
    /// Per-goal autonomy dial: autonomous / aware / supervised (AUTO-01).
    #[serde(default)]
    pub autonomy_level: super::autonomy::AutonomyLevel,
    /// Attribution tag for goal-run output (AUTH-01).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub authorship_tag: Option<super::authorship::AuthorshipTag>,
}

// ---------------------------------------------------------------------------
// Heartbeat
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HeartbeatOutcome {
    Ok,
    Alert,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatItem {
    pub id: String,
    pub label: String,
    pub prompt: String,
    #[serde(default = "default_zero")]
    pub interval_minutes: u64,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_run_at: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_result: Option<HeartbeatOutcome>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_message: Option<String>,
    #[serde(default)]
    pub notify_on_alert: bool,
    #[serde(default)]
    pub notify_channels: Vec<String>,
}

fn default_zero() -> u64 {
    0
}

// ---------------------------------------------------------------------------
// Heartbeat structured checks (Phase 2 — core heartbeat)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum HeartbeatCheckType {
    StaleTodos,
    StuckGoalRuns,
    UnrepliedGatewayMessages,
    RepoChanges,
    SkillLifecycle,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CheckSeverity {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckDetail {
    pub id: String,
    pub label: String,
    pub age_hours: f64,
    pub severity: CheckSeverity,
    pub context: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatCheckResult {
    pub check_type: HeartbeatCheckType,
    pub items_found: usize,
    pub summary: String,
    pub details: Vec<CheckDetail>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatDigestItem {
    pub priority: u8,
    pub check_type: HeartbeatCheckType,
    pub title: String,
    pub suggestion: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatChecksConfig {
    #[serde(default = "default_true")]
    pub stale_todos_enabled: bool,
    #[serde(default = "default_stale_todo_threshold_hours")]
    pub stale_todo_threshold_hours: u64,
    #[serde(default = "default_true")]
    pub stuck_goals_enabled: bool,
    #[serde(default = "default_stuck_goal_threshold_hours")]
    pub stuck_goal_threshold_hours: u64,
    #[serde(default = "default_true")]
    pub unreplied_messages_enabled: bool,
    #[serde(default = "default_unreplied_threshold_hours")]
    pub unreplied_message_threshold_hours: u64,
    #[serde(default = "default_true")]
    pub repo_changes_enabled: bool,
    #[serde(default)]
    pub stale_todos_cron: Option<String>,
    #[serde(default)]
    pub stuck_goals_cron: Option<String>,
    #[serde(default)]
    pub unreplied_messages_cron: Option<String>,
    #[serde(default)]
    pub repo_changes_cron: Option<String>,
    // Per D-06: Per-check priority weights (0.0-1.0). 1.0 = every cycle.
    #[serde(default = "default_priority_weight")]
    pub stale_todos_priority_weight: f64,
    #[serde(default = "default_priority_weight")]
    pub stuck_goals_priority_weight: f64,
    #[serde(default = "default_priority_weight")]
    pub unreplied_messages_priority_weight: f64,
    #[serde(default = "default_priority_weight")]
    pub repo_changes_priority_weight: f64,
    // Per D-11: Per-check priority overrides (pin to specific weight).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stale_todos_priority_override: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stuck_goals_priority_override: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unreplied_messages_priority_override: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_changes_priority_override: Option<f64>,
    /// Per D-11: Global reset action — when true, resets all learned priority weights to 1.0.
    #[serde(default)]
    pub reset_learned_priorities: bool,
}

fn default_priority_weight() -> f64 {
    1.0
}

fn default_stale_todo_threshold_hours() -> u64 {
    24
}
fn default_stuck_goal_threshold_hours() -> u64 {
    2
}
fn default_unreplied_threshold_hours() -> u64 {
    1
}

// ---------------------------------------------------------------------------
// Audit configuration (per D-05/D-10)
// ---------------------------------------------------------------------------

/// Which action types to include in the audit feed. Per D-05.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditScopeConfig {
    #[serde(default = "default_true")]
    pub heartbeat: bool,
    #[serde(default = "default_true")]
    pub tool: bool,
    #[serde(default = "default_true")]
    pub escalation: bool,
    #[serde(default = "default_true")]
    pub skill: bool,
    #[serde(default = "default_true")]
    pub subagent: bool,
}

impl Default for AuditScopeConfig {
    fn default() -> Self {
        Self {
            heartbeat: true,
            tool: true,
            escalation: true,
            skill: true,
            subagent: true,
        }
    }
}

/// Audit trail configuration: scope, confidence thresholds, retention. Per D-05/D-10.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditConfig {
    /// Which action types to include in the audit feed.
    #[serde(default)]
    pub scope: AuditScopeConfig,
    /// Confidence threshold below which to show qualifiers. Per D-10.
    #[serde(default = "default_confidence_threshold")]
    pub confidence_threshold: f64,
    /// Whether to always show confidence (overrides threshold). Per D-10.
    #[serde(default)]
    pub always_show_confidence: bool,
    /// Maximum audit entries to retain. Per Pitfall 4.
    #[serde(default = "default_max_audit_entries")]
    pub max_entries: usize,
    /// Maximum age of audit entries in days.
    #[serde(default = "default_max_audit_age_days")]
    pub max_age_days: u32,
}

fn default_confidence_threshold() -> f64 {
    0.80
}
fn default_max_audit_entries() -> usize {
    10_000
}
fn default_max_audit_age_days() -> u32 {
    30
}

impl Default for AuditConfig {
    fn default() -> Self {
        Self {
            scope: AuditScopeConfig::default(),
            confidence_threshold: default_confidence_threshold(),
            always_show_confidence: false,
            max_entries: default_max_audit_entries(),
            max_age_days: default_max_audit_age_days(),
        }
    }
}

impl Default for HeartbeatChecksConfig {
    fn default() -> Self {
        Self {
            stale_todos_enabled: true,
            stale_todo_threshold_hours: default_stale_todo_threshold_hours(),
            stuck_goals_enabled: true,
            stuck_goal_threshold_hours: default_stuck_goal_threshold_hours(),
            unreplied_messages_enabled: true,
            unreplied_message_threshold_hours: default_unreplied_threshold_hours(),
            repo_changes_enabled: true,
            stale_todos_cron: None,
            stuck_goals_cron: None,
            unreplied_messages_cron: None,
            repo_changes_cron: None,
            stale_todos_priority_weight: default_priority_weight(),
            stuck_goals_priority_weight: default_priority_weight(),
            unreplied_messages_priority_weight: default_priority_weight(),
            repo_changes_priority_weight: default_priority_weight(),
            stale_todos_priority_override: None,
            stuck_goals_priority_override: None,
            unreplied_messages_priority_override: None,
            repo_changes_priority_override: None,
            reset_learned_priorities: false,
        }
    }
}

/// Convert legacy heartbeat_interval_mins to a cron expression. Per D-08.
pub fn interval_mins_to_cron(mins: u64) -> String {
    match mins {
        0 | 1 => "* * * * *".to_string(),
        m if m <= 59 && 60 % m == 0 => format!("*/{} * * * *", m),
        60 => "0 * * * *".to_string(),
        m if m > 60 && m % 60 == 0 => format!("0 */{} * * *", m / 60),
        m => format!("*/{} * * * *", m.min(59)),
    }
}

// ---------------------------------------------------------------------------
// Notifications
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum NotificationSeverity {
    Info,
    Warning,
    Alert,
    Error,
}

// ---------------------------------------------------------------------------
// Persistent memory (SOUL.md, MEMORY.md, USER.md)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentMemory {
    pub soul: String,
    pub memory: String,
    pub user_profile: String,
}

// ---------------------------------------------------------------------------
// Generation stats helper
// ---------------------------------------------------------------------------

/// Compute tokens-per-second and generation duration from timing data.
/// Compute generation_ms and tokens-per-second from the elapsed duration and
/// output token count. Pass `first_token_at.unwrap_or(started_at).elapsed()`
/// as `generation_secs`.
pub fn compute_generation_stats(
    generation_secs: f64,
    output_tokens: u64,
) -> (Option<u64>, Option<f64>) {
    let generation_ms = Some((generation_secs * 1000.0).round() as u64);
    let tps = if output_tokens > 0 && generation_secs > 0.0 {
        Some(output_tokens as f64 / generation_secs)
    } else {
        None
    };
    (generation_ms, tps)
}

// ---------------------------------------------------------------------------
// LLM completion types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub enum CompletionChunk {
    Delta {
        content: String,
        reasoning: Option<String>,
    },
    ToolCalls {
        tool_calls: Vec<ToolCall>,
        content: Option<String>,
        reasoning: Option<String>,
        input_tokens: Option<u64>,
        output_tokens: Option<u64>,
        response_id: Option<String>,
        upstream_thread_id: Option<String>,
    },
    Done {
        content: String,
        reasoning: Option<String>,
        input_tokens: u64,
        output_tokens: u64,
        response_id: Option<String>,
        upstream_thread_id: Option<String>,
    },
    TransportFallback {
        from: ApiTransport,
        to: ApiTransport,
        message: String,
    },
    Retry {
        attempt: u32,
        max_retries: u32,
        delay_ms: u64,
        failure_class: String,
        message: String,
    },
    Error {
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    /// FOUN-05: Channel capacity is configurable via AgentConfig.
    #[test]
    fn configurable_channel_capacity() {
        // Test defaults
        let json_minimal = r#"{}"#;
        let parsed: AgentConfig = serde_json::from_str(json_minimal).unwrap();
        assert_eq!(parsed.pty_channel_capacity, 1024);
        assert_eq!(parsed.agent_event_channel_capacity, 512);
        assert!(parsed.auto_retry);
        assert_eq!(
            parsed.concierge.detail_level,
            ConciergeDetailLevel::ContextSummary
        );

        // Test serde roundtrip with custom values
        let json = r#"{"pty_channel_capacity": 2048, "agent_event_channel_capacity": 1024}"#;
        let parsed: AgentConfig = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.pty_channel_capacity, 2048);
        assert_eq!(parsed.agent_event_channel_capacity, 1024);
    }

    #[test]
    fn alibaba_coding_plan_uses_openai_by_default() {
        assert_eq!(
            get_provider_api_type(
                "alibaba-coding-plan",
                "qwen3.5-plus",
                "https://coding-intl.dashscope.aliyuncs.com/v1"
            ),
            ApiType::OpenAI
        );
    }

    #[test]
    fn alibaba_coding_plan_switches_to_anthropic_for_anthropic_base_url() {
        assert_eq!(
            get_provider_api_type(
                "alibaba-coding-plan",
                "qwen3.5-plus",
                "https://coding-intl.dashscope.aliyuncs.com/apps/anthropic"
            ),
            ApiType::Anthropic
        );
    }

    /// FOUN-04: Circuit breaker AgentEvent variants serialize and deserialize correctly.
    // -----------------------------------------------------------------------
    // Heartbeat type contract tests (BEAT-01, BEAT-02, BEAT-04, BEAT-05)
    // -----------------------------------------------------------------------

    #[test]
    fn heartbeat_checks_config_deserializes_from_empty_json() {
        let cfg: HeartbeatChecksConfig = serde_json::from_str("{}").unwrap();
        assert!(cfg.stale_todos_enabled);
        assert_eq!(cfg.stale_todo_threshold_hours, 24);
        assert!(cfg.stuck_goals_enabled);
        assert_eq!(cfg.stuck_goal_threshold_hours, 2);
        assert!(cfg.unreplied_messages_enabled);
        assert_eq!(cfg.unreplied_message_threshold_hours, 1);
        assert!(cfg.repo_changes_enabled);
        assert!(cfg.stale_todos_cron.is_none());
        assert!(cfg.stuck_goals_cron.is_none());
        assert!(cfg.unreplied_messages_cron.is_none());
        assert!(cfg.repo_changes_cron.is_none());
        // BEAT-06: Priority weight fields default to 1.0
        assert!((cfg.stale_todos_priority_weight - 1.0).abs() < f64::EPSILON);
        assert!((cfg.stuck_goals_priority_weight - 1.0).abs() < f64::EPSILON);
        assert!((cfg.unreplied_messages_priority_weight - 1.0).abs() < f64::EPSILON);
        assert!((cfg.repo_changes_priority_weight - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn heartbeat_checks_config_priority_overrides_default_none() {
        let cfg: HeartbeatChecksConfig = serde_json::from_str("{}").unwrap();
        assert!(cfg.stale_todos_priority_override.is_none());
        assert!(cfg.stuck_goals_priority_override.is_none());
        assert!(cfg.unreplied_messages_priority_override.is_none());
        assert!(cfg.repo_changes_priority_override.is_none());
        assert!(!cfg.reset_learned_priorities);
    }

    #[test]
    fn agent_config_adaptive_heartbeat_defaults() {
        let cfg: AgentConfig = serde_json::from_str("{}").unwrap();
        assert!((cfg.ema_alpha - 0.3).abs() < f64::EPSILON);
        assert_eq!(cfg.low_activity_frequency_factor, 4);
        assert!((cfg.ema_activity_threshold - 2.0).abs() < f64::EPSILON);
    }

    #[test]
    fn heartbeat_check_type_serializes_to_snake_case() {
        assert_eq!(
            serde_json::to_string(&HeartbeatCheckType::StaleTodos).unwrap(),
            "\"stale_todos\""
        );
        assert_eq!(
            serde_json::to_string(&HeartbeatCheckType::StuckGoalRuns).unwrap(),
            "\"stuck_goal_runs\""
        );
        assert_eq!(
            serde_json::to_string(&HeartbeatCheckType::UnrepliedGatewayMessages).unwrap(),
            "\"unreplied_gateway_messages\""
        );
        assert_eq!(
            serde_json::to_string(&HeartbeatCheckType::RepoChanges).unwrap(),
            "\"repo_changes\""
        );
    }

    #[test]
    fn check_severity_serializes_to_snake_case() {
        assert_eq!(
            serde_json::to_string(&CheckSeverity::Low).unwrap(),
            "\"low\""
        );
        assert_eq!(
            serde_json::to_string(&CheckSeverity::Medium).unwrap(),
            "\"medium\""
        );
        assert_eq!(
            serde_json::to_string(&CheckSeverity::High).unwrap(),
            "\"high\""
        );
        assert_eq!(
            serde_json::to_string(&CheckSeverity::Critical).unwrap(),
            "\"critical\""
        );
    }

    #[test]
    fn heartbeat_check_result_roundtrips_through_serde() {
        let result = HeartbeatCheckResult {
            check_type: HeartbeatCheckType::StaleTodos,
            items_found: 2,
            summary: "2 stale TODO(s)".to_string(),
            details: vec![CheckDetail {
                id: "todo-1".to_string(),
                label: "Fix the bug".to_string(),
                age_hours: 48.5,
                severity: CheckSeverity::High,
                context: "TODO pending for 48.5h".to_string(),
            }],
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: HeartbeatCheckResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.check_type, HeartbeatCheckType::StaleTodos);
        assert_eq!(parsed.items_found, 2);
        assert_eq!(parsed.details.len(), 1);
        assert_eq!(parsed.details[0].severity, CheckSeverity::High);
    }

    #[test]
    fn heartbeat_digest_item_roundtrips_through_serde() {
        let item = HeartbeatDigestItem {
            priority: 1,
            check_type: HeartbeatCheckType::StuckGoalRuns,
            title: "Stuck goal run".to_string(),
            suggestion: "Consider cancelling".to_string(),
        };
        let json = serde_json::to_string(&item).unwrap();
        let parsed: HeartbeatDigestItem = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.priority, 1);
        assert_eq!(parsed.check_type, HeartbeatCheckType::StuckGoalRuns);
        assert_eq!(parsed.title, "Stuck goal run");
        assert_eq!(parsed.suggestion, "Consider cancelling");
    }

    #[test]
    fn agent_config_backward_compat_new_heartbeat_fields() {
        // JSON missing heartbeat_cron, heartbeat_checks, quiet_hours, dnd_enabled
        let json = r#"{}"#;
        let parsed: AgentConfig = serde_json::from_str(json).unwrap();
        assert!(parsed.heartbeat_cron.is_none());
        assert!(parsed.heartbeat_checks.stale_todos_enabled);
        assert_eq!(parsed.heartbeat_checks.stale_todo_threshold_hours, 24);
        assert!(parsed.quiet_hours_start.is_none());
        assert!(parsed.quiet_hours_end.is_none());
        assert!(!parsed.dnd_enabled);
    }

    #[test]
    fn agent_event_heartbeat_digest_serde_roundtrip() {
        let event = AgentEvent::HeartbeatDigest {
            cycle_id: "cycle-1".to_string(),
            actionable: true,
            digest: "2 items need attention".to_string(),
            items: vec![HeartbeatDigestItem {
                priority: 1,
                check_type: HeartbeatCheckType::StaleTodos,
                title: "Stale todos".to_string(),
                suggestion: "Review pending items".to_string(),
            }],
            checked_at: 1234567890,
            explanation: Some("Heartbeat found stale items".to_string()),
            confidence: Some(0.85),
        };
        let json = serde_json::to_string(&event).unwrap();
        let parsed: AgentEvent = serde_json::from_str(&json).unwrap();
        match parsed {
            AgentEvent::HeartbeatDigest {
                cycle_id,
                actionable,
                digest,
                items,
                checked_at,
                explanation,
                confidence,
            } => {
                assert_eq!(cycle_id, "cycle-1");
                assert!(actionable);
                assert_eq!(digest, "2 items need attention");
                assert_eq!(items.len(), 1);
                assert_eq!(checked_at, 1234567890);
                assert_eq!(explanation.as_deref(), Some("Heartbeat found stale items"));
                assert!((confidence.unwrap() - 0.85).abs() < f64::EPSILON);
            }
            _ => panic!("wrong variant after deserialize"),
        }
    }

    #[test]
    fn interval_mins_to_cron_converts_correctly() {
        assert_eq!(interval_mins_to_cron(1), "* * * * *");
        assert_eq!(interval_mins_to_cron(15), "*/15 * * * *");
        assert_eq!(interval_mins_to_cron(60), "0 * * * *");
        assert_eq!(interval_mins_to_cron(120), "0 */2 * * *");
        assert_eq!(interval_mins_to_cron(0), "* * * * *");
    }

    #[test]
    fn circuit_breaker_event_serde_roundtrip() {
        let event = AgentEvent::ProviderCircuitOpen {
            provider: "openai".to_string(),
            failed_model: None,
            trip_count: 3,
            reason: "circuit breaker open".to_string(),
            suggested_alternatives: vec![ProviderAlternativeSuggestion {
                provider_id: "groq".to_string(),
                model: Some("llama-3.3-70b-versatile".to_string()),
                reason: "healthy and configured".to_string(),
            }],
        };
        let json = serde_json::to_string(&event).unwrap();
        let parsed: AgentEvent = serde_json::from_str(&json).unwrap();
        match parsed {
            AgentEvent::ProviderCircuitOpen {
                provider,
                failed_model,
                trip_count,
                reason,
                suggested_alternatives,
            } => {
                assert_eq!(provider, "openai");
                assert!(failed_model.is_none());
                assert_eq!(trip_count, 3);
                assert_eq!(reason, "circuit breaker open");
                assert_eq!(suggested_alternatives.len(), 1);
                assert_eq!(suggested_alternatives[0].provider_id, "groq");
            }
            _ => panic!("wrong variant after deserialize"),
        }

        let recovery = AgentEvent::ProviderCircuitRecovered {
            provider: "anthropic".to_string(),
        };
        let json = serde_json::to_string(&recovery).unwrap();
        let parsed: AgentEvent = serde_json::from_str(&json).unwrap();
        match parsed {
            AgentEvent::ProviderCircuitRecovered { provider } => {
                assert_eq!(provider, "anthropic");
            }
            _ => panic!("wrong variant after deserialize"),
        }
    }

    #[test]
    fn circuit_breaker_event_deserializes_richer_outage_metadata() {
        let json = serde_json::json!({
            "type": "provider_circuit_open",
            "provider": "openai",
            "failed_model": "gpt-4o",
            "trip_count": 4,
            "reason": "circuit breaker open after repeated failures",
            "suggested_alternatives": [
                {
                    "provider_id": "groq",
                    "model": "llama-3.3-70b-versatile",
                    "reason": "healthy and configured"
                }
            ]
        });

        let parsed: AgentEvent = serde_json::from_value(json).unwrap();
        match parsed {
            AgentEvent::ProviderCircuitOpen {
                provider,
                failed_model,
                trip_count,
                reason,
                suggested_alternatives,
            } => {
                assert_eq!(provider, "openai");
                assert_eq!(failed_model.as_deref(), Some("gpt-4o"));
                assert_eq!(trip_count, 4);
                assert_eq!(reason, "circuit breaker open after repeated failures");
                assert_eq!(suggested_alternatives.len(), 1);
                assert_eq!(suggested_alternatives[0].provider_id, "groq");
                assert_eq!(
                    suggested_alternatives[0].model.as_deref(),
                    Some("llama-3.3-70b-versatile")
                );
            }
            _ => panic!("wrong variant after deserialize"),
        }
    }

    #[test]
    fn circuit_breaker_event_deserializes_legacy_shape_without_reason() {
        let json = serde_json::json!({
            "type": "provider_circuit_open",
            "provider": "openai",
            "trip_count": 2
        });

        let parsed: AgentEvent = serde_json::from_value(json).unwrap();
        match parsed {
            AgentEvent::ProviderCircuitOpen {
                provider,
                failed_model,
                trip_count,
                reason,
                suggested_alternatives,
            } => {
                assert_eq!(provider, "openai");
                assert!(failed_model.is_none());
                assert_eq!(trip_count, 2);
                assert_eq!(reason, "circuit breaker open");
                assert!(suggested_alternatives.is_empty());
            }
            _ => panic!("wrong variant after deserialize"),
        }
    }

    // ── Consolidation type tests (Phase 5 — MEMO-02/MEMO-07) ────────────

    #[test]
    fn consolidation_config_defaults() {
        let cfg = ConsolidationConfig::default();
        assert!(cfg.enabled);
        assert_eq!(cfg.budget_secs, 30);
        assert_eq!(cfg.idle_threshold_secs, 300);
        assert_eq!(cfg.tombstone_ttl_days, 7);
        assert_eq!(cfg.heuristic_promotion_threshold, 3);
        assert!((cfg.memory_decay_half_life_hours - 69.0).abs() < f64::EPSILON);
        assert!(!cfg.auto_resume_goal_runs);
        assert!((cfg.fact_decay_supersede_threshold - 0.2).abs() < f64::EPSILON);
    }

    #[test]
    fn consolidation_config_deserializes_from_empty_json() {
        let cfg: ConsolidationConfig = serde_json::from_str("{}").unwrap();
        assert!(cfg.enabled);
        assert_eq!(cfg.budget_secs, 30);
        assert_eq!(cfg.idle_threshold_secs, 300);
    }

    #[test]
    fn consolidation_config_on_agent_config() {
        let json = r#"{}"#;
        let parsed: AgentConfig = serde_json::from_str(json).unwrap();
        assert!(parsed.consolidation.enabled);
        assert_eq!(parsed.consolidation.budget_secs, 30);
    }

    #[test]
    fn consolidation_result_defaults_are_zero() {
        let result = ConsolidationResult::default();
        assert_eq!(result.traces_reviewed, 0);
        assert_eq!(result.facts_decayed, 0);
        assert_eq!(result.tombstones_purged, 0);
        assert_eq!(result.facts_refined, 0);
        assert!(result.skipped_reason.is_none());
    }

    // -----------------------------------------------------------------------
    // Skill discovery type contract tests (SKIL-01, SKIL-02, SKIL-03, SKIL-05)
    // -----------------------------------------------------------------------

    #[test]
    fn skill_maturity_status_from_str_roundtrip() {
        let draft = serde_json::from_str::<SkillMaturityStatus>(r#""draft""#).unwrap();
        assert_eq!(draft, SkillMaturityStatus::Draft);
        let json = serde_json::to_string(&draft).unwrap();
        let roundtripped: SkillMaturityStatus = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtripped, SkillMaturityStatus::Draft);
    }

    #[test]
    fn skill_maturity_status_serde_snake_case() {
        assert_eq!(
            serde_json::to_string(&SkillMaturityStatus::Draft).unwrap(),
            r#""draft""#
        );
        assert_eq!(
            serde_json::to_string(&SkillMaturityStatus::Testing).unwrap(),
            r#""testing""#
        );
        assert_eq!(
            serde_json::to_string(&SkillMaturityStatus::Active).unwrap(),
            r#""active""#
        );
        assert_eq!(
            serde_json::to_string(&SkillMaturityStatus::Proven).unwrap(),
            r#""proven""#
        );
        assert_eq!(
            serde_json::to_string(&SkillMaturityStatus::PromotedToCanonical).unwrap(),
            r#""promoted_to_canonical""#
        );
    }

    #[test]
    fn skill_promotion_config_defaults() {
        let cfg = SkillPromotionConfig::default();
        assert_eq!(cfg.testing_to_active, 3);
        assert_eq!(cfg.active_to_proven, 5);
        assert_eq!(cfg.proven_to_canonical, 10);
    }

    #[test]
    fn skill_promotion_config_deserializes_from_empty_json() {
        let cfg: SkillPromotionConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(cfg.testing_to_active, 3);
        assert_eq!(cfg.active_to_proven, 5);
        assert_eq!(cfg.proven_to_canonical, 10);
    }

    #[test]
    fn skill_discovery_config_defaults() {
        let cfg = SkillDiscoveryConfig::default();
        assert_eq!(cfg.min_tool_count, 8);
        assert_eq!(cfg.min_replan_count, 1);
        assert!((cfg.min_quality_score - 0.8).abs() < f64::EPSILON);
        assert!((cfg.novelty_similarity_threshold - 0.7).abs() < f64::EPSILON);
    }

    #[test]
    fn skill_discovery_config_deserializes_from_empty_json() {
        let cfg: SkillDiscoveryConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(cfg.min_tool_count, 8);
        assert_eq!(cfg.min_replan_count, 1);
    }

    #[test]
    fn consolidation_result_has_skill_fields() {
        let result = ConsolidationResult::default();
        assert_eq!(result.skill_candidates_flagged, 0);
        assert_eq!(result.skills_drafted, 0);
        assert_eq!(result.skills_tested, 0);
        assert_eq!(result.skills_promoted, 0);
    }

    #[test]
    fn heartbeat_check_type_skill_lifecycle_serializes() {
        let check = HeartbeatCheckType::SkillLifecycle;
        assert_eq!(
            serde_json::to_string(&check).unwrap(),
            r#""skill_lifecycle""#
        );
        let roundtripped: HeartbeatCheckType =
            serde_json::from_str(r#""skill_lifecycle""#).unwrap();
        assert_eq!(roundtripped, HeartbeatCheckType::SkillLifecycle);
    }

    #[test]
    fn skill_maturity_status_as_str() {
        assert_eq!(SkillMaturityStatus::Draft.as_str(), "draft");
        assert_eq!(SkillMaturityStatus::Testing.as_str(), "testing");
        assert_eq!(SkillMaturityStatus::Active.as_str(), "active");
        assert_eq!(SkillMaturityStatus::Proven.as_str(), "proven");
        assert_eq!(
            SkillMaturityStatus::PromotedToCanonical.as_str(),
            "promoted_to_canonical"
        );
    }

    #[test]
    fn skill_maturity_status_from_status_str() {
        assert_eq!(
            SkillMaturityStatus::from_status_str("draft"),
            Some(SkillMaturityStatus::Draft)
        );
        assert_eq!(
            SkillMaturityStatus::from_status_str("testing"),
            Some(SkillMaturityStatus::Testing)
        );
        assert_eq!(
            SkillMaturityStatus::from_status_str("active"),
            Some(SkillMaturityStatus::Active)
        );
        assert_eq!(
            SkillMaturityStatus::from_status_str("proven"),
            Some(SkillMaturityStatus::Proven)
        );
        assert_eq!(
            SkillMaturityStatus::from_status_str("promoted_to_canonical"),
            Some(SkillMaturityStatus::PromotedToCanonical)
        );
        assert_eq!(
            SkillMaturityStatus::from_status_str("promoted-to-canonical"),
            Some(SkillMaturityStatus::PromotedToCanonical)
        );
        assert_eq!(SkillMaturityStatus::from_status_str("bogus"), None);
    }

    #[test]
    fn agent_config_has_skill_discovery_and_promotion() {
        let cfg: AgentConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(cfg.skill_discovery.min_tool_count, 8);
        assert_eq!(cfg.skill_promotion.testing_to_active, 3);
    }
}
