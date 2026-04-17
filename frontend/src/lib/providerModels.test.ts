import {
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
