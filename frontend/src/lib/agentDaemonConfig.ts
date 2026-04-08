import type { AgentSettings } from "./agentStore/settings.ts";
import {
  getDefaultAuthSource,
  getEffectiveContextWindow,
  normalizeAuthSource,
} from "./agentStore/providers.ts";
import type { AgentProviderConfig } from "./agentStore/types.ts";
import { getBridge } from "./bridge.ts";

export type DaemonOwnedAuthCapability = {
  daemonOwnedAuthAvailable: boolean;
  chatgptSubscriptionAvailable: boolean;
};

export function getAgentBridge() {
  return getBridge();
}

export function resolveDaemonBackend(
  backend: AgentSettings["agent_backend"],
): Exclude<AgentSettings["agent_backend"], "legacy"> {
  return backend === "legacy" ? "daemon" : backend;
}

export function shouldUseDaemonRuntime(
  backend: AgentSettings["agent_backend"],
): boolean {
  if (backend === "openclaw" || backend === "hermes") {
    return true;
  }
  return Boolean(getAgentBridge()?.agentSendMessage);
}

export function getDaemonOwnedAuthCapability(
  backend: AgentSettings["agent_backend"],
  bridge: AmuxBridge | null = getAgentBridge(),
): DaemonOwnedAuthCapability {
  const daemonOwnedAuthAvailable = backend === "daemon"
    || (backend === "legacy" && Boolean(bridge?.agentSendMessage));

  return {
    daemonOwnedAuthAvailable,
    chatgptSubscriptionAvailable: daemonOwnedAuthAvailable,
  };
}

export function getProviderAuthSupportOptions(
  backend: AgentSettings["agent_backend"],
  bridge: AmuxBridge | null = getAgentBridge(),
): { daemonOwnedAuthAvailable: boolean } {
  return {
    daemonOwnedAuthAvailable: getDaemonOwnedAuthCapability(backend, bridge).daemonOwnedAuthAvailable,
  };
}

function escapeJsonPointerSegment(segment: string): string {
  return segment.replace(/~/g, "~0").replace(/\//g, "~1");
}

export function flattenDaemonConfigEntries(
  value: unknown,
  pointer = "",
  entries: Array<{ keyPath: string; value: unknown }> = [],
): Array<{ keyPath: string; value: unknown }> {
  if (value && typeof value === "object" && !Array.isArray(value)) {
    const record = value as Record<string, unknown>;
    const keys = Object.keys(record);
    if (keys.length > 0) {
      for (const key of keys) {
        flattenDaemonConfigEntries(
          record[key],
          `${pointer}/${escapeJsonPointerSegment(key)}`,
          entries,
        );
      }
      return entries;
    }
  }
  entries.push({ keyPath: pointer, value });
  return entries;
}

export function diffDaemonConfigEntries(
  previous: unknown,
  next: unknown,
): Array<{ keyPath: string; value: unknown }> {
  const previousMap = new Map(
    flattenDaemonConfigEntries(previous).map(({ keyPath, value }) => [
      keyPath,
      JSON.stringify(value),
    ]),
  );
  return flattenDaemonConfigEntries(next).filter(
    ({ keyPath, value }) => previousMap.get(keyPath) !== JSON.stringify(value),
  );
}

export function buildDaemonAgentConfig(
  agentSettings: AgentSettings,
) {
  const daemonBackend = resolveDaemonBackend(agentSettings.agent_backend);
  const providerKey = agentSettings.active_provider;
  const providerConfig = agentSettings[providerKey] as AgentProviderConfig | undefined;
  const authSupportOptions = getProviderAuthSupportOptions(agentSettings.agent_backend);
  const authSource = providerConfig
    ? normalizeAuthSource(providerKey, providerConfig.auth_source, authSupportOptions)
    : getDefaultAuthSource(providerKey, authSupportOptions);
  const providerConfigs = providerConfig
    ? {
      [providerKey]: {
        base_url: providerConfig.base_url || "",
        model: providerConfig.model || "",
        assistant_id: providerConfig.assistant_id || "",
        api_transport: providerConfig.api_transport || "chat_completions",
        auth_source: authSource,
        context_window_tokens: getEffectiveContextWindow(providerKey, providerConfig),
        reasoning_effort: agentSettings.reasoning_effort || "high",
      },
    }
    : {};

  return {
    enabled: agentSettings.enabled,
    agent_backend: daemonBackend,
    provider: providerKey,
    base_url: providerConfig?.base_url || "",
    model: providerConfig?.model || "",
    assistant_id: providerConfig?.assistant_id || "",
    api_transport: providerConfig?.api_transport || "chat_completions",
    auth_source: authSource,
    reasoning_effort: agentSettings.reasoning_effort || "high",
    system_prompt: agentSettings.system_prompt,
    auto_compact_context: agentSettings.auto_compact_context,
    max_context_messages: agentSettings.max_context_messages,
    max_tool_loops: agentSettings.max_tool_loops,
    max_retries: agentSettings.max_retries,
    retry_delay_ms: agentSettings.retry_delay_ms,
    message_loop_delay_ms: agentSettings.message_loop_delay_ms,
    tool_call_delay_ms: agentSettings.tool_call_delay_ms,
    llm_stream_chunk_timeout_secs: agentSettings.llm_stream_chunk_timeout_secs,
    auto_retry: agentSettings.auto_retry,
    context_window_tokens: providerConfig
      ? getEffectiveContextWindow(providerKey, providerConfig)
      : 128000,
    context_budget_tokens: agentSettings.context_budget_tokens,
    compact_threshold_pct: agentSettings.compact_threshold_pct,
    keep_recent_on_compact: agentSettings.keep_recent_on_compact,
    builtin_sub_agents: {
      weles: {
        max_concurrent_reviews: agentSettings.weles_max_concurrent_reviews,
      },
    },
    compaction: {
      strategy: agentSettings.compaction.strategy,
      weles: {
        provider: agentSettings.compaction.weles.provider,
        model: agentSettings.compaction.weles.model,
        reasoning_effort: agentSettings.compaction.weles.reasoning_effort,
      },
      custom_model: {
        provider: agentSettings.compaction.custom_model.provider,
        base_url: agentSettings.compaction.custom_model.base_url,
        model: agentSettings.compaction.custom_model.model,
        api_key: agentSettings.compaction.custom_model.api_key,
        assistant_id: agentSettings.compaction.custom_model.assistant_id,
        auth_source: agentSettings.compaction.custom_model.auth_source,
        api_transport: agentSettings.compaction.custom_model.api_transport,
        reasoning_effort: agentSettings.compaction.custom_model.reasoning_effort,
        context_window_tokens: agentSettings.compaction.custom_model.context_window_tokens,
      },
    },
    skill_recommendation: {
      enabled: agentSettings.skill_recommendation.enabled,
      background_community_search: agentSettings.skill_recommendation.background_community_search,
      community_preapprove_timeout_secs:
        agentSettings.skill_recommendation.community_preapprove_timeout_secs,
      suggest_global_enable_after_approvals:
        agentSettings.skill_recommendation.suggest_global_enable_after_approvals,
    },
    enable_honcho_memory: agentSettings.enable_honcho_memory,
    honcho_api_key: agentSettings.honcho_api_key,
    honcho_base_url: agentSettings.honcho_base_url,
    honcho_workspace_id: agentSettings.honcho_workspace_id,
    providers: providerConfigs,
    tools: {
      bash: agentSettings.enable_bash_tool,
      web_search: agentSettings.enable_web_search_tool,
      web_browse: agentSettings.enable_web_browsing_tool,
      vision: agentSettings.enable_vision_tool,
      gateway_messaging: true,
      file_operations: true,
      system_info: true,
    },
    gateway: {
      enabled: agentSettings.gateway_enabled,
      slack_token: agentSettings.slack_token,
      slack_channel_filter: agentSettings.slack_channel_filter,
      telegram_token: agentSettings.telegram_token,
      telegram_allowed_chats: agentSettings.telegram_allowed_chats,
      discord_token: agentSettings.discord_token,
      discord_channel_filter: agentSettings.discord_channel_filter,
      discord_allowed_users: agentSettings.discord_allowed_users,
      whatsapp_token: agentSettings.whatsapp_token,
      whatsapp_phone_id: agentSettings.whatsapp_phone_id,
      whatsapp_allowed_contacts: agentSettings.whatsapp_allowed_contacts,
      command_prefix: agentSettings.gateway_command_prefix || "!tamux",
    },
    anticipatory: {
      enabled: agentSettings.anticipatory_enabled,
      morning_brief: agentSettings.anticipatory_morning_brief,
      predictive_hydration: agentSettings.anticipatory_predictive_hydration,
      stuck_detection: agentSettings.anticipatory_stuck_detection,
    },
    operator_model: {
      enabled: agentSettings.operator_model_enabled,
      allow_message_statistics: agentSettings.operator_model_allow_message_statistics,
      allow_approval_learning: agentSettings.operator_model_allow_approval_learning,
      allow_attention_tracking: agentSettings.operator_model_allow_attention_tracking,
      allow_implicit_feedback: agentSettings.operator_model_allow_implicit_feedback,
    },
    collaboration: {
      enabled: agentSettings.collaboration_enabled,
    },
    compliance: {
      mode: agentSettings.compliance_mode,
      retention_days: agentSettings.compliance_retention_days,
      sign_all_events: agentSettings.compliance_sign_all_events,
    },
    tool_synthesis: {
      enabled: agentSettings.tool_synthesis_enabled,
      require_activation: agentSettings.tool_synthesis_require_activation,
      max_generated_tools: agentSettings.tool_synthesis_max_generated_tools,
    },
  };
}
