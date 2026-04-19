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

function jsonArrayContainsAudio(value: unknown): boolean {
  return Array.isArray(value)
    && value.some((entry) => typeof entry === "string" && entry.trim().toLowerCase() === "audio");
}

function jsonArrayContainsModality(value: unknown, modality: string): boolean {
  return Array.isArray(value)
    && value.some((entry) => typeof entry === "string" && entry.trim().toLowerCase() === modality);
}

function modalitySideHasAudio(modality: string, side: "input" | "output"): boolean {
  const trimmed = modality.trim().toLowerCase();
  if (!trimmed) {
    return false;
  }

  const parts = trimmed.split("->");
  if (parts.length !== 2) {
    return false;
  }

  const directional = side === "input" ? parts[0] : parts[1];
  return directional
    .split(/[+,|/ ]/)
    .some((token) => token.trim() === "audio");
}

function jsonStringHasDirectionalAudio(
  value: unknown,
  side: "input" | "output",
): boolean {
  return typeof value === "string" && modalitySideHasAudio(value, side);
}

function jsonStringContainsModality(value: unknown, modality: string): boolean {
  return typeof value === "string"
    && value
      .trim()
      .toLowerCase()
      .split(/[^a-z]+/)
      .some((token) => token === modality);
}

function fetchedModelAudioDirectionOverride(
  model: FetchedRemoteModel,
  endpoint: "stt" | "tts",
): boolean | undefined {
  const providerPrefixSensitive = model.id.startsWith("xai/")
    || model.id.startsWith("openai/")
    || model.id.startsWith("openrouter/");
  const haystack = `${model.id} ${model.name}`.toLowerCase();

  const looksLikeStt = haystack.includes("transcribe")
    || haystack.includes("transcription")
    || haystack.includes("speech-to-text")
    || haystack.includes("speech to text")
    || haystack.includes("whisper")
    || (providerPrefixSensitive && haystack.includes("listen"));
  const looksLikeTts = haystack.includes("text-to-speech")
    || haystack.includes("text to speech")
    || haystack.includes("-tts")
    || haystack.includes(" tts")
    || (providerPrefixSensitive && haystack.includes("speak"));

  if (endpoint === "stt") {
    if (looksLikeStt && !looksLikeTts) return true;
    if (looksLikeTts && !looksLikeStt) return false;
  }
  if (endpoint === "tts") {
    if (looksLikeTts && !looksLikeStt) return true;
    if (looksLikeStt && !looksLikeTts) return false;
  }

  return undefined;
}

function modelMetadataContainsAudio(
  model: FetchedRemoteModel,
  endpoint: "stt" | "tts",
): boolean {
  const value = model.metadata;
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return false;
  }
  const record = value as Record<string, unknown>;
  const architecture = record.architecture && typeof record.architecture === "object" && !Array.isArray(record.architecture)
    ? record.architecture as Record<string, unknown>
    : null;

  const inputAudio = jsonArrayContainsAudio(
    architecture?.input_modalities ?? record.input_modalities,
  );
  const outputAudio = jsonArrayContainsAudio(
    architecture?.output_modalities ?? record.output_modalities,
  );
  const modalityInputAudio = jsonStringHasDirectionalAudio(
    architecture?.modality ?? record.modality,
    "input",
  );
  const modalityOutputAudio = jsonStringHasDirectionalAudio(
    architecture?.modality ?? record.modality,
    "output",
  );

  const directionalMatch = endpoint === "stt"
    ? inputAudio || modalityInputAudio
    : outputAudio || modalityOutputAudio;
  if (directionalMatch) {
    return true;
  }

  const override = fetchedModelAudioDirectionOverride(model, endpoint);
  return override ?? false;
}

export function filterFetchedModelsForAudio(
  models: FetchedRemoteModel[],
  endpoint: "stt" | "tts",
): FetchedRemoteModel[] {
  return models.filter((model) => modelMetadataContainsAudio(model, endpoint));
}

function modelMetadataContainsImage(model: FetchedRemoteModel): boolean {
  const value = model.metadata;
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return false;
  }
  const record = value as Record<string, unknown>;
  const architecture = record.architecture && typeof record.architecture === "object" && !Array.isArray(record.architecture)
    ? record.architecture as Record<string, unknown>
    : null;

  return Boolean(
    jsonArrayContainsModality(architecture?.input_modalities ?? record.input_modalities, "image")
    || jsonArrayContainsModality(architecture?.output_modalities ?? record.output_modalities, "image")
    || jsonArrayContainsModality(architecture?.modalities ?? record.modalities, "image")
    || jsonStringContainsModality(architecture?.modality ?? record.modality, "image")
    || model.pricing?.image
  );
}

export function filterFetchedModelsForImageGeneration(
  models: FetchedRemoteModel[],
): FetchedRemoteModel[] {
  return models.filter((model) => modelMetadataContainsImage(model));
}
