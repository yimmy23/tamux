import { describe, expect, it } from "vitest";
import { parseLeadingAgentDirective } from "./agentDirective";

describe("parseLeadingAgentDirective", () => {
  const known = ["weles", "rarog", "swarozyc"];

  it("parses internal delegation", () => {
    expect(parseLeadingAgentDirective("!weles check claim", known)).toEqual({
      kind: "internal_delegate",
      agentAlias: "weles",
      body: "check claim",
    });
  });

  it("parses participant deactivation phrases", () => {
    expect(parseLeadingAgentDirective("@weles stop", known)).toEqual({
      kind: "participant_deactivate",
      agentAlias: "weles",
    });
    expect(parseLeadingAgentDirective("@weles return", known)).toEqual({
      kind: "participant_deactivate",
      agentAlias: "weles",
    });
  });

  it("returns null for unknown leading aliases", () => {
    expect(parseLeadingAgentDirective("@unknown inspect @src/file.ts", known)).toBeNull();
  });

  it("preserves file refs in the body", () => {
    expect(parseLeadingAgentDirective("@weles inspect @src/file.ts", known)).toEqual({
      kind: "participant_upsert",
      agentAlias: "weles",
      body: "inspect @src/file.ts",
    });
  });

  it("parses builtin persona aliases beyond weles and rarog", () => {
    expect(parseLeadingAgentDirective("@swarozyc review svarog output", known)).toEqual({
      kind: "participant_upsert",
      agentAlias: "swarozyc",
      body: "review svarog output",
    });
  });
});
