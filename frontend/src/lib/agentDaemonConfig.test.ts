import { describe, expect, it } from "vitest";
import { buildDaemonAgentConfig } from "./agentDaemonConfig";
import { DEFAULT_AGENT_SETTINGS } from "./agentStore/settings";

describe("daemon agent config audio wiring", () => {
  it("serializes canonical nested audio settings and referenced audio providers", () => {
    const daemonConfig = buildDaemonAgentConfig({
      ...DEFAULT_AGENT_SETTINGS,
      active_provider: "openai",
      openai: {
        ...DEFAULT_AGENT_SETTINGS.openai,
        api_key: "sk-active",
      },
      audio_stt_provider: "openai",
      audio_stt_model: "gpt-4o-transcribe",
      audio_stt_language: "pl",
      audio_tts_provider: "custom",
      audio_tts_model: "sonic-voice",
      audio_tts_voice: "alloy",
      custom: {
        ...DEFAULT_AGENT_SETTINGS.custom,
        base_url: "https://audio.example/v1",
        model: "fallback-audio-model",
        api_key: "sk-audio",
      },
    });

    expect(daemonConfig.audio).toEqual({
      stt: {
        enabled: true,
        provider: "openai",
        model: "gpt-4o-transcribe",
        language: "pl",
      },
      tts: {
        enabled: true,
        provider: "custom",
        model: "sonic-voice",
        voice: "alloy",
        auto_speak: false,
      },
    });

    expect(daemonConfig.providers?.openai).toMatchObject({
      base_url: DEFAULT_AGENT_SETTINGS.openai.base_url,
      model: DEFAULT_AGENT_SETTINGS.openai.model,
    });
    expect(daemonConfig.providers?.custom).toMatchObject({
      base_url: "https://audio.example/v1",
      model: "fallback-audio-model",
    });
  });

  it("serializes canonical nested image generation settings and referenced image providers", () => {
    const daemonConfig = buildDaemonAgentConfig({
      ...DEFAULT_AGENT_SETTINGS,
      active_provider: "openai",
      openai: {
        ...DEFAULT_AGENT_SETTINGS.openai,
        api_key: "sk-active",
      },
      image_generation_provider: "xai",
      image_generation_model: "grok-4-image",
      xai: {
        ...DEFAULT_AGENT_SETTINGS.xai,
        api_key: "sk-image",
        model: "grok-4-image",
      },
    } as any);

    expect(daemonConfig.image).toEqual({
      generation: {
        provider: "xai",
        model: "grok-4-image",
      },
    });

    expect(daemonConfig.providers?.xai).toMatchObject({
      base_url: DEFAULT_AGENT_SETTINGS.xai.base_url,
      model: "grok-4-image",
    });
  });
});
