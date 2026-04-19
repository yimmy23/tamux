import {
  filterFetchedModelsForAudio,
  formatRemoteModelPricingSubtitle,
  normalizeFetchedRemoteModel,
} from "./providerModels";
import { expect, test } from "vitest";

test("normalizeFetchedRemoteModel retains arbitrary metadata", () => {
  const model = normalizeFetchedRemoteModel({
    id: "openai/gpt-5.4",
    name: "GPT-5.4",
    context_window: 128000,
    pricing: {
      prompt: "0.0000025",
      completion: "0.00001",
    },
    architecture: {
      modality: "text->text",
    },
  });

  expect(model.metadata?.architecture?.modality).toBe("text->text");
});

test("formatRemoteModelPricingSubtitle formats prompt and completion cost", () => {
  const model = normalizeFetchedRemoteModel({
    id: "openai/gpt-5.4",
    pricing: {
      prompt: "0.0000025",
      completion: "0.00001",
    },
  });

  expect(formatRemoteModelPricingSubtitle(model)).toBe(
    "Prompt $2.50/M tok, completion $10.00/M tok",
  );
});

test("formatRemoteModelPricingSubtitle returns free for zero prompt and completion", () => {
  const model = normalizeFetchedRemoteModel({
    id: "meta-llama/free",
    pricing: {
      prompt: "0",
      completion: "0",
    },
  });

  expect(formatRemoteModelPricingSubtitle(model)).toBe("free");
});

test("filterFetchedModelsForAudio keeps multimodal and audio-priced models for stt", () => {
  const models = [
    normalizeFetchedRemoteModel({
      id: "openai/gpt-audio",
      pricing: { audio: "0.000032" },
      architecture: {
        input_modalities: ["text", "audio"],
        output_modalities: ["text"],
      },
    }),
    normalizeFetchedRemoteModel({
      id: "openai/gpt-text",
      pricing: { prompt: "0.0000025" },
      architecture: {
        input_modalities: ["text"],
        output_modalities: ["text"],
      },
    }),
  ];

  expect(filterFetchedModelsForAudio(models, "stt").map((model) => model.id)).toEqual([
    "openai/gpt-audio",
  ]);
});

test("filterFetchedModelsForAudio keeps audio-output models for tts", () => {
  const models = [
    normalizeFetchedRemoteModel({
      id: "openai/gpt-audio",
      architecture: {
        modality: "text+audio->text+audio",
      },
    }),
    normalizeFetchedRemoteModel({
      id: "openai/gpt-text",
      architecture: {
        modality: "text->text",
      },
    }),
  ];

  expect(filterFetchedModelsForAudio(models, "tts").map((model) => model.id)).toEqual([
    "openai/gpt-audio",
  ]);
});

test("filterFetchedModelsForAudio recognizes top-level modalities metadata", () => {
  const models = [
    normalizeFetchedRemoteModel({
      id: "groq/whisper-large-v3",
      modalities: ["audio", "text"],
    }),
    normalizeFetchedRemoteModel({
      id: "groq/llama-3.3-70b-versatile",
      modalities: ["text"],
    }),
  ];

  expect(filterFetchedModelsForAudio(models, "stt").map((model) => model.id)).toEqual([
    "groq/whisper-large-v3",
  ]);
});
