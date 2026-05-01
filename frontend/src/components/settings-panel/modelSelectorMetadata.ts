import {
  getModelModalities,
  type ModelDefinition,
  type Modality,
} from "../../lib/agentStore";
import type { FetchedRemoteModel, RemoteModelPricing } from "../../lib/providerModels";

const MODALITY_ORDER: Modality[] = ["text", "image", "audio", "video"];

export interface ModelSelectorMetadataInput {
  predefinedModel?: ModelDefinition | null;
  fetchedModel?: Pick<FetchedRemoteModel, "pricing" | "metadata"> | null;
}

export interface ModelSelectorPricingDetails {
  inputPrice: string;
  outputPrice: string;
  summary: string;
}

export interface ModelSelectorMetadata {
  modalities: Modality[];
  pricingSummary: string;
  inputPrice: string;
  outputPrice: string;
}

function appendUnique(target: Modality[], modality: Modality): void {
  if (!target.includes(modality)) {
    target.push(modality);
  }
}

function parseModalitiesFromArray(value: unknown, target: Modality[]): void {
  if (!Array.isArray(value)) return;
  for (const entry of value) {
    if (typeof entry !== "string") continue;
    const normalized = entry.trim().toLowerCase();
    if (isKnownModality(normalized)) {
      appendUnique(target, normalized);
    }
  }
}

function parseModalitiesFromString(value: unknown, target: Modality[]): void {
  if (typeof value !== "string") return;
  for (const token of value.split(/[^a-zA-Z]+/)) {
    const normalized = token.trim().toLowerCase();
    if (isKnownModality(normalized)) {
      appendUnique(target, normalized);
    }
  }
}

function isKnownModality(value: string): value is Modality {
  return MODALITY_ORDER.includes(value as Modality);
}

function objectRecord(value: unknown): Record<string, unknown> | null {
  return value && typeof value === "object" && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
}

function fetchedMetadataRecord(
  fetchedModel?: Pick<FetchedRemoteModel, "metadata"> | null,
): Record<string, unknown> | null {
  const metadata = objectRecord(fetchedModel?.metadata);
  if (!metadata) return null;
  const nested = objectRecord(metadata.metadata);
  return nested ? { ...metadata, ...nested } : metadata;
}

function extractFetchedModalities(
  fetchedModel?: Pick<FetchedRemoteModel, "pricing" | "metadata"> | null,
): Modality[] {
  if (!fetchedModel) return [];

  const metadata = fetchedMetadataRecord(fetchedModel);
  const architecture = objectRecord(metadata?.architecture);
  const modalities: Modality[] = [];

  parseModalitiesFromArray(
    architecture?.input_modalities ?? metadata?.input_modalities ?? metadata?.modalities,
    modalities,
  );
  parseModalitiesFromArray(
    architecture?.output_modalities ?? metadata?.output_modalities ?? metadata?.modalities,
    modalities,
  );
  parseModalitiesFromString(architecture?.modality ?? metadata?.modality, modalities);

  if (fetchedModel.pricing?.image) {
    appendUnique(modalities, "image");
  }
  if (fetchedModel.pricing?.audio) {
    appendUnique(modalities, "audio");
  }

  return modalities.sort(
    (left, right) => MODALITY_ORDER.indexOf(left) - MODALITY_ORDER.indexOf(right),
  );
}

function parseRate(value?: string | null): number | null {
  if (!value) return null;
  const parsed = Number.parseFloat(value);
  return Number.isFinite(parsed) ? parsed : null;
}

function formatRate(value?: string | null): string {
  const normalized = typeof value === "string" ? value.trim() : "";
  if (!normalized) return "n/a";
  const parsed = parseRate(normalized);
  if (parsed == null) return normalized;
  return `$${(parsed * 1_000_000).toFixed(2)}/M tok`;
}

function firstPricingSignal(values: Array<string | null | undefined>): string {
  const nextValue = values.find((value) => typeof value === "string" && value.trim().length > 0);
  return formatRate(nextValue ?? null);
}

function pricingFields(pricing?: RemoteModelPricing | null): ModelSelectorPricingDetails {
  if (!pricing) {
    return {
      inputPrice: "n/a",
      outputPrice: "n/a",
      summary: "Input n/a · Output n/a",
    };
  }

  const inputPrice = firstPricingSignal([
    pricing.prompt,
    pricing.completion,
    pricing.request,
    pricing.image,
    pricing.internal_reasoning,
    pricing.web_search,
    pricing.audio,
    pricing.input_cache_read,
    pricing.input_cache_write,
  ]);
  const outputPrice = firstPricingSignal([
    pricing.completion,
    pricing.prompt,
    pricing.request,
    pricing.image,
    pricing.internal_reasoning,
    pricing.web_search,
    pricing.audio,
    pricing.input_cache_read,
    pricing.input_cache_write,
  ]);

  return {
    inputPrice,
    outputPrice,
    summary: `Input ${inputPrice} · Output ${outputPrice}`,
  };
}

export function extractModelSelectorModalities(
  input: ModelSelectorMetadataInput,
): Modality[] {
  const modalities: Modality[] = [];
  for (const modality of getModelModalities(input.predefinedModel ?? undefined)) {
    appendUnique(modalities, modality);
  }
  for (const modality of extractFetchedModalities(input.fetchedModel)) {
    appendUnique(modalities, modality);
  }
  return modalities.sort(
    (left, right) => MODALITY_ORDER.indexOf(left) - MODALITY_ORDER.indexOf(right),
  );
}

export function formatModelSelectorPricing(
  fetchedModel?: Pick<FetchedRemoteModel, "pricing"> | null,
): ModelSelectorPricingDetails {
  return pricingFields(fetchedModel?.pricing);
}

export function buildModelSelectorMetadata(
  input: ModelSelectorMetadataInput,
): ModelSelectorMetadata {
  const pricing = formatModelSelectorPricing(input.fetchedModel);
  return {
    modalities: extractModelSelectorModalities(input),
    pricingSummary: pricing.summary,
    inputPrice: pricing.inputPrice,
    outputPrice: pricing.outputPrice,
  };
}
