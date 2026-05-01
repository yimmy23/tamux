import { expect, test } from "vitest";
import { buildModelFetchKey, shouldFetchRemoteModels } from "./modelSelectorFetch";

test("model fetch key changes with provider and output modality", () => {
  expect(buildModelFetchKey({
    providerId: "openrouter",
    baseUrl: "https://openrouter.ai/api/v1",
    apiKey: "",
    outputModalities: "embedding",
  })).not.toBe(buildModelFetchKey({
    providerId: "openrouter",
    baseUrl: "https://openrouter.ai/api/v1",
    apiKey: "",
    outputModalities: "audio",
  }));
});

test("model fetch key trims optional output modality", () => {
  expect(buildModelFetchKey({
    providerId: "openrouter",
    baseUrl: "https://openrouter.ai/api/v1",
    apiKey: "key",
    outputModalities: " embedding ",
  })).toBe("openrouter\nhttps://openrouter.ai/api/v1\nkey\nembedding");
});

test("remote model fetch follows tui auth-source gate", () => {
  expect(shouldFetchRemoteModels({
    supportsModelFetch: true,
    providerId: "openrouter",
    authSource: "api_key",
  })).toBe(true);
  expect(shouldFetchRemoteModels({
    supportsModelFetch: true,
    providerId: "openai",
    authSource: "chatgpt_subscription",
  })).toBe(false);
  expect(shouldFetchRemoteModels({
    supportsModelFetch: false,
    providerId: "custom",
    authSource: "api_key",
  })).toBe(false);
});
