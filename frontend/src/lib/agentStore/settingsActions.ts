import { getBridge } from "../bridge";
import { resolveReactChatHistoryMessageLimit } from "../chatHistoryPageSize";
import { normalizeAgentProviderId } from "./providers";
import { normalizeDaemonBackedAgentMode } from "./daemonBackedSettings";
import {
  buildHydratedRemoteThread,
  type RemoteAgentThreadRecord,
} from "./history";
import {
  DEFAULT_AGENT_SETTINGS,
  type DiskAgentSettings,
  looksLikeDaemonAgentConfig,
  normalizeAgentSettingsFromSource,
} from "./settings";
import type { AgentState, AgentStoreGet, AgentStoreSet, ConciergeConfig } from "./storeTypes";
import { getDaemonAgentConfig } from "../daemonConfig";

type SettingsActionKeys =
  | "updateAgentSetting"
  | "resetAgentSettings"
  | "refreshAgentSettingsFromDaemon"
  | "markAgentSettingsSynced";

type ConciergeActionKeys =
  | "refreshConciergeConfig"
  | "updateConciergeConfig"
  | "dismissConciergeWelcome";

type GatewayActionKeys = "setGatewayStatus";

export const DEFAULT_CONCIERGE_CONFIG: ConciergeConfig = {
  enabled: true,
  detail_level: "proactive_triage",
  reasoning_effort: undefined,
  auto_cleanup_on_navigate: true,
};

function isValidConciergeConfig(value: unknown): value is AgentState["conciergeConfig"] {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return false;
  }
  const record = value as Record<string, unknown>;
  return (
    typeof record.enabled === "boolean"
    || typeof record.detail_level === "string"
    || typeof record.reasoning_effort === "string"
    || typeof record.auto_cleanup_on_navigate === "boolean"
  );
}

export function createSettingsActions(
  set: AgentStoreSet,
  _get: AgentStoreGet,
): Pick<AgentState, SettingsActionKeys> {
  return {
    updateAgentSetting: (key, value) => {
      set((state) => {
        const nextValue = key === "active_provider" ? normalizeAgentProviderId(value) : value;
        const nextSettings = { ...state.agentSettings, [key]: nextValue };
        const activeProvider = nextSettings.active_provider;
        const activeProviderConfig = nextSettings[activeProvider];
        return {
          agentSettings: {
            ...nextSettings,
            agent_backend: normalizeDaemonBackedAgentMode(
              nextSettings.agent_backend,
              activeProvider,
              activeProviderConfig.auth_source,
            ) as AgentState["agentSettings"]["agent_backend"],
          },
          agentSettingsDirty: state.agentSettingsHydrated,
        };
      });
    },
    resetAgentSettings: () => {
      const defaults = { ...DEFAULT_AGENT_SETTINGS };
      set((state) => ({
        agentSettings: defaults,
        agentSettingsDirty: state.agentSettingsHydrated,
      }));
    },
    refreshAgentSettingsFromDaemon: async () => {
      const bridge = getBridge();
      if (!bridge?.agentGetConfig) {
        set({ agentSettingsHydrated: true, agentSettingsDirty: false });
        return true;
      }
      try {
        const daemonState = await getDaemonAgentConfig();
        if (!looksLikeDaemonAgentConfig(daemonState)) {
          set({ agentSettingsHydrated: false });
          return false;
        }
        const merged = normalizeAgentSettingsFromSource(daemonState as DiskAgentSettings);
        set({
          agentSettings: merged,
          agentSettingsHydrated: true,
          agentSettingsDirty: false,
        });
        return true;
      } catch {
        set({ agentSettingsHydrated: false });
        return false;
      }
    },
    markAgentSettingsSynced: () => set({ agentSettingsDirty: false }),
  };
}

export function createConciergeActions(
  set: AgentStoreSet,
  get: AgentStoreGet,
): Pick<AgentState, ConciergeActionKeys> {
  return {
    refreshConciergeConfig: async () => {
      const bridge = getBridge();
      if (!bridge?.agentGetConciergeConfig) {
        return;
      }
      try {
        const config = await bridge.agentGetConciergeConfig();
        if (isValidConciergeConfig(config)) {
          set({ conciergeConfig: config });
        }
      } catch {
        // Ignore bridge failures and keep current UI state.
      }
    },
    updateConciergeConfig: async (config) => {
      const bridge = getBridge();
      if (!bridge?.agentSetConciergeConfig) {
        return;
      }
      try {
        await bridge.agentSetConciergeConfig(config);
        if (bridge.agentGetConciergeConfig) {
          const refreshed = await bridge.agentGetConciergeConfig();
          if (isValidConciergeConfig(refreshed)) {
            set({ conciergeConfig: refreshed });
            return;
          }
        }
        if (isValidConciergeConfig(config)) {
          set({ conciergeConfig: config });
        }
      } catch {
        // Ignore bridge failures and keep current UI state.
      }
    },
    dismissConciergeWelcome: async () => {
      const bridge = getBridge();
      if (!bridge?.agentDismissConciergeWelcome) {
        return;
      }
      try {
        await bridge.agentDismissConciergeWelcome();
        set({ conciergeWelcome: null });
        if (!bridge.agentGetThread) {
          return;
        }
        const remoteThread = await bridge.agentGetThread("concierge", {
          messageLimit: resolveReactChatHistoryMessageLimit(
            get().agentSettings.react_chat_history_page_size,
          ) ?? null,
        }).catch(() => null);
        const hydrated = buildHydratedRemoteThread(
          (remoteThread ?? {}) as RemoteAgentThreadRecord,
          get().agentSettings.agent_name,
        );
        if (!hydrated) {
          return;
        }
        set((state) => {
          const existing = state.threads.find((thread) => thread.daemonThreadId === "concierge");
          if (!existing) {
            return state;
          }
          return {
            threads: state.threads.map((thread) =>
              thread.id === existing.id ? { ...hydrated.thread, id: existing.id } : thread),
            messages: {
              ...state.messages,
              [existing.id]: hydrated.messages.map((message) => ({
                ...message,
                threadId: existing.id,
              })),
            },
          };
        });
      } catch {
        // Ignore bridge failures and keep current UI state.
      }
    },
  };
}

export function createGatewayActions(set: AgentStoreSet): Pick<AgentState, GatewayActionKeys> {
  return {
    setGatewayStatus: (platform, status, lastError, consecutiveFailures) => {
      set((state) => ({
        gatewayStatuses: {
          ...state.gatewayStatuses,
          [platform]: {
            status,
            lastError,
            consecutiveFailures,
            updatedAt: Date.now(),
          },
        },
      }));
    },
  };
}
