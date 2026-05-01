export function buildModelFetchKey({
  providerId,
  baseUrl,
  apiKey,
  outputModalities,
}: {
  providerId: string;
  baseUrl: string;
  apiKey: string;
  outputModalities?: string;
}): string {
  return [
    providerId.trim(),
    baseUrl.trim(),
    apiKey,
    outputModalities?.trim() ?? "",
  ].join("\n");
}

export function shouldFetchRemoteModels({
  supportsModelFetch,
  providerId,
  authSource,
}: {
  supportsModelFetch: boolean;
  providerId: string;
  authSource?: string;
}): boolean {
  return supportsModelFetch
    && !(providerId === "openai" && authSource === "chatgpt_subscription");
}
