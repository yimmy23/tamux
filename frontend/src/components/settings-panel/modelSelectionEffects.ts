import type { ModelDefinition } from "../../lib/agentStore";
import type { FetchedRemoteModel } from "../../lib/providerModels";
import { extractModelSelectorModalities } from "./modelSelectorMetadata";

export interface ModelSelectionEffectsInput {
  enableVisionTool: boolean;
  currentSttModel: string;
  selectedModelId: string;
  predefinedModel?: ModelDefinition | null;
  fetchedModel?: Pick<FetchedRemoteModel, "pricing" | "metadata"> | null;
}

export interface ModelSelectionEffects {
  enableVision: boolean;
  promptForSttReuse: boolean;
  sttModelOnConfirm: string | null;
}

export function getModelSelectionEffects(
  input: ModelSelectionEffectsInput,
): ModelSelectionEffects {
  const modalities = extractModelSelectorModalities({
    predefinedModel: input.predefinedModel,
    fetchedModel: input.fetchedModel,
  });
  const supportsImage = modalities.includes("image");
  const supportsAudio = modalities.includes("audio");
  const selectedModelId = input.selectedModelId.trim();

  return {
    enableVision: supportsImage && !input.enableVisionTool,
    promptForSttReuse:
      supportsAudio &&
      selectedModelId.length > 0 &&
      input.currentSttModel.trim() !== selectedModelId,
    sttModelOnConfirm: supportsAudio && selectedModelId.length > 0 ? selectedModelId : null,
  };
}

export function applySttReuseDecision(
  currentSttModel: string,
  selectedModelId: string,
  confirmed: boolean,
): string {
  if (!confirmed) {
    return currentSttModel;
  }
  return selectedModelId;
}
