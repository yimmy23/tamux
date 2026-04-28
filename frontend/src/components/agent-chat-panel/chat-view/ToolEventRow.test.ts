import { describe, expect, it } from "vitest";

import { toolStatusTone } from "./toolStatusTone";

describe("toolStatusTone", () => {
  it("uses success for done tools, warning for requested/executing tools, and danger for errors", () => {
    expect(toolStatusTone("done").text).toBe("var(--success)");
    expect(toolStatusTone("requested").text).toBe("var(--warning)");
    expect(toolStatusTone("executing").text).toBe("var(--warning)");
    expect(toolStatusTone("error").text).toBe("var(--danger)");
  });
});
