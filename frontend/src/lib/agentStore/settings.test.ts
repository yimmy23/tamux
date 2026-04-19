import { expect, test } from "vitest";
import { buildDaemonAgentConfig } from "../agentDaemonConfig";
import {
  DEFAULT_AGENT_SETTINGS,
  normalizeAgentSettingsFromSource,
} from "./settings";

test("xAI defaults use hosted settings", () => {
  expect(DEFAULT_AGENT_SETTINGS.xai).toMatchObject({
    base_url: "https://api.x.ai/v1",
    model: "grok-4",
    api_transport: "responses",
  });
});

test("xAI settings normalize and serialize for audio flows", () => {
  const normalized = normalizeAgentSettingsFromSource({
    active_provider: "xai",
    xai: {
      base_url: "https://api.x.ai/v1",
      model: "grok-4-voice",
      api_key: "sk-xai",
      api_transport: "chat_completions",
    },
    audio_stt_provider: "xai",
    audio_stt_model: "grok-4-voice",
    audio_tts_provider: "xai",
    audio_tts_model: "grok-4-voice",
  });

  expect(normalized.active_provider).toBe("xai");
  expect(normalized.audio_stt_provider).toBe("xai");
  expect(normalized.audio_tts_provider).toBe("xai");
  expect(normalized.xai).toMatchObject({
    base_url: "https://api.x.ai/v1",
    model: "grok-4-voice",
    api_key: "sk-xai",
    api_transport: "chat_completions",
  });

  const daemonConfig = buildDaemonAgentConfig({
    ...DEFAULT_AGENT_SETTINGS,
    active_provider: "xai",
    xai: {
      ...DEFAULT_AGENT_SETTINGS.xai,
      api_key: "sk-xai",
      model: "grok-4-voice",
    },
    audio_stt_provider: "xai",
    audio_stt_model: "grok-4-voice",
    audio_tts_provider: "xai",
    audio_tts_model: "grok-4-voice",
  });

  expect(daemonConfig.providers?.xai).toMatchObject({
    base_url: "https://api.x.ai/v1",
    model: "grok-4-voice",
  });
});

test("image generation settings prefer nested daemon config over legacy flat keys", () => {
  const normalized = normalizeAgentSettingsFromSource({
    image: {
      generation: {
        provider: "openai",
        model: "gpt-image-1",
      },
    },
    image_generation_provider: "xai",
    image_generation_model: "grok-4-image",
  } as any);

  expect(normalized.image_generation_provider).toBe("openai");
  expect(normalized.image_generation_model).toBe("gpt-image-1");
});
