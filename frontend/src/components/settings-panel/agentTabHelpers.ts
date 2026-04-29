import type { AgentProviderId, ModelDefinition, ProviderAuthState } from "../../lib/agentStore";
import {
  AGENT_PROVIDER_IDS,
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

const XIAOMI_TTS_MODELS: ModelDefinition[] = [
  { id: "mimo-v2.5-tts", name: "MiMo V2.5 TTS", contextWindow: 128000, modalities: ["audio"] },
  { id: "mimo-v2.5-tts-voiceclone", name: "MiMo V2.5 TTS VoiceClone", contextWindow: 128000, modalities: ["audio"] },
  { id: "mimo-v2.5-tts-voicedesign", name: "MiMo V2.5 TTS VoiceDesign", contextWindow: 128000, modalities: ["audio"] },
];

const MINIMAX_TTS_MODELS: ModelDefinition[] = [
  { id: "speech-2.8-hd", name: "MiniMax Speech 2.8 HD", contextWindow: 0, modalities: ["audio"] },
  { id: "speech-2.8-turbo", name: "MiniMax Speech 2.8 Turbo", contextWindow: 0, modalities: ["audio"] },
  { id: "speech-2.6-hd", name: "MiniMax Speech 2.6 HD", contextWindow: 0, modalities: ["audio"] },
  { id: "speech-2.6-turbo", name: "MiniMax Speech 2.6 Turbo", contextWindow: 0, modalities: ["audio"] },
  { id: "speech-02-hd", name: "MiniMax Speech 02 HD", contextWindow: 0, modalities: ["audio"] },
  { id: "speech-02-turbo", name: "MiniMax Speech 02 Turbo", contextWindow: 0, modalities: ["audio"] },
  { id: "speech-01-hd", name: "MiniMax Speech 01 HD", contextWindow: 0, modalities: ["audio"] },
  { id: "speech-01-turbo", name: "MiniMax Speech 01 Turbo", contextWindow: 0, modalities: ["audio"] },
];

const OPENAI_IMAGE_MODELS: ModelDefinition[] = [
  { id: "gpt-image-1", name: "GPT Image 1", contextWindow: 0, modalities: ["image"] },
  { id: "gpt-image-2", name: "GPT Image 2", contextWindow: 0, modalities: ["image"] },
];

const OPENROUTER_IMAGE_MODELS: ModelDefinition[] = [
  { id: "openai/gpt-image-1", name: "OpenAI GPT Image 1", contextWindow: 0, modalities: ["image"] },
  { id: "openai/gpt-image-2", name: "OpenAI GPT Image 2", contextWindow: 0, modalities: ["image"] },
];

const MINIMAX_IMAGE_MODELS: ModelDefinition[] = [
  { id: "image-01", name: "MiniMax Image 01", contextWindow: 0, modalities: ["image"] },
];

export type ProviderOption = {
  id: AgentProviderId;
  label: string;
};

function isBuiltInProviderId(providerId: string): boolean {
  return AGENT_PROVIDER_IDS.some((id) => id === providerId);
}

export function providerAuthStateIsSelectable(state: ProviderAuthState): boolean {
  return state.authenticated
    || state.provider_id === "custom"
    || state.provider_id === "azure-openai"
    || !isBuiltInProviderId(state.provider_id);
}

export function selectableProviderAuthStates(
  providerAuthStates: ProviderAuthState[],
): ProviderAuthState[] {
  return providerAuthStates.filter(providerAuthStateIsSelectable);
}

export function buildProviderOptions(
  builtInProviderOptions: readonly ProviderOption[],
  providerAuthStates: ProviderAuthState[],
): { allProviderOptions: ProviderOption[]; providerOptions: ProviderOption[] } {
  const allProviderOptions: ProviderOption[] = [
    ...builtInProviderOptions,
    ...providerAuthStates
      .filter((state) => !builtInProviderOptions.some((provider) => provider.id === state.provider_id))
      .map((state) => ({ id: state.provider_id as AgentProviderId, label: state.provider_name || state.provider_id })),
  ];
  const providerOptions = allProviderOptions.filter((provider) => {
    const authState = providerAuthStates.find((state) => state.provider_id === provider.id);
    if (authState) {
      return providerAuthStateIsSelectable(authState);
    }
    return provider.id === "custom" || provider.id === "azure-openai" || !isBuiltInProviderId(provider.id);
  });

  return { allProviderOptions, providerOptions };
}

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
  if ((providerId === "minimax" || providerId === "minimax-coding-plan") && kind === "tts") {
    return MINIMAX_TTS_MODELS;
  }
  if (providerId === "xiaomi-mimo-token-plan" && kind === "tts") {
    return XIAOMI_TTS_MODELS;
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

export function filterAudioProviderOptions(
  providerOptions: ProviderOption[],
  kind: "stt" | "tts",
): ProviderOption[] {
  return providerOptions.filter((provider) => providerSupportsAudioTool(provider.id, kind));
}

export function imageGenerationModelOptions(providerId: AgentProviderId): ModelDefinition[] {
  if (providerId === "openai" || providerId === "azure-openai" || providerId === "custom") {
    return OPENAI_IMAGE_MODELS;
  }
  if (providerId === "openrouter") {
    return OPENROUTER_IMAGE_MODELS;
  }
  if (providerId === "minimax" || providerId === "minimax-coding-plan") {
    return MINIMAX_IMAGE_MODELS;
  }
  return [];
}

export function embeddingModelOptions(providerId: AgentProviderId): ModelDefinition[] {
  if (providerId === "openai") {
    return [
      { id: "text-embedding-3-small", name: "Text Embedding 3 Small", contextWindow: 8192, modalities: ["embedding"] },
      { id: "text-embedding-3-large", name: "Text Embedding 3 Large", contextWindow: 8192, modalities: ["embedding"] },
    ];
  }
  if (providerId === "azure-openai" || providerId === "custom") {
    return [
      { id: "text-embedding-3-small", name: "Text Embedding 3 Small", contextWindow: 8192, modalities: ["embedding"] },
    ];
  }
  return [];
}

export function normalizeEmbeddingModelForProviderChange(
  providerId: AgentProviderId,
  currentModel: string,
): string {
  const normalizedCurrentModel = currentModel.trim();
  const knownEmbeddingModels = embeddingModelOptions(providerId);
  if (knownEmbeddingModels.length > 0 && normalizedCurrentModel.length === 0) {
    return knownEmbeddingModels[0]?.id ?? "";
  }
  if (knownEmbeddingModels.length > 0) {
    return knownEmbeddingModels.some((model) => model.id === normalizedCurrentModel)
      ? normalizedCurrentModel
      : knownEmbeddingModels[0]?.id ?? "";
  }
  return "";
}

export function filterEmbeddingProviderOptions(providerOptions: ProviderOption[]): ProviderOption[] {
  return providerOptions.filter((provider) => embeddingModelOptions(provider.id).length > 0);
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
