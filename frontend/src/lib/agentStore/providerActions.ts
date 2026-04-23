import { getBridge } from "../bridge";
import { isValidProviderAuthStates } from "./history";
import { hydrateProviderDefinitionsFromCatalog } from "./providers";
import type { AgentState, AgentStoreGet, AgentStoreSet } from "./storeTypes";
import type { ProviderAuthState, SubAgentDefinition } from "./types";

export function getSubAgentCapabilities(definition: SubAgentDefinition) {
  const isProtected = Boolean(definition.builtin || definition.immutable_identity);
  return {
    isProtected,
    canToggle: Boolean(definition.disable_allowed ?? true),
    canDelete: Boolean(definition.delete_allowed ?? true),
    protectedReason: definition.protected_reason ?? (isProtected ? "Protected built-in sub-agent" : ""),
  } as const;
}

export function sanitizeSubAgentUpdate(
  existing: SubAgentDefinition | undefined,
  definition: SubAgentDefinition,
): SubAgentDefinition {
  if (!existing || !getSubAgentCapabilities(existing).isProtected) {
    return definition;
  }

  return {
    ...definition,
    id: existing.id,
    name: existing.name,
    enabled: existing.enabled,
    builtin: existing.builtin,
    immutable_identity: existing.immutable_identity,
    disable_allowed: existing.disable_allowed,
    delete_allowed: existing.delete_allowed,
    protected_reason: existing.protected_reason,
  };
}

type ProviderActionKeys =
  | "refreshProviderAuthStates"
  | "validateProvider"
  | "loginProvider"
  | "logoutProvider"
  | "addSubAgent"
  | "removeSubAgent"
  | "updateSubAgent"
  | "refreshSubAgents";

export function createProviderActions(
  set: AgentStoreSet,
  get: AgentStoreGet,
): Pick<AgentState, ProviderActionKeys> {
  return {
    refreshProviderAuthStates: async () => {
      const bridge = getBridge();
      if (!bridge?.agentGetProviderAuthStates) {
        return;
      }
      try {
        if (bridge.agentGetProviderCatalog) {
          const catalog = await bridge.agentGetProviderCatalog();
          const { diagnostics } = hydrateProviderDefinitionsFromCatalog(catalog);
          if (diagnostics.length > 0) {
            console.warn("custom provider configuration diagnostics", diagnostics);
          }
        }
        const states = await bridge.agentGetProviderAuthStates();
        if (isValidProviderAuthStates(states)) {
          set({ providerAuthStates: states as ProviderAuthState[] });
        }
      } catch {
        // Ignore bridge failures and keep current UI state.
      }
    },
    validateProvider: async (providerId, base_url, api_key, auth_source) => {
      const bridge = getBridge();
      if (!bridge?.agentValidateProvider) {
        return { valid: false, error: "Bridge not available" };
      }
      try {
        return await bridge.agentValidateProvider(providerId, base_url, api_key, auth_source);
      } catch (error) {
        return { valid: false, error: String(error) };
      }
    },
    loginProvider: async (providerId, api_key, base_url) => {
      const bridge = getBridge();
      if (!bridge?.agentLoginProvider) {
        return;
      }
      try {
        const result = await bridge.agentLoginProvider(providerId, api_key, base_url);
        if (isValidProviderAuthStates(result)) {
          set({ providerAuthStates: result as ProviderAuthState[] });
        }
      } catch {
        // Ignore bridge failures and keep current UI state.
      }
    },
    logoutProvider: async (providerId) => {
      const bridge = getBridge();
      if (!bridge?.agentLogoutProvider) {
        return;
      }
      try {
        const result = await bridge.agentLogoutProvider(providerId);
        if (isValidProviderAuthStates(result)) {
          set({ providerAuthStates: result as ProviderAuthState[] });
        }
      } catch {
        // Ignore bridge failures and keep current UI state.
      }
    },
    addSubAgent: async (definition) => {
      const bridge = getBridge();
      if (!bridge?.agentSetSubAgent) {
        return;
      }
      const fullDefinition: SubAgentDefinition = {
        ...definition,
        id: `subagent_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`,
        created_at: Math.floor(Date.now() / 1000),
      };
      try {
        await bridge.agentSetSubAgent(JSON.stringify(fullDefinition));
        await get().refreshSubAgents();
      } catch {
        // Ignore bridge failures and keep current UI state.
      }
    },
    removeSubAgent: async (id) => {
      const bridge = getBridge();
      if (!bridge?.agentRemoveSubAgent) {
        return;
      }
      const existing = get().subAgents.find((entry) => entry.id === id);
      if (existing && !getSubAgentCapabilities(existing).canDelete) {
        return;
      }
      try {
        await bridge.agentRemoveSubAgent(id);
        await get().refreshSubAgents();
      } catch {
        // Ignore bridge failures and keep current UI state.
      }
    },
    updateSubAgent: async (definition) => {
      const bridge = getBridge();
      if (!bridge?.agentSetSubAgent) {
        return;
      }
      const existing = get().subAgents.find((entry) => entry.id === definition.id);
      const sanitized = sanitizeSubAgentUpdate(existing, definition);
      try {
        await bridge.agentSetSubAgent(JSON.stringify(sanitized));
        await get().refreshSubAgents();
      } catch {
        // Ignore bridge failures and keep current UI state.
      }
    },
    refreshSubAgents: async () => {
      const bridge = getBridge();
      if (!bridge?.agentListSubAgents) {
        return;
      }
      try {
        const list = await bridge.agentListSubAgents();
        if (Array.isArray(list)) {
          set({ subAgents: list as SubAgentDefinition[] });
        }
      } catch {
        // Ignore bridge failures and keep current UI state.
      }
    },
  };
}
