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
        id: "arcee-ai/trinity-large-thinking",
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
