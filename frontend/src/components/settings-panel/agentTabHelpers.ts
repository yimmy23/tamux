import type { AgentProviderId, ModelDefinition } from "../../lib/agentStore";
import {
  getDefaultModelForProvider,
  providerSupportsAudioTool,
} from "../../lib/agentStore";

const OPENAI_STT_MODELS: ModelDefinition[] = [
  { id: "gpt-4o-transcribe", name: "GPT-4o Transcribe", contextWindow: 128000, modalities: ["audio"] },
  { id: "gpt-4o-mini-transcribe", name: "GPT-4o Mini Transcribe", contextWindow: 128000, modalities: ["audio"] },
  { id: "whisper-1", name: "Whisper 1", contextWindow: 0, modalities: ["audio"] },
];

const OPENAI_TTS_MODELS: ModelDefinition[] = [
  { id: "gpt-4o-mini-tts", name: "GPT-4o Mini TTS", contextWindow: 128000, modalities: ["audio"] },
  { id: "tts-1", name: "TTS 1", contextWindow: 0, modalities: ["audio"] },
  { id: "tts-1-hd", name: "TTS 1 HD", contextWindow: 0, modalities: ["audio"] },
];

const XAI_AUDIO_MODELS: ModelDefinition[] = [
  { id: "grok-4", name: "Grok 4", contextWindow: 262144, modalities: ["audio"] },
];

const OPENAI_IMAGE_MODELS: ModelDefinition[] = [
  { id: "gpt-image-1", name: "GPT Image 1", contextWindow: 0, modalities: ["image"] },
];

const OPENROUTER_IMAGE_MODELS: ModelDefinition[] = [
  { id: "openai/gpt-image-1", name: "OpenAI GPT Image 1", contextWindow: 0, modalities: ["image"] },
];

export type ProviderOption = {
  id: AgentProviderId;
  label: string;
};

export function audioModelOptions(
  providerId: AgentProviderId,
  kind: "stt" | "tts",
): ModelDefinition[] | undefined {
  if (providerId === "openai" || providerId === "azure-openai") {
    return kind === "stt" ? OPENAI_STT_MODELS : OPENAI_TTS_MODELS;
  }
  if (providerId === "xai") {
    return XAI_AUDIO_MODELS;
  }
  return undefined;
}

export function normalizeAudioModelForProviderChange(
  providerId: AgentProviderId,
  kind: "stt" | "tts",
  currentModel: string,
): string {
  const normalizedCurrentModel = currentModel.trim();
  const knownAudioModels = audioModelOptions(providerId, kind) ?? [];
  if (knownAudioModels.length > 0 && normalizedCurrentModel.length === 0) {
    return knownAudioModels[0]?.id ?? "";
  }
  if (knownAudioModels.length > 0) {
    return knownAudioModels.some((model) => model.id === normalizedCurrentModel)
      ? normalizedCurrentModel
      : knownAudioModels[0]?.id ?? "";
  }
  if (providerId === "custom") {
    return normalizedCurrentModel || getDefaultModelForProvider(providerId);
  }
  return "";
}

export function filterAudioProviderOptions(providerOptions: ProviderOption[]): ProviderOption[] {
  return providerOptions.filter((provider) => providerSupportsAudioTool(provider.id, "stt"));
}

export function imageGenerationModelOptions(providerId: AgentProviderId): ModelDefinition[] {
  if (providerId === "openai" || providerId === "azure-openai" || providerId === "custom") {
    return OPENAI_IMAGE_MODELS;
  }
  if (providerId === "openrouter") {
    return OPENROUTER_IMAGE_MODELS;
  }
  return [];
}

export function normalizeImageGenerationModelForProviderChange(
  providerId: AgentProviderId,
  currentModel: string,
): string {
  const normalizedCurrentModel = currentModel.trim();
  const knownImageModels = imageGenerationModelOptions(providerId);
  if (knownImageModels.length > 0 && normalizedCurrentModel.length === 0) {
    return knownImageModels[0]?.id ?? "";
  }
  if (knownImageModels.length > 0) {
    return knownImageModels.some((model) => model.id === normalizedCurrentModel)
      ? normalizedCurrentModel
      : knownImageModels[0]?.id ?? "";
  }
  if (providerId === "custom") {
    return normalizedCurrentModel || "gpt-image-1";
  }
  return "";
}

export function filterImageGenerationProviderOptions(providerOptions: ProviderOption[]): ProviderOption[] {
  return providerOptions.filter((provider) =>
    provider.id === "custom" || imageGenerationModelOptions(provider.id).length > 0);
}

export function normalizeLlmStreamTimeoutInput(value: string): number | null {
  const parsed = Number.parseInt(value, 10);
  if (Number.isNaN(parsed)) {
    return null;
  }
  return Math.min(1800, Math.max(30, parsed));
}
