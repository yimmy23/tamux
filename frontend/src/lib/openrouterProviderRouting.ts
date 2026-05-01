export type OpenRouterEndpointProvider = {
  slug: string;
  name: string;
};

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

function normalizeString(value: unknown): string | null {
  if (typeof value !== "string") return null;
  const trimmed = value.trim();
  return trimmed.length > 0 ? trimmed : null;
}

export function normalizeOpenRouterProviderSlugs(values: unknown): string[] {
  if (!Array.isArray(values)) return [];
  const out: string[] = [];
  for (const value of values) {
    const slug = normalizeString(value);
    if (!slug || out.includes(slug)) continue;
    out.push(slug);
  }
  return out;
}

export function buildOpenRouterEndpointsUrl(baseUrl: string, modelId: string): string | null {
  const trimmedModel = modelId.trim();
  const [author, ...slugParts] = trimmedModel.split("/");
  const slug = slugParts.join("/");
  if (!author || !slug) return null;

  const normalizedBase = (baseUrl.trim() || "https://openrouter.ai/api/v1")
    .replace(/\/chat\/completions\/?$/, "")
    .replace(/\/responses\/?$/, "")
    .replace(/\/+$/, "");

  return `${normalizedBase}/models/${encodeURIComponent(author)}/${encodeURIComponent(slug)}/endpoints`;
}

export function parseOpenRouterEndpointProviders(payload: unknown): OpenRouterEndpointProvider[] {
  const data = isRecord(payload) ? payload.data : undefined;
  const endpoints = isRecord(data) ? data.endpoints : undefined;
  if (!Array.isArray(endpoints)) return [];

  const providers: OpenRouterEndpointProvider[] = [];
  for (const endpoint of endpoints) {
    if (!isRecord(endpoint)) continue;
    const slug = normalizeString(endpoint.tag)
      || normalizeString(endpoint.slug)
      || normalizeString(endpoint.provider_slug);
    if (!slug || providers.some((provider) => provider.slug === slug)) continue;
    const name = normalizeString(endpoint.provider_name)
      || normalizeString(endpoint.name)
      || slug;
    providers.push({ slug, name });
  }
  return providers;
}

export async function fetchOpenRouterEndpointProviders({
  baseUrl,
  model,
  apiKey,
  signal,
}: {
  baseUrl: string;
  model: string;
  apiKey?: string;
  signal?: AbortSignal;
}): Promise<OpenRouterEndpointProvider[]> {
  const url = buildOpenRouterEndpointsUrl(baseUrl, model);
  if (!url) return [];

  const headers: Record<string, string> = {};
  const trimmedKey = apiKey?.trim();
  if (trimmedKey) {
    headers.Authorization = `Bearer ${trimmedKey}`;
  }

  const response = await fetch(url, { headers, signal });
  if (!response.ok) {
    throw new Error(`OpenRouter endpoint lookup failed: ${response.status}`);
  }
  return parseOpenRouterEndpointProviders(await response.json());
}
