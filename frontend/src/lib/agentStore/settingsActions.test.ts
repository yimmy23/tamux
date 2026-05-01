import { afterEach, describe, expect, it, vi } from "vitest";

import { DEFAULT_CONCIERGE_CONFIG } from "./settingsActions";
import { useAgentStore } from "./store";

describe("concierge settings actions", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    useAgentStore.setState({ conciergeConfig: DEFAULT_CONCIERGE_CONFIG });
  });

  it("hydrates Rarog provider and model from daemon concierge config", async () => {
    vi.stubGlobal("window", {
      zorai: {
        agentGetConciergeConfig: vi.fn().mockResolvedValue({
          provider: "custom-rarog",
          model: "rarog-model",
        }),
      },
    });

    await useAgentStore.getState().refreshConciergeConfig();

    expect(useAgentStore.getState().conciergeConfig).toMatchObject({
      enabled: true,
      detail_level: "proactive_triage",
      provider: "custom-rarog",
      model: "rarog-model",
    });
  });

  it("clears Rarog provider and model when daemon config inherits from Svarog", async () => {
    useAgentStore.setState({
      conciergeConfig: {
        ...DEFAULT_CONCIERGE_CONFIG,
        provider: "custom-rarog",
        model: "rarog-model",
      },
    });
    vi.stubGlobal("window", {
      zorai: {
        agentGetConciergeConfig: vi.fn().mockResolvedValue({
          enabled: true,
          detail_level: "proactive_triage",
        }),
      },
    });

    await useAgentStore.getState().refreshConciergeConfig();

    expect(useAgentStore.getState().conciergeConfig.provider).toBeUndefined();
    expect(useAgentStore.getState().conciergeConfig.model).toBeUndefined();
  });
});
