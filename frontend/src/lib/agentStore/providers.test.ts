import { describe, expect, it } from "vitest";

import {
  getDefaultModelForProvider,
  getProviderDefinition,
  normalizeAgentProviderId,
} from "./providers.ts";

describe("frontend NVIDIA provider catalog", () => {
  it("registers NVIDIA with hosted defaults and fetch support", () => {
    const nvidia = getProviderDefinition("nvidia");

    expect(nvidia).toBeDefined();
    expect(nvidia?.defaultBaseUrl).toBe("https://integrate.api.nvidia.com/v1");
    expect(nvidia?.defaultModel).toBe("minimaxai/minimax-m2.7");
    expect(nvidia?.supportsModelFetch).toBe(true);
  });

  it("recognizes NVIDIA as a valid provider id", () => {
    expect(normalizeAgentProviderId("nvidia")).toBe("nvidia");
    expect(getDefaultModelForProvider("nvidia")).toBe("minimaxai/minimax-m2.7");
  });
});