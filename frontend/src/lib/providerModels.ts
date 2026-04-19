export interface RemoteModelPricing {
  prompt?: string | null;
  completion?: string | null;
  image?: string | null;
  request?: string | null;
  web_search?: string | null;
  internal_reasoning?: string | null;
  input_cache_read?: string | null;
  input_cache_write?: string | null;
  audio?: string | null;
}

export interface RemoteModelMetadata extends Record<string, any> {
  pricing?: RemoteModelPricing | null;
}

export interface FetchedRemoteModel {
  id: string;
  name: string;
  contextWindow: number;
  pricing?: RemoteModelPricing | null;
  metadata?: RemoteModelMetadata | null;
}

function normalizeString(value: unknown): string | undefined {
  if (typeof value === "string") {
    const trimmed = value.trim();
    return trimmed.length > 0 ? trimmed : undefined;
  }
  if (typeof value === "number" && Number.isFinite(value)) {
    return `${value}`;
  }
  return undefined;
}

function normalizePricing(value: unknown): RemoteModelPricing | undefined {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return undefined;
  }
  const pricingValue = value as Record<string, unknown>;
  const pricing: RemoteModelPricing = {
    prompt: normalizeString(pricingValue.prompt),
    completion: normalizeString(pricingValue.completion),
    image: normalizeString(pricingValue.image),
    request: normalizeString(pricingValue.request),
    web_search: normalizeString(pricingValue.web_search),
    internal_reasoning: normalizeString(pricingValue.internal_reasoning),
    input_cache_read: normalizeString(pricingValue.input_cache_read),
    input_cache_write: normalizeString(pricingValue.input_cache_write),
    audio: normalizeString(pricingValue.audio),
  };

  return Object.values(pricing).some(Boolean) ? pricing : undefined;
}

function parseRate(value?: string | null): number | null {
  if (!value) return null;
  const parsed = Number.parseFloat(value);
  return Number.isFinite(parsed) ? parsed : null;
}

function formatRatePerMillion(value?: string | null): string | null {
  const parsed = parseRate(value);
  if (parsed == null) return null;
  return `$${(parsed * 1_000_000).toFixed(2)}/M tok`;
}

export function normalizeFetchedRemoteModel(value: unknown): FetchedRemoteModel {
  const record = value && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : {};
  const id = normalizeString(record.id) || "";
  const name = normalizeString(record.name) || id;
  const rawContextWindow = record.context_window ?? record.contextWindow;
  const contextWindow = typeof rawContextWindow === "number" && Number.isFinite(rawContextWindow)
    ? Math.max(0, Math.trunc(rawContextWindow))
    : 0;
  const pricing = normalizePricing(record.pricing);
  const metadata = { ...record } as RemoteModelMetadata;

  return {
    id,
    name,
    contextWindow,
    pricing,
    metadata,
  };
}

export function formatRemoteModelPricingSubtitle(
  model: Pick<FetchedRemoteModel, "pricing">,
): string | null {
  const pricing = model.pricing;
  if (!pricing) return null;

  const promptRate = parseRate(pricing.prompt);
  const completionRate = parseRate(pricing.completion);
  if (promptRate === 0 && completionRate === 0) {
    return "free";
  }

  const parts: string[] = [];
  const prompt = formatRatePerMillion(pricing.prompt);
  if (prompt) {
    parts.push(`Prompt ${prompt}`);
  }
  const completion = formatRatePerMillion(pricing.completion);
  if (completion) {
    parts.push(`completion ${completion}`);
  }

  return parts.length > 0 ? parts.join(", ") : null;
}

function modelMetadataContainsAudio(
  value: unknown,
  endpoint: "stt" | "tts",
): boolean {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return false;
  }
  const record = value as Record<string, unknown>;
  const architecture = record.architecture && typeof record.architecture === "object" && !Array.isArray(record.architecture)
    ? record.architecture as Record<string, unknown>
    : null;
  const hasAudioInArray = (source: unknown): boolean =>
    Array.isArray(source)
      && source.some((entry) => typeof entry === "string" && entry.trim().toLowerCase() === "audio");
  const hasAudioInString = (source: unknown): boolean =>
    typeof source === "string" && source.toLowerCase().includes("audio");

  const inputAudio = hasAudioInArray(architecture?.input_modalities ?? record.input_modalities ?? record.modalities);
  const outputAudio = hasAudioInArray(architecture?.output_modalities ?? record.output_modalities ?? record.modalities);
  const modalityAudio = hasAudioInString(architecture?.modality ?? record.modality);

  return endpoint === "stt"
    ? inputAudio || modalityAudio
    : outputAudio || modalityAudio;
}

export function filterFetchedModelsForAudio(
  models: FetchedRemoteModel[],
  endpoint: "stt" | "tts",
): FetchedRemoteModel[] {
  return models.filter((model) =>
    Boolean(model.pricing?.audio)
    || modelMetadataContainsAudio(model.metadata, endpoint));
}
