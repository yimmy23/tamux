import { describe, expect, it } from "vitest";

import { describeRuntimeMode } from "./runtimeMode";

describe("describeRuntimeMode", () => {
  it("explains browser preview limitations when no Electron bridge is present", () => {
    expect(describeRuntimeMode({ hasBridge: false })).toEqual({
      title: "Browser Preview Mode",
      summary: "npm run dev only serves the React UI.",
      detail:
        "The Electron bridge, daemon IPC, terminals, and provider actions are unavailable here. Use npm run dev:electron to launch the desktop shell.",
    });
  });

  it("returns null when the Electron bridge is available", () => {
    expect(describeRuntimeMode({ hasBridge: true })).toBeNull();
  });
});
