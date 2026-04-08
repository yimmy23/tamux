import {
  buildDaemonAgentConfig,
  getDaemonOwnedAuthCapability,
} from "./agentDaemonConfig.ts";
import { DEFAULT_AGENT_SETTINGS } from "./agentStore/settings.ts";
import {
  getDefaultModelForProvider,
  getDefaultAuthSource,
  getEffectiveContextWindow,
  getProviderDefinition,
  getSupportedAuthSources,
  normalizeAgentProviderId,
  normalizeAuthSource,
} from "./agentStore/providers.ts";

function assert(condition: unknown, message: string): void {
  if (!condition) {
    throw new Error(message);
  }
}

const daemonBridge = {
  agentSendMessage: async () => ({ ok: true }),
};

assert(
  getDaemonOwnedAuthCapability("daemon", daemonBridge).chatgptSubscriptionAvailable,
  "Daemon backend should expose daemon-owned ChatGPT auth",
);

assert(
  getDaemonOwnedAuthCapability("legacy", daemonBridge).chatgptSubscriptionAvailable,
  "Legacy backend should expose daemon-owned ChatGPT auth when the daemon bridge is present",
);

assert(
  !getDaemonOwnedAuthCapability("legacy", null).chatgptSubscriptionAvailable,
  "Legacy backend should not expose daemon-owned ChatGPT auth without the daemon bridge",
);

assert(
  !getDaemonOwnedAuthCapability("openclaw", daemonBridge).chatgptSubscriptionAvailable,
  "OpenClaw should not expose daemon-owned ChatGPT auth",
);

assert(
  !getDaemonOwnedAuthCapability("hermes", daemonBridge).chatgptSubscriptionAvailable,
  "Hermes should not expose daemon-owned ChatGPT auth",
);

const unsupportedOpenAiSources = getSupportedAuthSources("openai", {
  daemonOwnedAuthAvailable: false,
});

assert(
  !unsupportedOpenAiSources.includes("chatgpt_subscription"),
  "OpenAI should hide ChatGPT subscription auth when daemon-owned auth is unavailable",
);

assert(
  getDefaultAuthSource("openai", { daemonOwnedAuthAvailable: false }) === "api_key",
  "OpenAI should fall back to API key auth when daemon-owned auth is unavailable",
);

assert(
  normalizeAuthSource("openai", "chatgpt_subscription", {
    daemonOwnedAuthAvailable: false,
  }) === "api_key",
  "ChatGPT subscription auth should normalize to API key when daemon-owned auth is unavailable",
);

const unsupportedBackendConfig = buildDaemonAgentConfig({
  ...DEFAULT_AGENT_SETTINGS,
  agent_backend: "openclaw",
  active_provider: "openai",
  openai: {
    ...DEFAULT_AGENT_SETTINGS.openai,
    auth_source: "chatgpt_subscription",
  },
});

assert(
  unsupportedBackendConfig.auth_source === "api_key",
  "Daemon config should not emit ChatGPT subscription auth for non-daemon-backed execution",
);

const configuredDelaySettings = {
  ...DEFAULT_AGENT_SETTINGS,
  message_loop_delay_ms: 250,
  tool_call_delay_ms: 750,
  llm_stream_chunk_timeout_secs: 420,
  weles_max_concurrent_reviews: 4,
};

const configuredDelayDaemonConfig = buildDaemonAgentConfig(configuredDelaySettings);

assert(
  configuredDelayDaemonConfig.message_loop_delay_ms === 250,
  "Daemon config should forward message loop delay settings",
);

assert(
  configuredDelayDaemonConfig.tool_call_delay_ms === 750,
  "Daemon config should forward tool call delay settings",
);

assert(
  configuredDelayDaemonConfig.llm_stream_chunk_timeout_secs === 420,
  "Daemon config should forward LLM stream chunk timeout settings",
);

assert(
  configuredDelayDaemonConfig.builtin_sub_agents?.weles?.max_concurrent_reviews === 4,
  "Daemon config should forward WELES review concurrency settings",
);

assert(
  buildDaemonAgentConfig(DEFAULT_AGENT_SETTINGS).skill_recommendation?.enabled === true,
  "Daemon config should serialize skill recommendation enablement",
);

assert(
  buildDaemonAgentConfig(DEFAULT_AGENT_SETTINGS).skill_recommendation?.community_preapprove_timeout_secs === 30,
  "Daemon config should serialize the skill recommendation preapprove timeout",
);

assert(
  buildDaemonAgentConfig(DEFAULT_AGENT_SETTINGS).skill_recommendation?.background_community_search === true,
  "Daemon config should serialize background community skill discovery",
);

assert(
  buildDaemonAgentConfig(DEFAULT_AGENT_SETTINGS).skill_recommendation?.suggest_global_enable_after_approvals === 3,
  "Daemon config should serialize the global-enable suggestion threshold",
);

const customModelProviderConfig = {
  ...DEFAULT_AGENT_SETTINGS.openrouter,
  model: "openrouter/custom-preview",
  custom_model_name: "Custom Preview",
  context_window_tokens: 333_000,
};

assert(
  getEffectiveContextWindow("openrouter", customModelProviderConfig) === 333_000,
  "Custom model entries should honor a manual context window override on non-custom providers",
);

const customModelDaemonConfig = buildDaemonAgentConfig({
  ...DEFAULT_AGENT_SETTINGS,
  active_provider: "openrouter",
  openrouter: customModelProviderConfig,
});

assert(
  customModelDaemonConfig.context_window_tokens === 333_000,
  "Daemon config should forward the custom-model context window override",
);

assert(
  customModelDaemonConfig.providers?.openrouter?.context_window_tokens === 333_000,
  "Provider config should preserve the custom-model context window override",
);

const namedKnownModelConfig = {
  ...DEFAULT_AGENT_SETTINGS["github-copilot"],
  model: "totally-custom-runtime-id",
  custom_model_name: "Raptor mini (Preview)",
  context_window_tokens: null,
};

assert(
  getEffectiveContextWindow("github-copilot", namedKnownModelConfig) === 264_000,
  "Custom model names should resolve against the selected provider catalog when computing context length",
);

const namedUnknownModelConfig = {
  ...DEFAULT_AGENT_SETTINGS["github-copilot"],
  model: "totally-custom-runtime-id",
  custom_model_name: "Definitely Unknown Model",
  context_window_tokens: null,
};

assert(
  getEffectiveContextWindow("github-copilot", namedUnknownModelConfig) === 264_000,
  "Unknown custom models should default to a 264k context window",
);

assert(
  normalizeAgentProviderId("arcee") === "arcee",
  "Arcee should be recognized as a known provider id",
);

const arceeProvider = getProviderDefinition("arcee");

assert(
  arceeProvider?.defaultBaseUrl === "https://api.arcee.ai/api/v1",
  "Arcee should use the OpenAI-compatible Arcee API base URL",
);

assert(
  arceeProvider?.supportsModelFetch === true,
  "Arcee should support remote model fetching via the /models endpoint",
);

assert(
  getDefaultModelForProvider("arcee") === "trinity-large-thinking",
  "Arcee should default to trinity-large-thinking",
);

assert(
  getDefaultAuthSource("arcee") === "api_key",
  "Arcee should default to daemon-owned API key auth",
);

assert(
  getSupportedAuthSources("arcee").length === 1 && getSupportedAuthSources("arcee")[0] === "api_key",
  "Arcee should only expose API key auth",
);

assert(
  DEFAULT_AGENT_SETTINGS.arcee.base_url === "https://api.arcee.ai/api/v1",
  "Local fallback settings should include the Arcee base URL",
);

assert(
  DEFAULT_AGENT_SETTINGS.arcee.model === "trinity-large-thinking",
  "Local fallback settings should include the Arcee default model",
);

assert(
  DEFAULT_AGENT_SETTINGS.arcee.context_window_tokens === 256_000,
  "Local fallback settings should include the Arcee 256k context window",
);

assert(
  getEffectiveContextWindow("arcee", DEFAULT_AGENT_SETTINGS.arcee) === 256_000,
  "Arcee should resolve its default model to a 256k context window",
);
