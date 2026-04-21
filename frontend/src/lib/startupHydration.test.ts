import { beforeEach, describe, expect, it, vi } from "vitest";
import { hydrateAgentStore } from "./agentStore/hydrate";
import { useAgentStore } from "./agentStore/store";
import { loadAgentSettings } from "./agentStore/settings";
import { hydrateTierStore, useTierStore } from "./tierStore";

describe("startup daemon config hydration", () => {
  beforeEach(() => {
    useAgentStore.setState({
      agentSettings: loadAgentSettings(),
      agentSettingsHydrated: false,
      agentSettingsDirty: false,
      threads: [],
      messages: {},
      todos: {},
      activeThreadId: null,
    });
    useTierStore.getState().setTier("newcomer");
    vi.restoreAllMocks();
    Reflect.deleteProperty(globalThis, "window");
  });

  it("dedupes concurrent full-config fetches across agent and tier hydration", async () => {
    const agentGetConfig = vi.fn(async () => ({
      provider: "openai",
      model: "gpt-5.4",
      agent_backend: "daemon",
      tier: {
        user_override: "expert",
      },
    }));

    Object.assign(globalThis, {
      window: {
        tamux: {
          agentGetConfig,
        },
      },
    });

    await Promise.all([hydrateAgentStore(), hydrateTierStore()]);

    expect(agentGetConfig).toHaveBeenCalledTimes(1);
    expect(useAgentStore.getState().agentSettingsHydrated).toBe(true);
    expect(useTierStore.getState().currentTier).toBe("expert");
  });
});
