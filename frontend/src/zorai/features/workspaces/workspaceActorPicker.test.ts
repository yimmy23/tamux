import { describe, expect, it } from "vitest";
import type { SubAgentDefinition } from "@/lib/agentStore/types";
import { workspaceActorPickerOptions } from "./workspaceActorPicker";

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

describe("workspaceActorPickerOptions", () => {
  it("mirrors the TUI assignee picker", () => {
    const options = workspaceActorPickerOptions("assignee", [
      subAgent({ id: "qa", name: "QA" }),
      subAgent({ id: "off", name: "Off", enabled: false }),
    ]);

    expect(options.map((option) => option.label).slice(0, 4)).toEqual(["none", "svarog", "Weles", "Swarozyc"]);
    expect(options.map((option) => option.label)).toContain("Mokosh");
    expect(options.map((option) => option.label)).toContain("QA");
    expect(options.map((option) => option.label)).not.toContain("user");
    expect(options.map((option) => option.label)).not.toContain("Off");
  });

  it("mirrors the TUI reviewer picker", () => {
    const options = workspaceActorPickerOptions("reviewer", []);

    expect(options.map((option) => option.label).slice(0, 3)).toEqual(["none", "user", "svarog"]);
    expect(options).toContainEqual(expect.objectContaining({
      label: "Mokosh",
      value: "subagent:mokosh",
    }));
  });

  it("marks builtin personas without runtime settings for provider/model setup", () => {
    const options = workspaceActorPickerOptions("reviewer", [
      subAgent({ id: "mokosh", name: "Mokosh", builtin: true, provider: "", model: "" }),
      subAgent({ id: "qa", name: "QA", provider: "", model: "", builtin: false }),
    ]);

    expect(options.find((option) => option.value === "subagent:mokosh")?.requiresRuntimeSetup).toBe(true);
    expect(options.find((option) => option.value === "subagent:qa")?.requiresRuntimeSetup).toBe(false);
  });
});
