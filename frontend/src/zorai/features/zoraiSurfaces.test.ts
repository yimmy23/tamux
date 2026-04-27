import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

function readFeature(path: string): string {
  return readFileSync(new URL(path, import.meta.url), "utf8");
}

describe("Zorai feature surfaces", () => {
  it("keeps Goals native to the Zorai shell instead of embedding legacy task UI", () => {
    const source = readFeature("./goals/GoalsView.tsx");

    expect(source).not.toContain("TasksView");
    expect(source).toContain("zorai-goals-surface");
  });

  it("keeps Activity native to the Zorai shell instead of embedding legacy trace UI", () => {
    const source = readFeature("./activity/ActivityView.tsx");

    expect(source).not.toContain("TraceView");
    expect(source).toContain("zorai-activity-surface");
  });

  it("keeps Settings native to the Zorai shell instead of embedding the old settings panel", () => {
    const source = readFeature("./settings/SettingsView.tsx");
    const panelSource = readFeature("./settings/SettingsPanels.tsx");

    expect(source).not.toContain("components/SettingsPanel");
    expect(panelSource).toContain("zorai-settings-grid");
  });

  it("keeps Settings scrollable inside the Zorai shell", () => {
    const shellCss = readFeature("../styles/zorai.css");
    const surfaceCss = readFeature("../styles/zorai-surfaces.css");

    expect(shellCss).toMatch(/\.zorai-main\s*{[^}]*min-height:\s*0/s);
    expect(shellCss).toMatch(/\.zorai-main\s*{[^}]*overflow:\s*hidden/s);
    expect(surfaceCss).toMatch(/\.zorai-settings-surface\s*{[^}]*overflow:\s*auto/s);
  });

  it("keeps Tools navigation in the rail and actions in the main surface", () => {
    const source = readFeature("./tools/ToolsView.tsx");

    expect(source).not.toContain("zorai-tool-picker");
    expect(source).toContain("New terminal surface");
    expect(source).toContain("Split right");
    expect(source).toContain("New infinite canvas");
  });

  it("keeps Threads native to the Zorai shell instead of embedding the old chat view", () => {
    const source = readFeature("./threads/ThreadsView.tsx");

    expect(source).not.toContain("ChatView");
    expect(source).toContain("zorai-native-thread-surface");
    expect(source).toContain("zorai-thread-composer");
  });
});
