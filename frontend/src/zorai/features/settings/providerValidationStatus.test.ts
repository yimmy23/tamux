import { describe, expect, it } from "vitest";

import {
  formatProviderValidationError,
  getProviderValidationButtonLabel,
  markProviderValidationTesting,
  providerValidationStatusFromResult,
} from "./providerValidationStatus";

describe("provider validation status helpers", () => {
  it("marks a provider test as testing before the validation result arrives", () => {
    const statuses = markProviderValidationTesting({}, "deepseek");

    expect(statuses.deepseek).toEqual({ state: "testing", message: "Testing..." });
    expect(getProviderValidationButtonLabel(statuses, "deepseek")).toBe("Testing...");
    expect(getProviderValidationButtonLabel(statuses, "openai")).toBe("Test");
  });

  it("formats provider validation success and failures for the settings row", () => {
    expect(providerValidationStatusFromResult({ valid: true })).toEqual({
      state: "success",
      message: "ok",
    });
    expect(providerValidationStatusFromResult({ valid: false, error: "bad token" })).toEqual({
      state: "error",
      message: "bad token",
    });
    expect(providerValidationStatusFromResult({ valid: false })).toEqual({
      state: "error",
      message: "failed",
    });
    expect(formatProviderValidationError(new Error("bridge unavailable"))).toBe("bridge unavailable");
    expect(formatProviderValidationError("plain failure")).toBe("plain failure");
  });
});
