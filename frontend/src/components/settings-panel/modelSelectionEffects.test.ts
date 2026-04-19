import type { ModelDefinition } from "../../lib/agentStore";
import { normalizeFetchedRemoteModel } from "../../lib/providerModels";
import {
  applySttReuseDecision,
  getModelSelectionEffects,
} from "./modelSelectionEffects";
import { expect, test } from "vitest";

test("selecting an image-capable model requests enabling vision when disabled", () => {
  const predefinedModel: ModelDefinition = {
    id: "gpt-4.1",
    name: "GPT-4.1",
    contextWindow: 128000,
    modalities: ["text", "image"],
  };

  expect(
    getModelSelectionEffects({
      enableVisionTool: false,
      currentSttModel: "whisper-1",
      selectedModelId: "gpt-4.1",
      predefinedModel,
    }),
  ).toMatchObject({
    enableVision: true,
    promptForSttReuse: false,
  });
});

test("selecting an image-capable model does nothing extra when vision is already enabled", () => {
  const predefinedModel: ModelDefinition = {
    id: "gpt-4.1",
    name: "GPT-4.1",
    contextWindow: 128000,
    modalities: ["text", "image"],
  };

  expect(
    getModelSelectionEffects({
      enableVisionTool: true,
      currentSttModel: "whisper-1",
      selectedModelId: "gpt-4.1",
      predefinedModel,
    }).enableVision,
  ).toBe(false);
});

test("selecting an audio-capable model requests STT confirmation", () => {
  const fetchedModel = normalizeFetchedRemoteModel({
    id: "gpt-4o-audio",
    pricing: {
      audio: "0.000032",
    },
    architecture: {
      input_modalities: ["text", "audio"],
      output_modalities: ["text"],
    },
  });

  expect(
    getModelSelectionEffects({
      enableVisionTool: false,
      currentSttModel: "whisper-1",
      selectedModelId: "gpt-4o-audio",
      fetchedModel,
    }),
  ).toMatchObject({
    promptForSttReuse: true,
    sttModelOnConfirm: "gpt-4o-audio",
  });
});

test("confirming applies the selected model to audio_stt_model", () => {
  expect(applySttReuseDecision("whisper-1", "gpt-4o-audio", true)).toBe(
    "gpt-4o-audio",
  );
});

test("declining leaves audio_stt_model unchanged", () => {
  expect(applySttReuseDecision("whisper-1", "gpt-4o-audio", false)).toBe(
    "whisper-1",
  );
});
