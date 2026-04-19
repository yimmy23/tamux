import { describe, expect, it } from "vitest";

import { usesDashScopeEnableThinking } from "./shared";

describe("usesDashScopeEnableThinking", () => {
  it("recognizes current qwen defaults on the DashScope path", () => {
    expect(usesDashScopeEnableThinking("qwen", "qwen-max")).toBe(true);
    expect(usesDashScopeEnableThinking("qwen", "qwen-plus")).toBe(true);
    expect(usesDashScopeEnableThinking("qwen", "qwen-turbo")).toBe(false);
  });

  it("keeps coding-plan thinking models enabled", () => {
    expect(usesDashScopeEnableThinking("alibaba-coding-plan", "qwen3.6-plus")).toBe(true);
    expect(usesDashScopeEnableThinking("alibaba-coding-plan", "glm-5")).toBe(true);
  });
});
