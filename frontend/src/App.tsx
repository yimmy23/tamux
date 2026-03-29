import { Suspense, lazy, useEffect, useMemo } from "react";
import { getBridge } from "@/lib/bridge";
import { LayoutContainer } from "./components/LayoutContainer";
import { SurfaceTabBar } from "./components/SurfaceTabBar";
import { StatusBar } from "./components/StatusBar";
import { Sidebar } from "./components/Sidebar";
import { TitleBar } from "./components/TitleBar";
import { AgentApprovalOverlay } from "./components/AgentApprovalOverlay";
// ConciergeToast is rendered inline below — no separate import needed.
import { SetupOnboardingPanel } from "./components/SetupOnboardingPanel";
import { OperatorProfileOnboardingPanel } from "./components/OperatorProfileOnboardingPanel";
import { useAgentMissionStore } from "./lib/agentMissionStore";
import { useAgentStore } from "./lib/agentStore";
import { applyAppShellTheme, getAppShellTheme } from "./lib/themes";
import { useSettingsStore } from "./lib/settingsStore";
import { useWorkspaceStore } from "./lib/workspaceStore";
import { useHotkeys } from "./hooks/useHotkeys";
import { saveSession, startAutoSave } from "./lib/sessionPersistence";
import { ConciergeToast } from "./components/ConciergeToast";
import { useNotificationStore } from "./lib/notificationStore";
import { useAuditStore } from "./lib/auditStore";
import { useTierStore, type CapabilityTier } from "./lib/tierStore";

const CommandPalette = lazy(() => import("./components/CommandPalette").then((module) => ({ default: module.CommandPalette })));
const NotificationPanel = lazy(() => import("./components/NotificationPanel").then((module) => ({ default: module.NotificationPanel })));
const SettingsPanel = lazy(() => import("./components/SettingsPanel").then((module) => ({ default: module.SettingsPanel })));
const SessionVaultPanel = lazy(() => import("./components/SessionVaultPanel").then((module) => ({ default: module.SessionVaultPanel })));
const CommandLogPanel = lazy(() => import("./components/CommandLogPanel").then((module) => ({ default: module.CommandLogPanel })));
const CommandHistoryPicker = lazy(() => import("./components/CommandHistoryPicker").then((module) => ({ default: module.CommandHistoryPicker })));
const SearchOverlay = lazy(() => import("./components/SearchOverlay").then((module) => ({ default: module.SearchOverlay })));
const AgentChatPanel = lazy(() => import("./components/AgentChatPanel").then((module) => ({ default: module.AgentChatPanel })));
const SnippetPicker = lazy(() => import("./components/SnippetPicker").then((module) => ({ default: module.SnippetPicker })));
const SystemMonitorPanel = lazy(() => import("./components/SystemMonitorPanel").then((module) => ({ default: module.SystemMonitorPanel })));
const FileManagerPanel = lazy(() => import("./components/FileManagerPanel").then((module) => ({ default: module.FileManagerPanel })));
const TimeTravelSlider = lazy(() => import("./components/TimeTravelSlider").then((module) => ({ default: module.TimeTravelSlider })));
const ExecutionCanvas = lazy(() => import("./components/ExecutionCanvas").then((module) => ({ default: module.ExecutionCanvas })));
const AuditPanel = lazy(() => import("./components/audit-panel/AuditPanel").then((module) => ({ default: module.AuditPanel })));

export default function App() {
  const createWorkspace = useWorkspaceStore((s) => s.createWorkspace);
  const createSurface = useWorkspaceStore((s) => s.createSurface);
  const splitActive = useWorkspaceStore((s) => s.splitActive);
  const toggleZoom = useWorkspaceStore((s) => s.toggleZoom);
  const toggleSidebar = useWorkspaceStore((s) => s.toggleSidebar);
  const toggleSettings = useWorkspaceStore((s) => s.toggleSettings);
  const toggleSearch = useWorkspaceStore((s) => s.toggleSearch);
  const toggleFileManager = useWorkspaceStore((s) => s.toggleFileManager);
  const toggleCommandPalette = useWorkspaceStore((s) => s.toggleCommandPalette);
  const toggleCommandHistory = useWorkspaceStore((s) => s.toggleCommandHistory);
  const toggleCommandLog = useWorkspaceStore((s) => s.toggleCommandLog);
  const toggleSystemMonitor = useWorkspaceStore((s) => s.toggleSystemMonitor);
  const toggleCanvas = useWorkspaceStore((s) => s.toggleCanvas);
  const toggleTimeTravel = useWorkspaceStore((s) => s.toggleTimeTravel);
  const workspaces = useWorkspaceStore((s) => s.workspaces);
  const sidebarVisible = useWorkspaceStore((s) => s.sidebarVisible);
  const sidebarWidth = useWorkspaceStore((s) => s.sidebarWidth);
  const agentPanelOpen = useWorkspaceStore((s) => s.agentPanelOpen);
  const commandPaletteOpen = useWorkspaceStore((s) => s.commandPaletteOpen);
  const notificationPanelOpen = useWorkspaceStore((s) => s.notificationPanelOpen);
  const settingsOpen = useWorkspaceStore((s) => s.settingsOpen);
  const sessionVaultOpen = useWorkspaceStore((s) => s.sessionVaultOpen);
  const commandLogOpen = useWorkspaceStore((s) => s.commandLogOpen);
  const commandHistoryOpen = useWorkspaceStore((s) => s.commandHistoryOpen);
  const searchOpen = useWorkspaceStore((s) => s.searchOpen);
  const snippetPickerOpen = useWorkspaceStore((s) => s.snippetPickerOpen);
  const systemMonitorOpen = useWorkspaceStore((s) => s.systemMonitorOpen);
  const fileManagerOpen = useWorkspaceStore((s) => s.fileManagerOpen);
  const canvasOpen = useWorkspaceStore((s) => s.canvasOpen);
  const timeTravelOpen = useWorkspaceStore((s) => s.timeTravelOpen);
  const settings = useSettingsStore((s) => s.settings);
  const activeWorkspace = useWorkspaceStore((s) => s.activeWorkspace());
  const activeSurface = useWorkspaceStore((s) => s.activeSurface());
  const agentSettings = useAgentStore((s) => s.agentSettings);
  const active_provider = agentSettings.active_provider;
  const cognitiveEvents = useAgentMissionStore((s) => s.cognitiveEvents);
  const operationalEvents = useAgentMissionStore((s) => s.operationalEvents);
  const approvals = useAgentMissionStore((s) => s.approvals);
  const snapshots = useAgentMissionStore((s) => s.snapshots);
  const historyHits = useAgentMissionStore((s) => s.historyHits);
  const symbolHits = useAgentMissionStore((s) => s.symbolHits);
  const toggleAgentPanel = useWorkspaceStore((s) => s.toggleAgentPanel);
  const toggleSessionVault = useWorkspaceStore((s) => s.toggleSessionVault);
  const auditPanelOpen = useAuditStore((s) => s.isOpen);
  const traceCount = cognitiveEvents.length;
  const opsCount = operationalEvents.length;
  const snapshotCount = snapshots.length;
  const historyHitsCount = historyHits.length;
  const symbolHitsCount = symbolHits.length;
  const approvalCount = useMemo(
    () => approvals.filter((entry) => entry.status === "pending").length,
    [approvals],
  );

  useHotkeys();

  useEffect(() => {
    if (workspaces.length === 0) {
      createWorkspace("Default");
    }
  }, []);

  useEffect(() => startAutoSave(30_000), []);

  // Concierge: listen for welcome events and request one on mount.
  // This runs in App (always mounted) because runtime.tsx (chat panel)
  // may not be open when the app loads.
  useEffect(() => {
    const amux = getBridge();
    if (!amux?.onAgentEvent) {
      console.warn("[concierge] no onAgentEvent bridge available");
      return;
    }

    console.log("[concierge] setting up agent event listener in App.tsx");
    const applyConciergeWelcome = (event: any) => {
      if (event?.type !== "concierge_welcome") return;
      useAgentStore.setState({
        conciergeWelcome: {
          content: event.content ?? "",
          actions: event.actions ?? [],
        },
      });
    };

    // Listen for the concierge_welcome event from the daemon.
    const unsubscribe = amux.onAgentEvent((event: any) => {
      console.log("[concierge] agent event received:", event?.type, event);
      if (event?.type === "concierge_welcome") {
        console.log("[concierge] ConciergeWelcome event! content length:", event.content?.length, "actions:", event.actions?.length);
        applyConciergeWelcome(event);
        void useAgentStore.getState().maybeStartOperatorProfileOnboarding();
      }
      if (event?.type === "operator-profile-session-started") {
        useAgentStore.getState().applyOperatorProfileSessionStarted(event.data ?? event);
      }
      if (event?.type === "operator-profile-question") {
        useAgentStore.getState().applyOperatorProfileQuestion(event.data ?? event);
      }
      if (event?.type === "operator-profile-progress") {
        useAgentStore.getState().applyOperatorProfileProgress(event.data ?? event);
      }
      if (event?.type === "operator-profile-session-completed") {
        useAgentStore.getState().applyOperatorProfileSessionCompleted(event.data ?? event);
      }
      if (event?.type === "operator-profile-summary") {
        useAgentStore.getState().getOperatorProfileSummary().catch(() => {});
      }
      if (event?.type === "heartbeat_digest" && event.actionable === true) {
        const items = Array.isArray(event.items) ? event.items : [];
        if (items.length > 0) {
          const body = items
            .sort((a: any, b: any) => (a.priority ?? 99) - (b.priority ?? 99))
            .map(
              (item: any, i: number) =>
                `[${i + 1}] ${item.title ?? "Unknown"}${item.suggestion ? " \u2014 " + item.suggestion : ""}`,
            )
            .join("\n");
          // Per D-01: render explanation inline beneath the heartbeat action
          const explanation = typeof event.explanation === "string" ? event.explanation : "";
          useNotificationStore.getState().addNotification({
            title: event.digest || "Heartbeat: items need attention",
            body: explanation ? body + "\n" + explanation : body,
            source: "heartbeat",
          });
        }
      }
      // Gateway status events (Phase 8 - Gateway Completion)
      if (event?.type === "gateway_status") {
        useAgentStore.getState().setGatewayStatus(
          event.platform ?? "",
          event.status ?? "disconnected",
          event.last_error ?? undefined,
          event.consecutive_failures ?? undefined,
        );
      }
      // Audit event handlers (Phase 3 - Transparent Autonomy)
      if (event?.type === "audit_action") {
        useAuditStore.getState().addEntry({
          id: event.id ?? "",
          timestamp: event.timestamp ?? Date.now(),
          actionType: event.action_type ?? "heartbeat",
          summary: event.summary ?? "",
          explanation: event.explanation ?? null,
          confidence: event.confidence ?? null,
          confidenceBand: event.confidence_band ?? null,
          causalTraceId: event.causal_trace_id ?? null,
          threadId: event.thread_id ?? null,
        });
      }
      if (event?.type === "escalation_update") {
        useAuditStore.getState().setEscalation({
          threadId: event.thread_id ?? "",
          fromLevel: event.from_level ?? "L0",
          toLevel: event.to_level ?? "L1",
          reason: event.reason ?? "",
          attempts: event.attempts ?? 0,
          auditId: event.audit_id ?? null,
        });
      }
      // Tier changed events (Phase 10 - Progressive UX)
      if (event?.type === "tier_changed" || event?.type === "tier-changed") {
        const data = event.data ?? event;
        const newTier = (data.new_tier ?? data.newTier) as string | undefined;
        const validTiers: CapabilityTier[] = ["newcomer", "familiar", "power_user", "expert"];
        if (newTier && validTiers.includes(newTier as CapabilityTier)) {
          useTierStore.getState().setTier(newTier as CapabilityTier);
        }
      }
    });

    void useAgentStore.getState().refreshConciergeConfig?.();

    const requestWelcome = async () => {
      const profileState = useAgentStore.getState().operatorProfile;
      if (profileState.sessionId || profileState.question || profileState.panelOpen) {
        return;
      }
      if (!amux.agentRequestConciergeWelcome) {
        console.warn("[concierge] agentRequestConciergeWelcome not available on bridge");
        return;
      }
      console.log("[concierge] sending agentRequestConciergeWelcome");
      await amux.agentRequestConciergeWelcome().catch((e: any) => {
        console.error("[concierge] request failed:", e);
      });
    };

    const timer = setTimeout(requestWelcome, 250);

    return () => {
      clearTimeout(timer);
      if (typeof unsubscribe === "function") unsubscribe();
    };
  }, []);

  // Ctrl+Shift+A toggles the Audit Feed panel
  useEffect(() => {
    const handleAuditShortcut = (e: KeyboardEvent) => {
      if (e.ctrlKey && e.shiftKey && e.key === "A") {
        e.preventDefault();
        useAuditStore.getState().togglePanel();
      }
    };
    window.addEventListener("keydown", handleAuditShortcut);
    return () => window.removeEventListener("keydown", handleAuditShortcut);
  }, []);

  useEffect(() => {
    const timeoutId = window.setTimeout(() => {
      saveSession();
    }, 500);

    return () => window.clearTimeout(timeoutId);
  }, [workspaces, sidebarVisible, sidebarWidth]);

  useEffect(() => {
    applyAppShellTheme(
      getAppShellTheme(
        settings.themeName,
        settings.useCustomTerminalColors,
        settings.customTerminalBackground,
        settings.customTerminalForeground,
        settings.customTerminalCursor,
        settings.customTerminalSelection,
      )
    );

    const amux = getBridge();
    void amux?.setWindowOpacity?.(settings.opacity);
  }, [
    settings.themeName,
    settings.useCustomTerminalColors,
    settings.customTerminalBackground,
    settings.customTerminalForeground,
    settings.customTerminalCursor,
    settings.customTerminalSelection,
    settings.opacity,
  ]);

  useEffect(() => {
    const amux = getBridge();
    if (!amux?.onAppCommand) return;

    return amux.onAppCommand((command: string) => {
      switch (command) {
        case "new-workspace":
          createWorkspace();
          break;
        case "new-surface":
          createSurface();
          break;
        case "toggle-settings":
          toggleSettings();
          break;
        case "toggle-command-palette":
          toggleCommandPalette();
          break;
        case "toggle-search":
          toggleSearch();
          break;
        case "toggle-file-manager":
          toggleFileManager();
          break;
        case "toggle-mission":
          toggleAgentPanel();
          break;
        case "toggle-command-history":
          toggleCommandHistory();
          break;
        case "toggle-command-log":
          toggleCommandLog();
          break;
        case "toggle-session-vault":
          toggleSessionVault();
          break;
        case "toggle-system-monitor":
          toggleSystemMonitor();
          break;
        case "toggle-canvas":
          toggleCanvas();
          break;
        case "toggle-time-travel":
          toggleTimeTravel();
          break;
        case "toggle-sidebar":
          toggleSidebar();
          break;
        case "split-right":
          splitActive("horizontal");
          break;
        case "split-down":
          splitActive("vertical");
          break;
        case "toggle-zoom":
          toggleZoom();
          break;
        case "about":
          if (!settingsOpen) {
            toggleSettings();
          }
          window.setTimeout(() => {
            window.dispatchEvent(new CustomEvent("tamux-open-settings-tab", {
              detail: { tab: "about" },
            }));
            window.dispatchEvent(new CustomEvent("amux-open-settings-tab", {
              detail: { tab: "about" },
            }));
          }, 50);
          break;
      }
    });
  }, [
    createWorkspace,
    createSurface,
    settingsOpen,
    splitActive,
    toggleAgentPanel,
    toggleCanvas,
    toggleCommandHistory,
    toggleCommandLog,
    toggleCommandPalette,
    toggleFileManager,
    toggleSearch,
    toggleSessionVault,
    toggleSettings,
    toggleSidebar,
    toggleSystemMonitor,
    toggleTimeTravel,
    toggleZoom,
  ]);

  return (
    <div
      style={{
        display: "flex",
        flexDirection: "column",
        width: "100%",
        height: "100%",
        background: "var(--bg-void)",
        overflow: "hidden",
      }}
    >
      <TitleBar />

      <div style={{ flex: 1, display: "flex", flexDirection: "column", minHeight: 0, gap: 0, padding: 0 }}>
        <MissionDeck
          workspaceName={activeWorkspace?.name ?? "No workspace"}
          surfaceName={activeSurface?.name ?? "No surface"}
          active_provider={active_provider}
          traceCount={traceCount}
          opsCount={opsCount}
          approvalCount={approvalCount}
          snapshotCount={snapshotCount}
          historyHitsCount={historyHitsCount}
          symbolHitsCount={symbolHitsCount}
          onOpenMission={toggleAgentPanel}
          onOpenVault={toggleSessionVault}
        />

        <SurfaceTabBar />

        <div style={{ flex: 1, display: "flex", overflow: "hidden", gap: 0, minHeight: 0, minWidth: 0 }}>
          {sidebarVisible && <Sidebar />}

          <div
            style={{
              flex: 1,
              display: "flex",
              flexDirection: "column",
              overflow: "hidden",
              minWidth: 0,
              minHeight: 0,
            }}
            className="amux-shell-card"
          >
            <LayoutContainer />

            <Suspense fallback={null}>
              {searchOpen && <SearchOverlay />}
              {timeTravelOpen && <TimeTravelSlider />}
            </Suspense>
          </div>

          <Suspense fallback={null}>
            {agentPanelOpen && <AgentChatPanel />}
            {settingsOpen && <SettingsPanel />}
            {sessionVaultOpen && <SessionVaultPanel />}
            {commandLogOpen && <CommandLogPanel />}
            {systemMonitorOpen && <SystemMonitorPanel />}
            {fileManagerOpen && <FileManagerPanel />}
          </Suspense>
        </div>

        <StatusBar />
      </div>

      <Suspense fallback={null}>
        {commandPaletteOpen && <CommandPalette />}
        {notificationPanelOpen && <NotificationPanel />}
        {auditPanelOpen && <AuditPanel />}
        {commandHistoryOpen && <CommandHistoryPicker />}
        {snippetPickerOpen && <SnippetPicker />}
        {canvasOpen && <ExecutionCanvas />}
      </Suspense>

      <SetupOnboardingPanel />
      <OperatorProfileOnboardingPanel />
      <AgentApprovalOverlay />
      <ConciergeToast />
    </div>
  );
}

function MissionDeck({
  workspaceName,
  surfaceName,
  active_provider,
  traceCount,
  opsCount,
  approvalCount,
  snapshotCount,
  historyHitsCount,
  symbolHitsCount,
  onOpenMission,
  onOpenVault,
}: {
  workspaceName: string;
  surfaceName: string;
  active_provider: string;
  traceCount: number;
  opsCount: number;
  approvalCount: number;
  snapshotCount: number;
  historyHitsCount: number;
  symbolHitsCount: number;
  onOpenMission: () => void;
  onOpenVault: () => void;
}) {
  const providerText = typeof active_provider === "string" && active_provider.trim().length > 0
    ? active_provider
    : "unknown";

  return (
    <div
      className="amux-shell-card"
      style={{
        flexShrink: 0,
        padding: "6px 10px",
        minHeight: 52,
        display: "flex",
        alignItems: "center",
        justifyContent: "space-between",
        gap: "var(--space-2)",
        overflowX: "auto",
      }}
    >
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: "var(--space-2)",
          minWidth: 0,
        }}
      >
        <span className="amux-agent-indicator" style={{ fontSize: 10, padding: "2px 8px" }}>Mission</span>
        <span
          style={{
            fontSize: "var(--text-sm)",
            fontWeight: 600,
            whiteSpace: "nowrap",
            overflow: "hidden",
            textOverflow: "ellipsis",
            maxWidth: 240,
          }}
          title={`${workspaceName} - ${surfaceName}`}
        >
          {workspaceName}
        </span>
        <span style={{ color: "var(--text-muted)", fontSize: "var(--text-xs)", whiteSpace: "nowrap" }}>
          {surfaceName}
        </span>
        <span className="amux-chip" style={{ fontSize: 10, padding: "2px 6px" }}>
          provider {providerText}
        </span>
      </div>

      <div style={{ display: "flex", alignItems: "center", gap: "var(--space-2)", whiteSpace: "nowrap" }}>
        <span className="amux-chip amux-chip--approval" style={{ fontSize: 10, padding: "2px 6px" }}>
          approvals {approvalCount}
        </span>
        <span className="amux-chip" style={{ fontSize: 10, padding: "2px 6px", color: "var(--reasoning)" }}>
          trace {traceCount}
        </span>
        <span className="amux-chip" style={{ fontSize: 10, padding: "2px 6px", color: "var(--agent)" }}>
          ops {opsCount}
        </span>
        <span className="amux-chip" style={{ fontSize: 10, padding: "2px 6px", color: "var(--timeline)" }}>
          recall {historyHitsCount + symbolHitsCount}
        </span>
        <span className="amux-chip" style={{ fontSize: 10, padding: "2px 6px" }}>
          snapshots {snapshotCount}
        </span>
      </div>

      <div style={{ display: "flex", alignItems: "center", gap: "var(--space-1)", whiteSpace: "nowrap" }}>
        <button
          type="button"
          onClick={onOpenMission}
          style={{
            padding: "4px 8px",
            border: "1px solid var(--accent-soft)",
            background: "var(--accent-soft)",
            color: "var(--accent)",
            fontSize: 11,
            fontWeight: 500,
            cursor: "pointer",
          }}
        >
          Mission
        </button>
        <button
          type="button"
          onClick={onOpenVault}
          style={{
            padding: "4px 8px",
            border: "1px solid var(--border)",
            background: "transparent",
            color: "var(--text-secondary)",
            fontSize: 11,
            fontWeight: 500,
            cursor: "pointer",
          }}
        >
          Vault
        </button>
      </div>
    </div>
  );
}
