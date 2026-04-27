import { describe, expect, it } from "vitest";
import { getDefaultZoraiTool, zoraiTools } from "./tools";

describe("Zorai tools", () => {
  it("opens terminal as the secondary operator tool by default", () => {
    expect(getDefaultZoraiTool()).toBe("terminal");
  });

  it("exposes runtime-backed tool destinations", () => {
    expect(zoraiTools.map((tool) => tool.id)).toEqual([
      "terminal",
      "canvas",
      "files",
      "browser",
      "history",
      "system",
      "vault",
    ]);
  });

  it("defines user-facing copy for every tool", () => {
    for (const tool of zoraiTools) {
      expect(tool.title.trim().length).toBeGreaterThan(0);
      expect(tool.description.trim().length).toBeGreaterThan(0);
    }
  });
});
