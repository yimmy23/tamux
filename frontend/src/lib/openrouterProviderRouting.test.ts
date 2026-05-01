import { describe, expect, it } from "vitest";
import {
  buildOpenRouterEndpointsUrl,
  normalizeOpenRouterProviderSlugs,
  parseOpenRouterEndpointProviders,
} from "./openrouterProviderRouting";

describe("openrouterProviderRouting", () => {
  it("builds the per-model endpoints URL from the OpenRouter API base URL", () => {
    expect(buildOpenRouterEndpointsUrl(
      "https://openrouter.ai/api/v1",
      "deepseek/deepseek-r1",
    )).toBe("https://openrouter.ai/api/v1/models/deepseek/deepseek-r1/endpoints");
  });

  it("parses endpoint provider routing slugs from OpenRouter endpoint data", () => {
    const providers = parseOpenRouterEndpointProviders({
      data: {
        endpoints: [
          { provider_name: "Novita", tag: "novita/fp8" },
          { provider_name: "Azure", tag: "azure" },
          { provider_name: "Novita duplicate", tag: "novita/fp8" },
        ],
      },
    });

    expect(providers).toEqual([
      { name: "Novita", slug: "novita/fp8" },
      { name: "Azure", slug: "azure" },
    ]);
  });

  it("normalizes stored provider slug arrays", () => {
    expect(normalizeOpenRouterProviderSlugs([" azure ", "", "azure", "novita/fp8"])).toEqual([
      "azure",
      "novita/fp8",
    ]);
  });
});
