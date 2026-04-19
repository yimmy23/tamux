#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProviderRef {
    pub id: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AudioToolKind {
    SpeechToText,
    TextToSpeech,
}

pub const PROVIDER_ID_ALIBABA_CODING_PLAN: &str = "alibaba-coding-plan";
pub const PROVIDER_ID_ANTHROPIC: &str = "anthropic";
pub const PROVIDER_ID_ARCEE: &str = "arcee";
pub const PROVIDER_ID_AZURE_OPENAI: &str = "azure-openai";
pub const PROVIDER_ID_CEREBRAS: &str = "cerebras";
pub const PROVIDER_ID_CHUTES: &str = "chutes";
pub const PROVIDER_ID_CUSTOM: &str = "custom";
pub const PROVIDER_ID_FEATHERLESS: &str = "featherless";
pub const PROVIDER_ID_GITHUB_COPILOT: &str = "github-copilot";
pub const PROVIDER_ID_GROQ: &str = "groq";
pub const PROVIDER_ID_HUGGINGFACE: &str = "huggingface";
pub const PROVIDER_ID_KIMI: &str = "kimi";
pub const PROVIDER_ID_KIMI_CODING_PLAN: &str = "kimi-coding-plan";
pub const PROVIDER_ID_LMSTUDIO: &str = "lmstudio";
pub const PROVIDER_ID_MINIMAX: &str = "minimax";
pub const PROVIDER_ID_MINIMAX_CODING_PLAN: &str = "minimax-coding-plan";
pub const PROVIDER_ID_NVIDIA: &str = "nvidia";
pub const PROVIDER_ID_NOUS_PORTAL: &str = "nous-portal";
pub const PROVIDER_ID_OLLAMA: &str = "ollama";
pub const PROVIDER_ID_OPENAI: &str = "openai";
pub const PROVIDER_ID_CHATGPT_SUBSCRIPTION: &str = "chatgpt_subscription";
pub const PROVIDER_ID_OPENCODE_ZEN: &str = "opencode-zen";
pub const PROVIDER_ID_OPENROUTER: &str = "openrouter";
pub const PROVIDER_ID_QWEN: &str = "qwen";
pub const PROVIDER_ID_QWEN_DEEPINFRA: &str = "qwen-deepinfra";
pub const PROVIDER_ID_TOGETHER: &str = "together";
pub const PROVIDER_ID_XIAOMI_MIMO_TOKEN_PLAN: &str = "xiaomi-mimo-token-plan";
pub const PROVIDER_ID_Z_AI: &str = "z.ai";
pub const PROVIDER_ID_Z_AI_CODING_PLAN: &str = "z.ai-coding-plan";

pub const ALIBABA_CODING_PLAN_PROVIDER: ProviderRef = ProviderRef {
    id: PROVIDER_ID_ALIBABA_CODING_PLAN,
};
pub const ANTHROPIC_PROVIDER: ProviderRef = ProviderRef {
    id: PROVIDER_ID_ANTHROPIC,
};
pub const AZURE_OPENAI_PROVIDER: ProviderRef = ProviderRef {
    id: PROVIDER_ID_AZURE_OPENAI,
};
pub const CUSTOM_PROVIDER: ProviderRef = ProviderRef {
    id: PROVIDER_ID_CUSTOM,
};
pub const GITHUB_COPILOT_PROVIDER: ProviderRef = ProviderRef {
    id: PROVIDER_ID_GITHUB_COPILOT,
};
pub const MINIMAX_PROVIDER: ProviderRef = ProviderRef {
    id: PROVIDER_ID_MINIMAX,
};
pub const MINIMAX_CODING_PLAN_PROVIDER: ProviderRef = ProviderRef {
    id: PROVIDER_ID_MINIMAX_CODING_PLAN,
};
pub const NVIDIA_PROVIDER: ProviderRef = ProviderRef {
    id: PROVIDER_ID_NVIDIA,
};
pub const NOUS_PORTAL_PROVIDER: ProviderRef = ProviderRef {
    id: PROVIDER_ID_NOUS_PORTAL,
};
pub const OPENAI_PROVIDER: ProviderRef = ProviderRef {
    id: PROVIDER_ID_OPENAI,
};
pub const OPENCODE_ZEN_PROVIDER: ProviderRef = ProviderRef {
    id: PROVIDER_ID_OPENCODE_ZEN,
};
pub const OPENROUTER_PROVIDER: ProviderRef = ProviderRef {
    id: PROVIDER_ID_OPENROUTER,
};
pub const QWEN_PROVIDER: ProviderRef = ProviderRef {
    id: PROVIDER_ID_QWEN,
};
pub const XIAOMI_MIMO_TOKEN_PLAN_PROVIDER: ProviderRef = ProviderRef {
    id: PROVIDER_ID_XIAOMI_MIMO_TOKEN_PLAN,
};

pub fn provider_supports_audio_tool(provider_id: &str, _kind: AudioToolKind) -> bool {
    matches!(
        provider_id,
        PROVIDER_ID_CUSTOM
            | PROVIDER_ID_OPENAI
            | PROVIDER_ID_AZURE_OPENAI
            | PROVIDER_ID_GROQ
            | PROVIDER_ID_OPENROUTER
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openrouter_is_marked_as_audio_capable_for_both_tools() {
        assert!(provider_supports_audio_tool(
            PROVIDER_ID_OPENROUTER,
            AudioToolKind::SpeechToText,
        ));
        assert!(provider_supports_audio_tool(
            PROVIDER_ID_OPENROUTER,
            AudioToolKind::TextToSpeech,
        ));
    }

    #[test]
    fn anthropic_is_not_marked_as_direct_audio_tool_provider() {
        assert!(!provider_supports_audio_tool(
            PROVIDER_ID_ANTHROPIC,
            AudioToolKind::SpeechToText,
        ));
        assert!(!provider_supports_audio_tool(
            PROVIDER_ID_ANTHROPIC,
            AudioToolKind::TextToSpeech,
        ));
    }
}
