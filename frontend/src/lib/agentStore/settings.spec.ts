import {
  DEFAULT_AGENT_SETTINGS,
  normalizeAgentSettingsFromSource,
} from "./settings.ts";

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

const normalizedTimeout = normalizeAgentSettingsFromSource({
  llm_stream_chunk_timeout_secs: 420,
});

assert(
  normalizedTimeout.llm_stream_chunk_timeout_secs === 420,
  "Settings normalization should preserve LLM stream chunk timeout overrides",
);
