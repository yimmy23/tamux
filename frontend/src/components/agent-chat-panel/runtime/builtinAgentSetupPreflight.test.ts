import { describe, expect, it } from "vitest";
import type { SubAgentDefinition } from "@/lib/agentStore/types";
import { builtinAgentSetupCandidate, isBuiltinPersonaSetupError } from "./builtinAgentSetupPreflight";

function subAgent(overrides: Partial<SubAgentDefinition> & Pick<SubAgentDefinition, "id">): SubAgentDefinition {
  return {
    id: overrides.id,
    name: overrides.name ?? overrides.id,
    provider: overrides.provider ?? "openai",
    model: overrides.model ?? "gpt-5.4",
    enabled: overrides.enabled ?? true,
    builtin: overrides.builtin ?? false,
    created_at: overrides.created_at ?? 0,
  };
}

describe("builtinAgentSetupCandidate", () => {
  it("opens setup for known builtin personas before sending async directives", () => {
    expect(builtinAgentSetupCandidate("!mokosh".slice(1), [])).toEqual({
      targetAgentId: "mokosh",
      targetAgentName: "Mokosh",
    });
  });

  it("opens setup for builtin subagents missing provider or model", () => {
    expect(builtinAgentSetupCandidate("Mokosh", [
      subAgent({ id: "mokosh", name: "Mokosh", provider: "", model: "", builtin: true }),
    ])).toEqual({
      targetAgentId: "mokosh",
      targetAgentName: "Mokosh",
    });
  });

  it("does not block configured builtin or custom subagents", () => {
    expect(builtinAgentSetupCandidate("mokosh", [
      subAgent({ id: "mokosh", provider: "openai", model: "gpt-5.4", builtin: true }),
    ])).toBeNull();
    expect(builtinAgentSetupCandidate("qa", [
      subAgent({ id: "qa", provider: "", model: "", builtin: false }),
    ])).toBeNull();
  });

  it("recognizes daemon setup errors as a fallback", () => {
    expect(isBuiltinPersonaSetupError("builtin agent 'mokosh' is not configured", "mokosh")).toBe(true);
  });
});
