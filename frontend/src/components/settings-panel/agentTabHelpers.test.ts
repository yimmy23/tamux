import { expect, test } from "vitest";
import {
  audioModelOptions,
  filterImageGenerationProviderOptions,
  imageGenerationModelOptions,
  normalizeImageGenerationModelForProviderChange,
  normalizeAudioModelForProviderChange,
} from "./agentTabHelpers";

test("image generation provider changes normalize unsupported models", () => {
  expect(normalizeImageGenerationModelForProviderChange("openai", "whisper-1")).toBe("gpt-image-1");
  expect(normalizeImageGenerationModelForProviderChange("openrouter", "")).toBe("openai/gpt-image-1");
  expect(normalizeImageGenerationModelForProviderChange("openai", "gpt-image-2")).toBe("gpt-image-2");
  expect(normalizeImageGenerationModelForProviderChange("openrouter", "openai/gpt-image-2")).toBe("openai/gpt-image-2");
});

test("image generation model options keep only image-capable provider models", () => {
  expect(imageGenerationModelOptions("openai").map((model) => model.id)).toContain("gpt-image-1");
  expect(imageGenerationModelOptions("openai").map((model) => model.id)).toContain("gpt-image-2");
  expect(imageGenerationModelOptions("openrouter").map((model) => model.id)).toContain("openai/gpt-image-1");
  expect(imageGenerationModelOptions("openrouter").map((model) => model.id)).toContain("openai/gpt-image-2");
  expect(imageGenerationModelOptions("minimax").map((model) => model.id)).toContain("image-01");
  expect(imageGenerationModelOptions("minimax-coding-plan").map((model) => model.id)).toContain("image-01");
  expect(imageGenerationModelOptions("featherless").length).toBe(0);
});

test("image generation provider options keep only providers with image-capable models", () => {
  expect(
    filterImageGenerationProviderOptions([
      { id: "featherless", label: "Featherless" },
      { id: "openai", label: "OpenAI" },
      { id: "openrouter", label: "OpenRouter" },
      { id: "minimax", label: "MiniMax" },
      { id: "minimax-coding-plan", label: "MiniMax Coding Plan" },
      { id: "custom", label: "Custom" },
    ]).map((provider) => provider.id),
  ).toEqual(["openai", "openrouter", "minimax", "minimax-coding-plan", "custom"]);
});

test("minimax audio model options expose tts only catalogs", () => {
  expect(audioModelOptions("minimax", "tts")?.map((model) => model.id)).toContain("speech-2.8-hd");
  expect(audioModelOptions("minimax-coding-plan", "tts")?.map((model) => model.id)).toContain("speech-2.8-turbo");
  expect(audioModelOptions("minimax", "stt")).toBeUndefined();
});

test("minimax provider changes normalize tts defaults and image defaults", () => {
  expect(normalizeAudioModelForProviderChange("minimax", "tts", "")).toBe("speech-2.8-hd");
  expect(normalizeAudioModelForProviderChange("minimax-coding-plan", "tts", "speech-2.6-turbo")).toBe("speech-2.6-turbo");
  expect(normalizeAudioModelForProviderChange("minimax", "stt", "whisper-1")).toBe("");
  expect(normalizeImageGenerationModelForProviderChange("minimax", "")).toBe("image-01");
  expect(normalizeImageGenerationModelForProviderChange("minimax-coding-plan", "image-01")).toBe("image-01");
});
