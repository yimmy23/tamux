import { describe, expect, it } from "vitest";

import {
  getDefaultModelForProvider,
  getProviderDefinition,
  normalizeAgentProviderId,
  normalizeProviderConfig,
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

describe("frontend Anthropic provider catalog", () => {
  it("registers Anthropic with static defaults", () => {
    const anthropic = getProviderDefinition("anthropic" as any);

    expect(anthropic).toBeDefined();
    expect(anthropic?.defaultBaseUrl).toBe("https://api.anthropic.com");
    expect(anthropic?.defaultModel).toBe("claude-opus-4-7");
    expect(anthropic?.supportsModelFetch).toBe(false);
    expect(anthropic?.models.map((model) => [model.id, model.contextWindow])).toEqual([
      ["claude-opus-4-7", 1_000_000],
      ["claude-opus-4-6", 1_000_000],
      ["claude-opus-4-5-20251101", 200_000],
      ["claude-opus-4-1-20250805", 200_000],
      ["claude-opus-4-20250514", 200_000],
      ["claude-sonnet-4-6", 1_000_000],
      ["claude-sonnet-4-5-20250929", 200_000],
      ["claude-sonnet-4-20250514", 200_000],
      ["claude-3-7-sonnet-20250219", 200_000],
      ["claude-haiku-4-5-20251001", 200_000],
      ["claude-3-5-haiku-20241022", 200_000],
      ["claude-3-opus-20240229", 200_000],
      ["claude-3-haiku-20240307", 200_000],
    ]);
  });

  it("recognizes Anthropic as a valid provider id", () => {
    expect(normalizeAgentProviderId("anthropic")).toBe("anthropic");
    expect(getDefaultModelForProvider("anthropic" as any)).toBe("claude-opus-4-7");
  });
});

describe("frontend Xiaomi MiMo token plan provider catalog", () => {
  it("registers Xiaomi MiMo token plan with static defaults", () => {
    const mimo = getProviderDefinition("xiaomi-mimo-token-plan" as any);

    expect(mimo).toBeDefined();
    expect(mimo?.defaultBaseUrl).toBe("https://api.xiaomimimo.com/v1");
    expect(mimo?.defaultModel).toBe("mimo-v2-pro");
    expect(mimo?.supportsModelFetch).toBe(false);
    expect(mimo?.models.map((model) => [model.id, model.contextWindow])).toEqual([
      ["mimo-v2-pro", 1_000_000],
      ["mimo-v2-omni", 256_000],
    ]);
  });

  it("recognizes Xiaomi MiMo token plan as a valid provider id", () => {
    expect(normalizeAgentProviderId("xiaomi-mimo-token-plan")).toBe("xiaomi-mimo-token-plan");
    expect(getDefaultModelForProvider("xiaomi-mimo-token-plan" as any)).toBe("mimo-v2-pro");
  });
});

describe("frontend Nous Portal provider catalog", () => {
  it("registers Nous Portal with fetchable defaults", () => {
    const nous = getProviderDefinition("nous-portal" as any);

    expect(nous).toBeDefined();
    expect(nous?.defaultBaseUrl).toBe("https://inference-api.nousresearch.com/v1");
    expect(nous?.defaultModel).toBe("nousresearch/hermes-4-70b");
    expect(nous?.supportsModelFetch).toBe(true);
    expect(nous?.models.map((model) => [model.id, model.contextWindow])).toEqual([
      ["nousresearch/hermes-4-70b", 131_072],
      ["nousresearch/hermes-4-405b", 131_072],
      ["nousresearch/hermes-3-llama-3.1-70b", 131_072],
      ["nousresearch/hermes-3-llama-3.1-405b", 131_072],
    ]);
  });

  it("recognizes Nous Portal as a valid provider id", () => {
    expect(normalizeAgentProviderId("nous-portal")).toBe("nous-portal");
    expect(getDefaultModelForProvider("nous-portal" as any)).toBe("nousresearch/hermes-4-70b");
  });
});

describe("frontend Azure OpenAI provider catalog", () => {
  it("registers Azure OpenAI with OpenAI-compatible defaults", () => {
    const azure = getProviderDefinition("azure-openai" as any);

    expect(azure).toBeDefined();
    expect(azure?.defaultBaseUrl).toBe(
      "https://YOUR-RESOURCE-NAME.openai.azure.com/openai/v1",
    );
    expect(azure?.defaultModel).toBe("");
    expect(azure?.supportsModelFetch).toBe(true);
    expect(azure?.defaultTransport).toBe("responses");
    expect(azure?.supportedAuthSources).toEqual(["api_key"]);
    expect(azure?.supportsResponseContinuity).toBe(true);
  });

  it("recognizes Azure OpenAI as a valid provider id", () => {
    expect(normalizeAgentProviderId("azure-openai")).toBe("azure-openai");
    expect(getDefaultModelForProvider("azure-openai" as any)).toBe("");
  });

  it("preserves the configured resource-specific base URL", () => {
    const normalized = normalizeProviderConfig(
      "azure-openai" as any,
      {
        base_url: "https://YOUR-RESOURCE-NAME.openai.azure.com/openai/v1",
        model: "",
        custom_model_name: "",
        api_key: "",
        assistant_id: "",
        api_transport: "responses",
        auth_source: "api_key",
        context_window_tokens: null,
      },
      {
        base_url: "https://my-real-resource.openai.azure.com/openai/v1",
        model: "deployment-name",
      },
    );

    expect(normalized.base_url).toBe("https://my-real-resource.openai.azure.com/openai/v1");
    expect(normalized.model).toBe("deployment-name");
  });
});
