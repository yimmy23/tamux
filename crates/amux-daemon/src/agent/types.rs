use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Provider definitions (static registry)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiType {
    OpenAI,
    Anthropic,
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

#[derive(Debug, Clone, Serialize)]
pub struct ModelDefinition {
    pub id: &'static str,
    pub name: &'static str,
    pub context_window: u32,
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
    },
    ModelDefinition {
        id: "gpt-5.4-mini",
        name: "GPT-5.4 Mini",
        context_window: 400000,
    },
    ModelDefinition {
        id: "gpt-5.4-nano",
        name: "GPT-5.4 Nano",
        context_window: 400000,
    },
    ModelDefinition {
        id: "gpt-5.3-codex",
        name: "GPT-5.3 Codex",
        context_window: 400000,
    },
    ModelDefinition {
        id: "gpt-5.2-codex",
        name: "GPT-5.2 Codex",
        context_window: 400000,
    },
    ModelDefinition {
        id: "gpt-5.2",
        name: "GPT-5.2",
        context_window: 400000,
    },
    ModelDefinition {
        id: "gpt-5.1-codex-max",
        name: "GPT-5.1 Codex Max",
        context_window: 400000,
    },
    ModelDefinition {
        id: "gpt-5.1-codex",
        name: "GPT-5.1 Codex",
        context_window: 400000,
    },
    ModelDefinition {
        id: "gpt-5.1-codex-mini",
        name: "GPT-5.1 Codex Mini",
        context_window: 400000,
    },
    ModelDefinition {
        id: "gpt-5.1",
        name: "GPT-5.1",
        context_window: 400000,
    },
    ModelDefinition {
        id: "gpt-5-codex",
        name: "GPT-5 Codex",
        context_window: 400000,
    },
    ModelDefinition {
        id: "gpt-5-codex-mini",
        name: "GPT-5 Codex Mini",
        context_window: 200000,
    },
    ModelDefinition {
        id: "gpt-5",
        name: "GPT-5",
        context_window: 400000,
    },
    ModelDefinition {
        id: "codex-mini-latest",
        name: "Codex Mini Latest",
        context_window: 200000,
    },
    ModelDefinition {
        id: "o3",
        name: "o3",
        context_window: 200000,
    },
    ModelDefinition {
        id: "o4-mini",
        name: "o4 Mini",
        context_window: 200000,
    },
];

pub const QWEN_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: "qwen-max",
        name: "Qwen Max",
        context_window: 32768,
    },
    ModelDefinition {
        id: "qwen-plus",
        name: "Qwen Plus",
        context_window: 32768,
    },
    ModelDefinition {
        id: "qwen-turbo",
        name: "Qwen Turbo",
        context_window: 8192,
    },
    ModelDefinition {
        id: "qwen-long",
        name: "Qwen Long",
        context_window: 1000000,
    },
];

pub const ZAI_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: "glm-5",
        name: "GLM-5",
        context_window: 128000,
    },
    ModelDefinition {
        id: "glm-4-plus",
        name: "GLM-4 Plus",
        context_window: 128000,
    },
    ModelDefinition {
        id: "glm-4",
        name: "GLM-4",
        context_window: 128000,
    },
    ModelDefinition {
        id: "glm-4-air",
        name: "GLM-4 Air",
        context_window: 128000,
    },
    ModelDefinition {
        id: "glm-4-flash",
        name: "GLM-4 Flash",
        context_window: 128000,
    },
];

pub const KIMI_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: "moonshot-v1-8k",
        name: "Moonshot V1 8K",
        context_window: 8192,
    },
    ModelDefinition {
        id: "moonshot-v1-32k",
        name: "Moonshot V1 32K",
        context_window: 32768,
    },
    ModelDefinition {
        id: "moonshot-v1-128k",
        name: "Moonshot V1 128K",
        context_window: 131072,
    },
];

pub const KIMI_CODING_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: "kimi-for-coding",
        name: "Kimi for Coding",
        context_window: 262144,
    },
    ModelDefinition {
        id: "kimi-k2.5",
        name: "Kimi K2.5",
        context_window: 262144,
    },
    ModelDefinition {
        id: "kimi-k2-turbo-preview",
        name: "Kimi K2 Turbo Preview",
        context_window: 262144,
    },
];

pub const MINIMAX_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: "MiniMax-M2.7",
        name: "MiniMax M2.7",
        context_window: 205000,
    },
    ModelDefinition {
        id: "MiniMax-M2.5",
        name: "MiniMax M2.5",
        context_window: 205000,
    },
    ModelDefinition {
        id: "MiniMax-M2.5-highspeed",
        name: "MiniMax M2.5 High Speed",
        context_window: 205000,
    },
    ModelDefinition {
        id: "MiniMax-M1-80k",
        name: "MiniMax M1 80K",
        context_window: 80000,
    },
];

pub const ALIBABA_CODING_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: "qwen3-coder",
        name: "Qwen3 Coder",
        context_window: 128000,
    },
    ModelDefinition {
        id: "qwen3-coder-next",
        name: "Qwen3 Coder Next",
        context_window: 128000,
    },
    ModelDefinition {
        id: "qwen3.5-plus",
        name: "Qwen3.5 Plus",
        context_window: 128000,
    },
    ModelDefinition {
        id: "glm-5",
        name: "GLM-5",
        context_window: 128000,
    },
    ModelDefinition {
        id: "kimi-k2.5",
        name: "Kimi K2.5",
        context_window: 262144,
    },
    ModelDefinition {
        id: "MiniMax-M2.5",
        name: "MiniMax M2.5",
        context_window: 205000,
    },
];

pub const OPENCODE_ZEN_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: "claude-opus-4-6",
        name: "Claude Opus 4.6",
        context_window: 200000,
    },
    ModelDefinition {
        id: "claude-sonnet-4-5",
        name: "Claude Sonnet 4.5",
        context_window: 200000,
    },
    ModelDefinition {
        id: "claude-sonnet-4",
        name: "Claude Sonnet 4",
        context_window: 200000,
    },
    ModelDefinition {
        id: "gpt-5.4",
        name: "GPT-5.4",
        context_window: 128000,
    },
    ModelDefinition {
        id: "gpt-5.4-mini",
        name: "GPT-5.4 Mini",
        context_window: 128000,
    },
    ModelDefinition {
        id: "gpt-5.3-codex",
        name: "GPT-5.3 Codex",
        context_window: 128000,
    },
    ModelDefinition {
        id: "minimax-m2.5",
        name: "MiniMax M2.5",
        context_window: 205000,
    },
    ModelDefinition {
        id: "glm-5",
        name: "GLM-5",
        context_window: 128000,
    },
    ModelDefinition {
        id: "kimi-k2.5",
        name: "Kimi K2.5",
        context_window: 262144,
    },
];

pub const OPENROUTER_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: "anthropic/claude-sonnet-4",
        name: "Claude Sonnet 4",
        context_window: 200000,
    },
    ModelDefinition {
        id: "anthropic/claude-3.5-sonnet",
        name: "Claude 3.5 Sonnet",
        context_window: 200000,
    },
    ModelDefinition {
        id: "openai/gpt-4o",
        name: "GPT-4o",
        context_window: 128000,
    },
    ModelDefinition {
        id: "google/gemini-pro-1.5",
        name: "Gemini Pro 1.5",
        context_window: 1000000,
    },
    ModelDefinition {
        id: "meta-llama/llama-3.3-70b-instruct",
        name: "Llama 3.3 70B",
        context_window: 128000,
    },
    ModelDefinition {
        id: "deepseek/deepseek-chat",
        name: "DeepSeek Chat",
        context_window: 64000,
    },
];

pub const GROQ_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: "llama-3.3-70b-versatile",
        name: "Llama 3.3 70B Versatile",
        context_window: 128000,
    },
    ModelDefinition {
        id: "llama-3.3-70b-specdec",
        name: "Llama 3.3 70B Speculative",
        context_window: 8192,
    },
    ModelDefinition {
        id: "llama-3.1-8b-instant",
        name: "Llama 3.1 8B",
        context_window: 128000,
    },
    ModelDefinition {
        id: "mixtral-8x7b-32768",
        name: "Mixtral 8x7B",
        context_window: 32768,
    },
];

pub const CEREBRAS_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: "llama-3.3-70b",
        name: "Llama 3.3 70B",
        context_window: 128000,
    },
    ModelDefinition {
        id: "llama-3.1-8b",
        name: "Llama 3.1 8B",
        context_window: 128000,
    },
];

pub const TOGETHER_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: "meta-llama/Llama-3.3-70B-Instruct-Turbo",
        name: "Llama 3.3 70B Turbo",
        context_window: 128000,
    },
    ModelDefinition {
        id: "meta-llama/Llama-3.2-90B-Vision-Instruct-Turbo",
        name: "Llama 3.2 90B Vision",
        context_window: 131072,
    },
    ModelDefinition {
        id: "Qwen/Qwen2.5-72B-Instruct-Turbo",
        name: "Qwen 2.5 72B Turbo",
        context_window: 32768,
    },
    ModelDefinition {
        id: "deepseek-ai/DeepSeek-V3",
        name: "DeepSeek V3",
        context_window: 128000,
    },
];

pub const OLLAMA_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: "llama3.1",
        name: "Llama 3.1",
        context_window: 128000,
    },
    ModelDefinition {
        id: "llama3.2",
        name: "Llama 3.2",
        context_window: 128000,
    },
    ModelDefinition {
        id: "qwen2.5",
        name: "Qwen 2.5",
        context_window: 128000,
    },
    ModelDefinition {
        id: "codellama",
        name: "Code Llama",
        context_window: 16384,
    },
    ModelDefinition {
        id: "mistral",
        name: "Mistral",
        context_window: 32768,
    },
];

pub const CHUTES_MODELS: &[ModelDefinition] = &[ModelDefinition {
    id: "deepseek-ai/DeepSeek-V3",
    name: "DeepSeek V3",
    context_window: 128000,
}];

pub const HUGGINGFACE_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: "meta-llama/Llama-3.3-70B-Instruct",
        name: "Llama 3.3 70B",
        context_window: 128000,
    },
    ModelDefinition {
        id: "Qwen/Qwen2.5-72B-Instruct",
        name: "Qwen 2.5 72B",
        context_window: 32768,
    },
    ModelDefinition {
        id: "mistralai/Mistral-7B-Instruct-v0.3",
        name: "Mistral 7B",
        context_window: 32768,
    },
];

pub const FEATHERLESS_MODELS: &[ModelDefinition] = &[
    ModelDefinition {
        id: "meta-llama/Llama-3.3-70B-Instruct",
        name: "Llama 3.3 70B",
        context_window: 128000,
    },
    ModelDefinition {
        id: "Qwen/Qwen2.5-72B-Instruct",
        name: "Qwen 2.5 72B",
        context_window: 32768,
    },
    ModelDefinition {
        id: "mistralai/Mistral-Small-24B-Instruct-2501",
        name: "Mistral Small 24B",
        context_window: 32768,
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
        default_base_url: "https://api.qwen.com/v1",
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
        auth_method: AuthMethod::Bearer,
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
        auth_method: AuthMethod::Bearer,
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
        default_model: "qwen3-coder",
        api_type: ApiType::OpenAI,
        auth_method: AuthMethod::Bearer,
        models: ALIBABA_CODING_MODELS,
        supports_model_fetch: true,
        anthropic_base_url: Some("https://coding-intl.dashscope.aliyuncs.com/apps/anthropic"),
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

pub fn get_provider_api_type(provider_id: &str, model: &str) -> ApiType {
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
// Agent configuration (persisted to ~/.tamux/agent/config.json)
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
    /// Additional persisted agent settings used by richer frontends and the TUI.
    #[serde(flatten)]
    pub extra: HashMap<String, Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GatewayConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub slack_token: String,
    #[serde(default)]
    pub telegram_token: String,
    #[serde(default)]
    pub discord_token: String,
    #[serde(default)]
    pub command_prefix: String,
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
    "You are tamux, an always-on agentic terminal multiplexer assistant. You can execute terminal commands, monitor systems, and send messages to connected chat platforms. Use your tools proactively. Be concise and direct.".into()
}
fn default_reasoning_effort() -> String {
    "high".into()
}
fn default_max_tool_loops() -> u32 {
    0
}
fn default_max_retries() -> u32 {
    3
}
fn default_retry_delay_ms() -> u64 {
    2000
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
fn default_heartbeat_mins() -> u64 {
    30
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
            auth_source: default_auth_source(),
            api_transport: default_api_transport(),
            reasoning_effort: default_reasoning_effort(),
            system_prompt: default_system_prompt(),
            max_tool_loops: default_max_tool_loops(),
            max_retries: default_max_retries(),
            retry_delay_ms: default_retry_delay_ms(),
            auto_compact_context: default_auto_compact_context(),
            max_context_messages: default_max_context_messages(),
            context_budget_tokens: default_context_budget_tokens(),
            context_window_tokens: default_context_window_tokens(),
            compact_threshold_pct: default_compact_threshold_pct(),
            keep_recent_on_compact: default_keep_recent_on_compact(),
            task_poll_interval_secs: default_task_poll_secs(),
            heartbeat_interval_mins: default_heartbeat_mins(),
            tools: ToolsConfig::default(),
            providers: HashMap::new(),
            gateway: GatewayConfig::default(),
            agent_backend: AgentBackend::default(),
            sub_agents: Vec::new(),
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
    HeartbeatResult {
        item_id: String,
        result: HeartbeatOutcome,
        message: String,
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
}

// ---------------------------------------------------------------------------
// Threads & messages
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentThread {
    pub id: String,
    pub title: String,
    pub messages: Vec<AgentMessage>,
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

impl AgentMessage {
    pub fn user(content: impl Into<String>, now: u64) -> Self {
        Self {
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

#[derive(Debug, Default, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GoalRunStepKind {
    #[default]
    Reason,
    Command,
    Research,
    Memory,
    Skill,
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
    },
    Error {
        message: String,
    },
}
