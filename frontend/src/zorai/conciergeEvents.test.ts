import { describe, expect, it } from "vitest";
import { shouldAutoStartOperatorProfileFromConcierge } from "./conciergeEvents";

describe("shouldAutoStartOperatorProfileFromConcierge", () => {
  it("ignores partial concierge stream frames without actions", () => {
    expect(shouldAutoStartOperatorProfileFromConcierge({
      type: "concierge_welcome",
      content: "partial",
      actions: [],
    })).toBe(false);
  });

  it("accepts the final concierge welcome frame with actions", () => {
    expect(shouldAutoStartOperatorProfileFromConcierge({
      type: "concierge_welcome",
      content: "done",
      actions: [{ id: "open", label: "Open" }],
    })).toBe(true);
  });
});
