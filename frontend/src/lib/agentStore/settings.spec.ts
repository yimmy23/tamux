import {
  DEFAULT_AGENT_SETTINGS,
  normalizeAgentSettingsFromSource,
} from "./settings.ts";
import { buildDaemonAgentConfig } from "../agentDaemonConfig.ts";

function assert(condition: unknown, message: string): void {
  if (!condition) {
    throw new Error(message);
  }
}

assert(
  DEFAULT_AGENT_SETTINGS.weles_max_concurrent_reviews === 2,
  "Default WELES review concurrency should be 2",
);

const normalized = normalizeAgentSettingsFromSource({
  builtin_sub_agents: {
    weles: {
      max_concurrent_reviews: 6,
    },
  },
});

assert(
  normalized.weles_max_concurrent_reviews === 6,
  "Settings normalization should read builtin WELES concurrency overrides",
);

assert(
  DEFAULT_AGENT_SETTINGS.llm_stream_chunk_timeout_secs === 300,
  "Default LLM stream chunk timeout should be 300 seconds",
);

assert(
  DEFAULT_AGENT_SETTINGS.react_chat_history_page_size === 100,
  "Default React chat history page size should be 100 messages",
);

assert(
  DEFAULT_AGENT_SETTINGS.tui_chat_history_page_size === 100,
  "Default TUI chat history page size should be 100 messages",
);

assert(
  DEFAULT_AGENT_SETTINGS.skill_recommendation.enabled === true,
  "Default skill recommendation gate should be enabled",
);

assert(
  DEFAULT_AGENT_SETTINGS.skill_recommendation.background_community_search === true,
  "Default background community skill discovery should be enabled",
);

assert(
  DEFAULT_AGENT_SETTINGS.skill_recommendation.community_preapprove_timeout_secs === 30,
  "Default skill recommendation preapprove timeout should be 30 seconds",
);

const normalizedTimeout = normalizeAgentSettingsFromSource({
  llm_stream_chunk_timeout_secs: 420,
});

assert(
  normalizedTimeout.llm_stream_chunk_timeout_secs === 420,
  "Settings normalization should preserve LLM stream chunk timeout overrides",
);

const normalizedHistoryPageSizes = normalizeAgentSettingsFromSource({
  react_chat_history_page_size: 0,
  tui_chat_history_page_size: 222,
});

assert(
  normalizedHistoryPageSizes.react_chat_history_page_size === 0,
  "Settings normalization should preserve the React unlimited history sentinel",
);

assert(
  normalizedHistoryPageSizes.tui_chat_history_page_size === 222,
  "Settings normalization should preserve the TUI history page size override",
);

const normalizedSkillRecommendation = normalizeAgentSettingsFromSource({
  skill_recommendation: {
    enabled: false,
    background_community_search: false,
    community_preapprove_timeout_secs: 45,
    suggest_global_enable_after_approvals: 5,
  },
});

assert(
  normalizedSkillRecommendation.skill_recommendation.enabled === false,
  "Settings normalization should read skill recommendation enablement overrides",
);

assert(
  normalizedSkillRecommendation.skill_recommendation.background_community_search === false,
  "Settings normalization should read background community search overrides",
);

assert(
  normalizedSkillRecommendation.skill_recommendation.community_preapprove_timeout_secs === 45,
  "Settings normalization should read the community preapprove timeout override",
);

assert(
  normalizedSkillRecommendation.skill_recommendation.suggest_global_enable_after_approvals === 5,
  "Settings normalization should read the global-enable suggestion threshold override",
);

assert(
  !("context_budget_tokens" in DEFAULT_AGENT_SETTINGS),
  "Default frontend settings should not expose removed context budget settings",
);

const normalizedLegacyBudget = normalizeAgentSettingsFromSource({
  context_budget_tokens: 222_000,
} as any);

assert(
  !("context_budget_tokens" in normalizedLegacyBudget),
  "Settings normalization should ignore legacy context budget settings",
);

const daemonConfigWithAudio = buildDaemonAgentConfig({
  ...DEFAULT_AGENT_SETTINGS,
  active_provider: "openai",
  openai: {
    ...DEFAULT_AGENT_SETTINGS.openai,
    api_key: "sk-active",
  },
  audio_stt_provider: "openai",
  audio_stt_model: "gpt-4o-transcribe",
  audio_stt_language: "pl",
  audio_tts_provider: "custom",
  audio_tts_model: "sonic-voice",
  audio_tts_voice: "alloy",
  custom: {
    ...DEFAULT_AGENT_SETTINGS.custom,
    base_url: "https://audio.example/v1",
    model: "fallback-audio-model",
    api_key: "sk-audio",
  },
});

assert(
  typeof daemonConfigWithAudio.audio === "object" && daemonConfigWithAudio.audio !== null,
  "Daemon config should include a canonical nested audio section",
);

assert(
  daemonConfigWithAudio.audio?.stt?.provider === "openai"
    && daemonConfigWithAudio.audio?.stt?.model === "gpt-4o-transcribe"
    && daemonConfigWithAudio.audio?.stt?.language === "pl",
  "Daemon config should persist nested STT settings",
);

assert(
  daemonConfigWithAudio.audio?.tts?.provider === "custom"
    && daemonConfigWithAudio.audio?.tts?.model === "sonic-voice"
    && daemonConfigWithAudio.audio?.tts?.voice === "alloy"
    && daemonConfigWithAudio.audio?.tts?.auto_speak === false,
  "Daemon config should persist nested TTS settings",
);

assert(
  daemonConfigWithAudio.providers?.openai?.base_url === DEFAULT_AGENT_SETTINGS.openai.base_url,
  "Daemon config should still include the active provider config",
);

assert(
  daemonConfigWithAudio.providers?.custom?.base_url === "https://audio.example/v1"
    && daemonConfigWithAudio.providers?.custom?.model === "fallback-audio-model",
  "Daemon config should include non-active audio provider configs needed by STT/TTS",
);
