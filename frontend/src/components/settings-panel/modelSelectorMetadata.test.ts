import type { ModelDefinition } from "../../lib/agentStore";
import { normalizeFetchedRemoteModel } from "../../lib/providerModels";
import {
  buildModelSelectorMetadata,
  extractModelSelectorModalities,
  formatModelSelectorPricing,
} from "./modelSelectorMetadata";
import { expect, test } from "vitest";

test("extractModelSelectorModalities uses static model modalities", () => {
  const model: ModelDefinition = {
    id: "gpt-4.1",
    name: "GPT-4.1",
    contextWindow: 128000,
    modalities: ["text", "image"],
  };

  expect(extractModelSelectorModalities({ predefinedModel: model })).toEqual([
    "text",
    "image",
  ]);
});

test("formatModelSelectorPricing formats fetched prompt and completion pricing", () => {
  const fetchedModel = normalizeFetchedRemoteModel({
    id: "gpt-4.1",
    pricing: {
      prompt: "0.0000025",
      completion: "0.00001",
    },
  });

  expect(formatModelSelectorPricing(fetchedModel).summary).toBe(
    "Input $2.50/M tok · Output $10.00/M tok",
  );
});

test("formatModelSelectorPricing falls back to n/a when pricing is absent", () => {
  const fetchedModel = normalizeFetchedRemoteModel({
    id: "gpt-4.1",
  });

  expect(formatModelSelectorPricing(fetchedModel).summary).toBe(
    "Input n/a · Output n/a",
  );
});

test("extractModelSelectorModalities defaults to text when no modality metadata exists", () => {
  const fetchedModel = normalizeFetchedRemoteModel({
    id: "custom-model",
  });

  expect(extractModelSelectorModalities({ fetchedModel })).toEqual(["text"]);
});

test("extractModelSelectorModalities reads daemon-nested fetched metadata", () => {
  const fetchedModel = normalizeFetchedRemoteModel({
    id: "openai/gpt-audio",
    metadata: {
      architecture: {
        input_modalities: ["text", "audio"],
        output_modalities: ["text", "audio"],
      },
    },
  });

  expect(extractModelSelectorModalities({ fetchedModel })).toEqual(["text", "audio"]);
});

test("buildModelSelectorMetadata keeps static modalities and fetched pricing together", () => {
  const predefinedModel: ModelDefinition = {
    id: "gpt-4.1",
    name: "GPT-4.1",
    contextWindow: 128000,
    modalities: ["text", "image"],
  };
  const fetchedModel = normalizeFetchedRemoteModel({
    id: "gpt-4.1",
    pricing: {
      prompt: "0.0000025",
      completion: "0.00001",
    },
  });

  expect(
    buildModelSelectorMetadata({
      predefinedModel,
      fetchedModel,
    }),
  ).toMatchObject({
    modalities: ["text", "image"],
    pricingSummary: "Input $2.50/M tok · Output $10.00/M tok",
  });
});
