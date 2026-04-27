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

  it("keeps TUI goal workspace modes in native Goals", () => {
    const source = readFeature("./goals/GoalWorkspacePanel.tsx");

    expect(source).toContain("Dossier");
    expect(source).toContain("Files");
    expect(source).toContain("Progress");
    expect(source).toContain("Usage");
    expect(source).toContain("Active agent");
    expect(source).toContain("Threads");
    expect(source).toContain("Needs attention");
  });

  it("keeps Activity native to the Zorai shell instead of embedding legacy trace UI", () => {
    const source = readFeature("./activity/ActivityView.tsx");

    expect(source).not.toContain("TraceView");
    expect(source).toContain("zorai-activity-surface");
  });

  it("exposes TUI-style usage statistics inside native Activity", () => {
    const source = readFeature("./activity/ActivityView.tsx");
    const usageSource = readFeature("./activity/ActivityUsagePanel.tsx");
    const surfaceCss = readFeature("../styles/zorai-surfaces.css");

    expect(source).toContain('"usage"');
    expect(source).toContain("UsagePanel");
    expect(usageSource).toContain("Provider / Model");
    expect(usageSource).toContain("Goal Usage");
    expect(surfaceCss).toContain("zorai-usage-grid");
  });

  it("keeps approval requests in native Zorai modal styling", () => {
    const source = readFeature("../../components/AgentApprovalOverlay.tsx");

    expect(source).toContain("zorai-approval-overlay");
    expect(source).toContain("zorai-approval-dialog");
    expect(source).not.toContain("amux-panel-title");
    expect(source).not.toContain("onMouseEnter");
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

  it("keeps Workspaces aligned to the TUI workspace board instead of old terminal workspaces", () => {
    const source = readFeature("./workspaces/WorkspacesView.tsx");

    expect(source).not.toContain("migration hints");
    expect(source).not.toContain("createSurface");
    expect(source).not.toContain("splitActive");
    expect(source).not.toContain("applyPresetLayout");
    expect(source).toContain("WorkspaceTaskStatus");
    expect(source).toContain("New task");
    expect(source).toContain("Toggle operator");
    expect(source).toContain("runWorkspaceTask");
    expect(source).toContain("moveWorkspaceTask");
  });

  it("keeps Threads native to the Zorai shell instead of embedding the old chat view", () => {
    const source = readFeature("./threads/ThreadsView.tsx");

    expect(source).not.toContain("ChatView");
    expect(source).toContain("zorai-native-thread-surface");
    expect(source).toContain("zorai-thread-composer");
  });

  it("keeps TUI-style pinned message controls in native Threads", () => {
    const source = readFeature("./threads/ThreadsView.tsx");

    expect(source).toContain("pinMessageForCompaction");
    expect(source).toContain("Pinned Compaction Context");
    expect(source).toContain("Pin Limit Reached");
  });

  it("fetches latest thread pages on selection and older pages on scroll-up", () => {
    const source = readFeature("./threads/ThreadsView.tsx");
    const runtimeSource = readFeature("../../components/agent-chat-panel/runtime/useAgentChatPanelProviderValue.ts");
    const eventsSource = readFeature("../../components/agent-chat-panel/runtime/useDaemonAgentEvents.ts");

    expect(source).toContain("openThread");
    expect(source).toContain("onScroll");
    expect(source).toContain("loadOlderThreadMessages");
    expect(source).toContain("threadHistoryLabel");
    expect(runtimeSource).toContain("loadThreadPage");
    expect(runtimeSource).toContain("localThreadId: threadId");
    expect(runtimeSource).toContain("messageOffset");
    expect(runtimeSource).toContain("threadPageLoadChainRef");
    expect(eventsSource).toContain("resolveDaemonEventLocalThreadId");
    expect(eventsSource).toContain("event.thread_id");
  });
});
