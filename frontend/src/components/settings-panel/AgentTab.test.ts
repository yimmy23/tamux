import { expect, test } from "vitest";
import {
  filterAudioProviderOptions,
  normalizeAudioModelForProviderChange,
  normalizeLlmStreamTimeoutInput,
} from "./agentTabHelpers";

test("normalizeLlmStreamTimeoutInput clamps and coerces input", () => {
  expect(normalizeLlmStreamTimeoutInput("30.5")).toBe(30);
  expect(normalizeLlmStreamTimeoutInput("9999")).toBe(1800);
  expect(normalizeLlmStreamTimeoutInput("29")).toBe(30);
});

test("normalizeAudioModelForProviderChange resets stale xAI audio models", () => {
  expect(normalizeAudioModelForProviderChange("xai", "stt", "whisper-1")).toBe("grok-4");
  expect(normalizeAudioModelForProviderChange("xai", "tts", "gpt-4o-mini-tts")).toBe("grok-4");
  expect(normalizeAudioModelForProviderChange("xai", "stt", "grok-4")).toBe("grok-4");
});

test("normalizeAudioModelForProviderChange uses Xiaomi TTS defaults and keeps STT empty", () => {
  expect(normalizeAudioModelForProviderChange("xiaomi-mimo-token-plan", "tts", "")).toBe("mimo-v2.5-tts");
  expect(normalizeAudioModelForProviderChange("xiaomi-mimo-token-plan", "tts", "mimo-v2.5-tts-voiceclone")).toBe("mimo-v2.5-tts-voiceclone");
  expect(normalizeAudioModelForProviderChange("xiaomi-mimo-token-plan", "stt", "whisper-1")).toBe("");
});

test("normalizeAudioModelForProviderChange avoids non-audio chat defaults for dynamic providers", () => {
  expect(normalizeAudioModelForProviderChange("openrouter", "tts", "grok-4")).toBe("");
  expect(normalizeAudioModelForProviderChange("groq", "stt", "whisper-1")).toBe("");
  expect(normalizeAudioModelForProviderChange("custom", "tts", "sonic-voice")).toBe("sonic-voice");
});

test("filterAudioProviderOptions keeps only supported audio providers", () => {
  expect(filterAudioProviderOptions([
    { id: "openai", label: "OpenAI / ChatGPT" },
    { id: "xai", label: "xAI" },
    { id: "openrouter", label: "OpenRouter" },
    { id: "anthropic", label: "Anthropic" },
    { id: "together", label: "Together" },
    { id: "custom", label: "Custom" },
  ], "stt")).toEqual([
    { id: "openai", label: "OpenAI / ChatGPT" },
    { id: "xai", label: "xAI" },
    { id: "openrouter", label: "OpenRouter" },
    { id: "custom", label: "Custom" },
  ]);
  expect(filterAudioProviderOptions([
    { id: "openai", label: "OpenAI / ChatGPT" },
    { id: "xiaomi-mimo-token-plan", label: "Xiaomi MiMo Token Plan" },
    { id: "anthropic", label: "Anthropic" },
  ], "tts")).toEqual([
    { id: "openai", label: "OpenAI / ChatGPT" },
    { id: "xiaomi-mimo-token-plan", label: "Xiaomi MiMo Token Plan" },
  ]);
});
