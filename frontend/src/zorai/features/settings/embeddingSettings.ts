import type { AgentSettings } from "@/lib/agentStore";
import { embeddingDimensionsFromFetchedModel, type FetchedRemoteModel } from "@/lib/providerModels";

export function embeddingSettingsPatchForModelSelection(
  value: string,
  details?: { fetchedModel?: Pick<FetchedRemoteModel, "metadata"> | null },
): Pick<AgentSettings, "semantic_embedding_model"> & Partial<Pick<AgentSettings, "semantic_embedding_dimensions">> {
  const dimensions = embeddingDimensionsFromFetchedModel(details?.fetchedModel);
  return {
    semantic_embedding_model: value,
    ...(dimensions != null ? { semantic_embedding_dimensions: dimensions } : {}),
  };
}
