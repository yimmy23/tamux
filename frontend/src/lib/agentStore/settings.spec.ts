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
