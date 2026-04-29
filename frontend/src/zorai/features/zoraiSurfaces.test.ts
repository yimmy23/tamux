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
    const source = readFeature("./goals/goalWorkspaceModel.ts");
    const panelSource = readFeature("./goals/GoalWorkspacePanel.tsx");

    expect(source).toContain("Dossier");
    expect(source).toContain("Files");
    expect(source).toContain("Progress");
    expect(source).toContain("Usage");
    expect(source).toContain("Active agent");
    expect(source).toContain("Threads");
    expect(source).toContain("Needs attention");
    expect(source).toContain("targetThreadId");
    expect(source).toContain("targetFilePath");
    expect(panelSource).toContain("loadGoalProjectionFiles");
    expect(panelSource).toContain("openThreadFilePreview");
  });

  it("enters a dedicated TUI-style goal view from mission control", () => {
    const source = readFeature("./goals/GoalsView.tsx");

    expect(source).toContain("workspaceOpen");
    expect(source).toContain("Open goal view");
    expect(source).toContain("Back to goals");
  });

  it("starts goals through the TUI-compatible Mission Control preflight", () => {
    const source = readFeature("./goals/GoalsView.tsx");
    const launchSource = readFeature("./goals/GoalLaunchPanel.tsx");
    const goalRunsSource = readFeature("../../lib/goalRuns.ts");
    const electronSource = readFileSync(new URL("../../../electron/main/agent-ipc-handlers.cjs", import.meta.url), "utf8");

    expect(source).toContain("GoalLaunchPanel");
    expect(source).toContain("goal-launch-overlay");
    expect(source).toContain("setLaunchOpen(true)");
    expect(source).not.toContain("Optional goal title");
    expect(launchSource).not.toContain("MISSION CONTROL");
    expect(launchSource).not.toContain("Prompt editor");
    expect(launchSource).not.toContain("Ctrl+O");
    expect(launchSource).not.toContain("Esc cancel");
    expect(launchSource).not.toContain("Thread Router");
    expect(launchSource).not.toContain("zorai-tui-pane");
    expect(launchSource).toContain("Goal prompt");
    expect(launchSource).toContain("Main Agent");
    expect(launchSource).toContain("Role Assignments");
    expect(launchSource).toContain("launchAssignments");
    expect(launchSource).toContain("onClose");
    expect(goalRunsSource).toContain("launchAssignments?: GoalAgentAssignment[]");
    expect(electronSource).toContain("launch_assignments");
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
    expect(usageSource).toContain("agentGetStatistics");
    expect(usageSource).toContain("Overview");
    expect(usageSource).toContain("Providers");
    expect(usageSource).toContain("Models");
    expect(usageSource).toContain("Rankings");
    expect(usageSource).toContain("Provider / Model");
    expect(usageSource).toContain("Top Models By Tokens");
    expect(usageSource).toContain("SessionUsageTable");
    expect(usageSource).toContain("stats.sessionRows");
    expect(usageSource).toContain("Provider models");
    expect(surfaceCss).toContain("zorai-usage-grid");
  });

  it("keeps approval requests in native Zorai modal styling", () => {
    const source = readFeature("../../components/AgentApprovalOverlay.tsx");

    expect(source).toContain("zorai-approval-overlay");
    expect(source).toContain("zorai-approval-dialog");
    expect(source).not.toContain("zorai-panel-title");
    expect(source).not.toContain("onMouseEnter");
  });

  it("keeps Settings native to the Zorai shell instead of embedding the old settings panel", () => {
    const source = readFeature("./settings/SettingsView.tsx");
    const panelSource = readFeature("./settings/SettingsPanels.tsx");
    const tabSource = readFeature("./settings/settingsTabs.ts");

    expect(source).not.toContain("components/SettingsPanel");
    expect(source).toContain("refreshAgentSettingsFromDaemon");
    expect(source).toContain("buildDaemonAgentConfig");
    expect(source).toContain("diffDaemonConfigEntries");
    expect(panelSource).toContain("zorai-settings-grid");
    expect(tabSource).toContain('title: "Svarog"');
    expect(tabSource).toContain('title: "Rarog"');
    expect(tabSource).toContain('title: "Chat"');
    expect(tabSource).toContain('id: "search"');
    expect(tabSource).toContain('title: "Terminal interface"');
    expect(panelSource).toContain("API Key");
    expect(panelSource).toContain("Logout");
    expect(panelSource).toContain("Svarog Provider");
    expect(panelSource).toContain("getSupportedApiTransports");
    expect(panelSource).toContain("normalizeApiTransport");
    expect(panelSource).toContain("activeProviderConfig.custom_model_name");
    expect(panelSource).not.toContain('label="Auth" description="Credential source.');
    expect(panelSource).toContain("Backend");
    expect(panelSource).toContain("daemon");
    expect(panelSource).not.toContain("OpenClaw");
    expect(panelSource).not.toContain("Hermes");
    expect(panelSource).toContain("selectedConciergeProvider");
    expect(panelSource).toContain("proactive_triage");
    expect(panelSource).toContain("(use Svarog)");
    expect(panelSource).toContain("managed_security_level");
    expect(panelSource).toContain("compaction.strategy");
    expect(panelSource).toContain("Compaction Strategy Settings");
    expect(panelSource).toContain("zorai-settings-grid--full");
    expect(panelSource).toContain("Version");
    expect(panelSource).toContain("Author");
    expect(panelSource).toContain("GitHub");
    expect(panelSource).toContain("Homepage");
    expect(panelSource).toContain("Web Search");
    expect(panelSource).toContain("SubAgentsTab");
    expect(panelSource).toContain("selectPlugin");
    expect(panelSource).toContain("pluginUpdateSettings");
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
    const shellSource = readFeature("../shell/ZoraiShell.tsx");

    expect(source).not.toContain("zorai-tool-picker");
    expect(source).toContain("New terminal surface");
    expect(source).toContain("Split right");
    expect(source).toContain("New infinite canvas");
    expect(source).toContain("LayoutContainer");
    expect(source).toContain("ToolsContext");
    expect(source).toContain("closeWorkspace");
    expect(source).toContain("Remove workspace");
    expect(shellSource).toContain("ToolsContext");
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
    expect(source).toContain("zorai-workspace-modal");
    expect(source).toContain("updateWorkspaceTask");
    expect(source).toContain("openThreadTarget");
    expect(source).toContain("goalRunId");
    expect(source).toContain("WorkspaceActorPickerControl");
    expect(source).not.toContain("placeholder=\"reviewer: user, svarog\"");
  });

  it("keeps Threads native to the Zorai shell instead of embedding the old chat view", () => {
    const source = readFeature("./threads/ThreadsView.tsx");
    const css = readFeature("../styles/zorai.css");

    expect(source).not.toContain("ChatView");
    expect(source).toContain("zorai-native-thread-surface");
    expect(source).toContain("zorai-thread-composer");
    expect(css).not.toContain(".zorai-thread-surface > div");
  });

  it("opens listed threads through the daemon detail loader", () => {
    const source = readFeature("../../components/agent-chat-panel/runtime/layout.tsx");
    const browserStart = source.indexOf("function AgentChatPanelThreadBrowserSurface");
    const browserEnd = source.indexOf("export function AgentChatPanelThreadsSurface");
    const browserSource = source.slice(browserStart, browserEnd);

    expect(browserSource).toContain("openThread");
    expect(browserSource).toContain("openThread(thread.id)");
    expect(browserSource).not.toContain("setActiveThread(thread.id)");
  });

  it("shows the streaming stop action next to Send in native Threads", () => {
    const source = readFeature("./threads/ThreadsView.tsx");
    const composerStart = source.indexOf("zorai-thread-composer__footer");
    const composerEnd = source.indexOf("{pinLimitResult");
    const composerSource = source.slice(composerStart, composerEnd);

    expect(composerSource).toContain("runtime.isStreamingResponse");
    expect(composerSource).toContain("runtime.stopStreaming(runtime.activeThreadId)");
    expect(composerSource.indexOf("Stop")).toBeGreaterThan(-1);
    expect(composerSource.indexOf("Stop")).toBeLessThan(composerSource.indexOf("Send"));
  });

  it("keeps TUI-style pinned message controls in native Threads", () => {
    const source = readFeature("./threads/ThreadsView.tsx");
    const contextSource = readFeature("./threads/ThreadsContextPanel.tsx");

    expect(source).toContain("pinMessageForCompaction");
    expect(contextSource).toContain("Pinned Messages");
    expect(source).toContain("Pin Limit Reached");
  });

  it("renders thread tool calls through collapsed tool rows instead of plain message bubbles", () => {
    const source = readFeature("./threads/ThreadsView.tsx");
    const toolSource = readFeature("../../components/agent-chat-panel/chat-view/ToolEventRow.tsx");

    expect(source).toContain("buildDisplayItems");
    expect(source).toContain("ToolEventRow");
    expect(source).toContain('item.type === "tool"');
    expect(source).not.toContain("summarizeToolMessage");
    expect(toolSource).toContain("toolStatusTone");
    expect(readFeature("../../components/agent-chat-panel/chat-view/toolStatusTone.ts")).toContain("toolStatusTone");
    expect(readFeature("../../components/agent-chat-panel/chat-view/toolStatusTone.ts")).toContain("var(--success)");
    expect(readFeature("../../components/agent-chat-panel/chat-view/toolStatusTone.ts")).toContain("var(--warning)");
  });

  it("keeps thread context aligned with TUI tabs and daemon token context windows", () => {
    const shellSource = readFeature("../shell/ZoraiShell.tsx");
    const contextSource = readFeature("./threads/ThreadsContextPanel.tsx");
    const spawnedSource = readFeature("./threads/ThreadsSpawnedContext.tsx");

    expect(shellSource).toContain("ThreadsContext");
    expect(contextSource).toContain("fetchThreadWorkContext");
    expect(readFeature("./threads/ThreadFilePreviewOverlay.tsx")).toContain("fetchGitDiff");
    expect(readFeature("./threads/ThreadFilePreviewOverlay.tsx")).toContain("fetchFilePreview");
    expect(contextSource).toContain("daemonThreadId");
    expect(contextSource).toContain("Todos");
    expect(contextSource).toContain("Files");
    expect(contextSource).toContain("Spawned");
    expect(contextSource).toContain("profileContextWindowTokens");
    expect(contextSource).toContain("activeContextWindowTokens");
    expect(contextSource).toContain("tokens");
    expect(contextSource).toContain("zorai-todo-context-list");
    expect(contextSource).toContain("zorai-todo-checkbox");
    expect(contextSource).toContain("}, [daemonThreadId]);");
    expect(contextSource).not.toContain("}, [activeThread, daemonThreadId]);");
    expect(contextSource).not.toContain("SpawnedAgentsPanel");
    expect(spawnedSource).toContain("zorai-spawned-card");
    expect(spawnedSource).not.toContain("ActionButton");
  });

  it("does not duplicate the left rail inside the right context panel", () => {
    const shellSource = readFeature("../shell/ZoraiShell.tsx");
    const contextRegion = shellSource.slice(
      shellSource.indexOf("<ZoraiContextPanel"),
      shellSource.indexOf("</ZoraiContextPanel>"),
    );

    expect(shellSource).toContain("renderContext(");
    expect(shellSource).toContain("GoalsContext");
    expect(contextRegion).not.toContain("renderRail(activeView");
  });

  it("opens thread file previews as an overlay over chat instead of inside the context sidebar", () => {
    const threadSource = readFeature("./threads/ThreadsView.tsx");
    const contextSource = readFeature("./threads/ThreadsContextPanel.tsx");
    const overlaySource = readFeature("./threads/ThreadFilePreviewOverlay.tsx");
    const css = readFeature("../styles/zorai.css");

    expect(threadSource).toContain("ThreadFilePreviewOverlay");
    expect(contextSource).toContain("openThreadFilePreview");
    expect(contextSource).not.toContain("fetchFilePreview");
    expect(contextSource).not.toContain("fetchGitDiff");
    expect(contextSource).not.toContain("zorai-file-preview");
    expect(overlaySource).toContain("zorai-file-preview-overlay");
    expect(overlaySource).toContain("Close preview");
    expect(css).toMatch(/\.zorai-file-preview-overlay\s*{[^}]*position:\s*absolute/s);
  });

  it("keeps native thread message content bounded to the card width", () => {
    const css = readFeature("../styles/zorai.css");

    expect(css).toMatch(/\.zorai-native-thread-surface\s*{[^}]*grid-template-areas:/s);
    expect(css).toMatch(/\.zorai-thread-chat-scroll\s*{[^}]*grid-area:\s*messages/s);
    expect(css).toMatch(/\.zorai-thread-composer\s*{[^}]*grid-area:\s*composer/s);
    expect(css).toMatch(/\.zorai-thread-chat-scroll\s*>\s*\*\s*{[^}]*min-width:\s*0/s);
    expect(css).toMatch(/\.zorai-message\s*{[^}]*box-sizing:\s*border-box/s);
    expect(css).toMatch(/\.zorai-message__content\s*{[^}]*overflow-wrap:\s*anywhere/s);
  });

  it("keeps assistant reasoning separate from visible message content", () => {
    const source = readFeature("./threads/ThreadsView.tsx");
    const css = readFeature("../styles/zorai.css");

    expect(source).toContain("zorai-message__reasoning-toggle");
    expect(source).toContain("hasVisibleContent");
    expect(source).not.toContain("reasoningPreview");
    expect(source).not.toContain("zorai-message__content--preview");
    expect(source).not.toContain("summarizeThreadMessageText");
    expect(source).toContain("Reasoning");
    expect(source).not.toContain("open={message.isStreaming ? true : undefined}");
    expect(source).not.toContain("<p className=\"zorai-message__reasoning\">");
    expect(css).toMatch(/\.zorai-message__reasoning\s*{[^}]*border:\s*1px solid var\(--zorai-border\)/s);
    expect(css).toMatch(/\.zorai-message__reasoning\s*>\s*div\s*{[^}]*max-height:\s*min\(42vh, 360px\)/s);
  });

  it("renders system metacognition messages as collapsed rows like tool calls", () => {
    const source = readFeature("./threads/ThreadsView.tsx");

    expect(source).toContain("isMetacognitionSystemMessage");
    expect(source).toContain("Meta-cognitive intervention");
    expect(source).toContain("MetacognitionEventRow");
    expect(source).toContain("const [collapsed, setCollapsed] = useState(true)");
    expect(source).toContain("return <MetacognitionEventRow");
  });

  it("fetches latest thread pages on selection and older pages on scroll-up", () => {
    const source = readFeature("./threads/ThreadsView.tsx");
    const runtimeSource = readFeature("../../components/agent-chat-panel/runtime/useAgentChatPanelProviderValue.ts");
    const eventsSource = readFeature("../../components/agent-chat-panel/runtime/useDaemonAgentEvents.ts");

    expect(source).toContain("openThread");
    expect(source).toContain("onScroll");
    expect(source).toContain("loadOlderThreadMessages");
    expect(source).toContain("threadHistoryLabel");
    expect(source).toContain("threadTabs");
    expect(source).toContain("dateFilters");
    expect(runtimeSource).toContain("loadThreadPage");
    expect(runtimeSource).toContain("localThreadId: threadId");
    expect(runtimeSource).toContain("messageOffset");
    expect(runtimeSource).toContain("threadPageLoadChainRef");
    expect(eventsSource).toContain("resolveDaemonEventLocalThreadId");
    expect(eventsSource).toContain("event.thread_id");
  });

  it("hydrates concierge thread actions before navigating into Threads", () => {
    const source = readFeature("../../components/ConciergeToast.tsx");

    expect(source).toContain("useAgentChatPanelRuntime");
    expect(source).toContain("openThreadTarget");
    expect(source).toContain("navigateZorai");
  });
});
