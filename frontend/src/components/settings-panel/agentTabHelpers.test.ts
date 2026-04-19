import { expect, test } from "vitest";
import {
  filterImageGenerationProviderOptions,
  imageGenerationModelOptions,
  normalizeImageGenerationModelForProviderChange,
} from "./agentTabHelpers";

test("image generation provider changes normalize unsupported models", () => {
  expect(normalizeImageGenerationModelForProviderChange("openai", "whisper-1")).toBe("gpt-image-1");
  expect(normalizeImageGenerationModelForProviderChange("openrouter", "")).toBe("openai/gpt-image-1");
});

test("image generation model options keep only image-capable provider models", () => {
  expect(imageGenerationModelOptions("openai").map((model) => model.id)).toContain("gpt-image-1");
  expect(imageGenerationModelOptions("openrouter").map((model) => model.id)).toContain("openai/gpt-image-1");
  expect(imageGenerationModelOptions("featherless").length).toBe(0);
});

test("image generation provider options keep only providers with image-capable models", () => {
  expect(
    filterImageGenerationProviderOptions([
      { id: "featherless", label: "Featherless" },
      { id: "openai", label: "OpenAI" },
      { id: "openrouter", label: "OpenRouter" },
      { id: "custom", label: "Custom" },
    ]).map((provider) => provider.id),
  ).toEqual(["openai", "openrouter", "custom"]);
});
