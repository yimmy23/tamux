import { Suspense, lazy, useCallback, useEffect, useMemo, useRef } from "react";
import { LayoutContainer } from "./components/LayoutContainer";
import { SurfaceTabBar } from "./components/SurfaceTabBar";
import { StatusBar } from "./components/StatusBar";
import { Sidebar } from "./components/Sidebar";
import { TitleBar } from "./components/TitleBar";
import { AgentApprovalOverlay } from "./components/AgentApprovalOverlay";
// ConciergeToast is rendered inline below — no separate import needed.
import { SetupOnboardingPanel } from "./components/SetupOnboardingPanel";
import { useAgentMissionStore } from "./lib/agentMissionStore";
import { clearThreadAbortController, setThreadAbortController, useAgentStore } from "./lib/agentStore";
import type { AgentProviderConfig } from "./lib/agentStore";
import { applyAppShellTheme, getAppShellTheme } from "./lib/themes";
import { useSettingsStore } from "./lib/settingsStore";
import { useWorkspaceStore } from "./lib/workspaceStore";
import { useHotkeys } from "./hooks/useHotkeys";
import { saveSession, startAutoSave } from "./lib/sessionPersistence";
import { prepareOpenAIRequest, sendChatCompletion } from "./lib/agentClient";
import { executeTool, getAvailableTools, getToolCapabilityDescription } from "./lib/agentTools";
import { readPersistedJson, scheduleJsonWrite } from "./lib/persistence";
import { ConciergeToast } from "./components/ConciergeToast";
import { useNotificationStore } from "./lib/notificationStore";
import { useAuditStore } from "./lib/auditStore";

const GATEWAY_THREAD_MAP_FILE = "gateway-thread-map.json";

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
  const gatewayThreadMapRef = useRef<Record<string, string>>({});
  const gatewayInFlightRef = useRef<Set<string>>(new Set());

  const persistGatewayThreadMap = useCallback(() => {
    scheduleJsonWrite(GATEWAY_THREAD_MAP_FILE, gatewayThreadMapRef.current, 100);
  }, []);

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

  useEffect(() => {
    void readPersistedJson<Record<string, string>>(GATEWAY_THREAD_MAP_FILE).then((persisted) => {
      if (persisted && typeof persisted === "object") {
        gatewayThreadMapRef.current = persisted;
      }
    });
  }, []);

  // Concierge: listen for welcome events and request one on mount.
  // This runs in App (always mounted) because runtime.tsx (chat panel)
  // may not be open when the app loads.
  useEffect(() => {
    const amux = (window as any).tamux ?? (window as any).amux;
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
    });

    void useAgentStore.getState().refreshConciergeConfig?.();

    const requestWelcome = () => {
      if (!amux.agentRequestConciergeWelcome) {
        console.warn("[concierge] agentRequestConciergeWelcome not available on bridge");
        return;
      }
      console.log("[concierge] sending agentRequestConciergeWelcome");
      amux.agentRequestConciergeWelcome().catch((e: any) => {
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

    const amux = (window as any).tamux ?? (window as any).amux;
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
    const amux = (window as any).tamux ?? (window as any).amux;
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

  const handleInboundGatewayMessage = useCallback(async (params: {
    provider: "discord" | "whatsapp" | "slack" | "telegram";
    channelId: string;
    userId: string;
    username: string;
    text: string;
    replyTarget: string;
  }) => {
    const amux = (window as any).tamux ?? (window as any).amux;
    const gatewaySettings = useAgentStore.getState().agentSettings;
    const agentState = useAgentStore.getState();

    if (!gatewaySettings.gateway_enabled) return;

    const provider = params.provider;
    const channelId = params.channelId.trim();
    const userId = params.userId.trim();
    const username = params.username.trim() || `${provider}-user`;
    const rawContent = params.text;

    if (!channelId) return;

    if (provider === "discord") {
      if (!gatewaySettings.discord_token) return;

      const allowedChannels = gatewaySettings.discord_channel_filter
        .split(",")
        .map((entry) => entry.trim())
        .filter(Boolean)
        .map((entry) => (entry.match(/\d{17,20}/)?.[0] ?? entry));

      if (allowedChannels.length > 0 && !allowedChannels.includes(channelId)) {
        return;
      }

      const allowedUsers = gatewaySettings.discord_allowed_users
        .split(",")
        .map((entry) => entry.trim())
        .filter(Boolean)
        .map((entry) => (entry.match(/\d{17,20}/)?.[0] ?? entry));

      if (allowedUsers.length > 0 && userId && !allowedUsers.includes(userId)) {
        return;
      }
    }

    if (provider === "slack") {
      if (!gatewaySettings.slack_token) return;

      const allowedChannels = gatewaySettings.slack_channel_filter
        .split(",")
        .map((entry) => entry.trim())
        .filter(Boolean)
        .map((entry) => entry.toLowerCase());

      if (allowedChannels.length > 0) {
        const normalizedChannelId = channelId.toLowerCase();
        const normalizedReplyTarget = params.replyTarget.toLowerCase();
        const allowed = allowedChannels.some((entry) => (
          entry === normalizedChannelId || entry === normalizedReplyTarget
        ));
        if (!allowed) return;
      }
    }

    if (provider === "telegram") {
      if (!gatewaySettings.telegram_token) return;

      const allowedChats = gatewaySettings.telegram_allowed_chats
        .split(",")
        .map((entry) => entry.trim())
        .filter(Boolean);
      if (allowedChats.length > 0 && !allowedChats.includes(channelId)) {
        return;
      }
    }

    if (provider === "whatsapp") {
      const allowedContacts = gatewaySettings.whatsapp_allowed_contacts
        .split(",")
        .map((entry) => entry.trim())
        .filter(Boolean);
      if (allowedContacts.length > 0) {
        const normalizedTarget = params.replyTarget.replace(/\s+/g, "").toLowerCase();
        const allowed = allowedContacts.some((entry) => {
          const normalizedEntry = entry.replace(/\s+/g, "").toLowerCase();
          return normalizedTarget.includes(normalizedEntry);
        });
        if (!allowed) return;
      }
    }

    const cleaned = rawContent
      .replace(/<@!?\d{17,20}>/g, "")
      .replace(/^\s*amux[:,]?\s*/i, "")
      .trim();

    if (!cleaned) return;
    const routeKey = `${provider}:${channelId}`;
    if (gatewayInFlightRef.current.has(routeKey)) return;

    gatewayInFlightRef.current.add(routeKey);
    try {
      const threadPrefix = `${provider}:${channelId}:`;
      const activeThreadId = useAgentStore.getState().activeThreadId;
      let threadId = activeThreadId ?? gatewayThreadMapRef.current[routeKey];
      let threadExists = threadId
        ? agentState.threads.some((thread) => thread.id === threadId)
        : false;

      if (activeThreadId && threadExists) {
        gatewayThreadMapRef.current[routeKey] = activeThreadId;
        persistGatewayThreadMap();
      }

      if (!threadId || !threadExists) {
        const existingThread = agentState.threads.find((thread) => thread.title.startsWith(threadPrefix));
        if (existingThread) {
          threadId = existingThread.id;
          gatewayThreadMapRef.current[routeKey] = threadId;
          threadExists = true;
          persistGatewayThreadMap();
        }
      }

      if (!threadId || !threadExists) {
        threadId = agentState.createThread({
          workspaceId: useWorkspaceStore.getState().activeWorkspaceId,
          title: `${threadPrefix}${username || "user"}`,
        });
        gatewayThreadMapRef.current[routeKey] = threadId;
        persistGatewayThreadMap();
      }

      agentState.addMessage(threadId, {
        role: "user",
        content: cleaned,
        inputTokens: 0,
        outputTokens: 0,
        totalTokens: 0,
        isCompactionSummary: false,
      });

      const active_provider = agentState.agentSettings.active_provider;
      const providerConfig = agentState.agentSettings[active_provider] as AgentProviderConfig;
      const tools = getAvailableTools({
        enable_bash_tool: agentState.agentSettings.enable_bash_tool,
        gateway_enabled: gatewaySettings.gateway_enabled,
        enable_vision_tool: agentState.agentSettings.enable_vision_tool,
        enable_web_browsing_tool: agentState.agentSettings.enable_web_browsing_tool,
      });
      const toolCapabilities = getToolCapabilityDescription(tools);
      const hiddenGatewayContext = [
        "[Hidden Gateway Context]",
        `provider=${provider}`,
        `channel=${channelId}`,
        `user=${username}${userId ? ` (${userId})` : ""}`,
        "You are tamux's terminal agent running inside the user's tamux terminal environment.",
        "You can use tamux tools and terminal capabilities to help the user.",
        "Respond as the same assistant and keep continuity for this provider thread.",
      ].join("\n");

      const system_prompt = `${agentState.agentSettings.system_prompt}${toolCapabilities}\n\n${hiddenGatewayContext}`;

      agentState.addMessage(threadId, {
        role: "assistant",
        content: "",
        provider: active_provider,
        model: providerConfig.model,
        api_transport: providerConfig.api_transport,
        inputTokens: 0,
        outputTokens: 0,
        totalTokens: 0,
        isCompactionSummary: false,
        isStreaming: true,
      });

      const configuredToolLoops = Number(agentState.agentSettings.max_tool_loops ?? 0);
      const max_tool_loops = Number.isFinite(configuredToolLoops) && configuredToolLoops > 0
        ? Math.min(1000, configuredToolLoops)
        : Infinity;
      let loopCount = 0;
      let finalReply = "";
      const getCurrentProviderConfig = () => (
        useAgentStore.getState().agentSettings[active_provider] as AgentProviderConfig
      );
      const updateThreadUpstreamState = (upstreamThreadId?: string) => {
        useAgentStore.setState((state) => ({
          threads: state.threads.map((thread) => thread.id === threadId ? {
            ...thread,
            upstreamThreadId: upstreamThreadId ?? null,
            upstreamTransport: preparedRequest.transport,
            upstreamProvider: active_provider,
            upstreamModel: getCurrentProviderConfig().model,
            upstreamAssistantId: getCurrentProviderConfig().assistant_id || null,
          } : thread),
        }));
      };
      let preparedRequest = prepareOpenAIRequest(
        useAgentStore.getState().getThreadMessages(threadId).slice(0, -1),
        agentState.agentSettings,
        active_provider,
        getCurrentProviderConfig().model,
        getCurrentProviderConfig().api_transport,
        getCurrentProviderConfig().auth_source,
        getCurrentProviderConfig().assistant_id,
        useAgentStore.getState().threads.find((entry) => entry.id === threadId),
      );
      const controller = new AbortController();
      setThreadAbortController(threadId, controller);
      let lastPersistedReasoning: string | null = null;

      const persistReasoningTrace = (reasoning: string) => {
        const normalized = reasoning.trim();
        if (!normalized) return;
        if (normalized === lastPersistedReasoning) return;

        const thread = useAgentStore.getState().threads.find((entry) => entry.id === threadId);
        useAgentMissionStore.getState().recordCognitiveOutput({
          paneId: thread?.paneId ?? `gateway:${provider}:${channelId}`,
          workspaceId: thread?.workspaceId ?? null,
          surfaceId: thread?.surfaceId ?? null,
          sessionId: null,
          text: `<INNER_MONOLOGUE>\n${normalized}\n</INNER_MONOLOGUE>`,
        });
        lastPersistedReasoning = normalized;
      };

      try {
        while (loopCount < max_tool_loops) {
          loopCount += 1;
          let accumulated = "";
          let accumulatedReasoning = "";
          const responseStartedAt = Date.now();
          let receivedToolCalls = false;
          let roundToolCalls: Array<{ id: string; type: "function"; function: { name: string; arguments: string } }> = [];

          for await (const chunk of sendChatCompletion({
            provider: active_provider,
            config: {
              ...providerConfig,
              api_transport: preparedRequest.transport,
            },
            system_prompt,
            messages: preparedRequest.messages,
            streaming: true,
            signal: controller.signal,
            tools: tools.length > 0 ? tools : undefined,
            previousResponseId: preparedRequest.previousResponseId,
            upstreamThreadId: preparedRequest.upstreamThreadId,
          })) {
            if (chunk.type === "delta") {
              accumulated += chunk.content;
              if (chunk.reasoning) {
                accumulatedReasoning += chunk.reasoning;
              }
              agentState.updateLastAssistantMessage(threadId, accumulated, true, {
                reasoning: accumulatedReasoning || undefined,
              });
            } else if (chunk.type === "done") {
              if (chunk.content && chunk.content !== accumulated) {
                accumulated = chunk.content;
              }
              if (chunk.reasoning) {
                accumulatedReasoning = chunk.reasoning;
              }

              persistReasoningTrace(accumulatedReasoning);

              const elapsedSeconds = Math.max(0.001, (Date.now() - responseStartedAt) / 1000);
              const outputTokens = Number(chunk.outputTokens ?? 0);
              const inputTokens = Number(chunk.inputTokens ?? 0);
              const totalTokens = Number(chunk.totalTokens ?? (inputTokens + outputTokens));
              const tps = outputTokens > 0 ? outputTokens / elapsedSeconds : undefined;

              agentState.updateLastAssistantMessage(threadId, accumulated || "(empty response)", false, {
                inputTokens,
                outputTokens,
                totalTokens,
                reasoning: accumulatedReasoning || undefined,
                reasoningTokens: chunk.reasoningTokens,
                audioTokens: chunk.audioTokens,
                videoTokens: chunk.videoTokens,
                cost: chunk.cost,
                tps,
                api_transport: preparedRequest.transport,
                responseId: chunk.responseId,
              });
              updateThreadUpstreamState(chunk.upstreamThreadId);
            } else if (chunk.type === "error") {
              accumulated = `Error: ${chunk.content}`;
              agentState.updateLastAssistantMessage(threadId, accumulated, false);
            } else if (chunk.type === "transport_fallback") {
              agentState.updateAgentSetting(active_provider as keyof ReturnType<typeof useAgentStore.getState>["agentSettings"], {
                ...providerConfig,
                api_transport: "chat_completions",
              } as any);
              preparedRequest = { ...preparedRequest, transport: "chat_completions", previousResponseId: undefined, upstreamThreadId: undefined };
              updateThreadUpstreamState(undefined);
            } else if (chunk.type === "tool_calls" && chunk.toolCalls) {
              receivedToolCalls = true;
              roundToolCalls = chunk.toolCalls;

              if (chunk.reasoning) {
                accumulatedReasoning = chunk.reasoning;
              }

              if (chunk.content) {
                accumulated = chunk.content;
              }

              persistReasoningTrace(accumulatedReasoning);
              agentState.updateLastAssistantMessage(threadId, accumulated || "Calling tools...", false, {
                reasoning: accumulatedReasoning || undefined,
                inputTokens: Number(chunk.inputTokens ?? 0),
                outputTokens: Number(chunk.outputTokens ?? 0),
                totalTokens: Number(chunk.totalTokens ?? ((chunk.inputTokens ?? 0) + (chunk.outputTokens ?? 0))),
                reasoningTokens: chunk.reasoningTokens,
                audioTokens: chunk.audioTokens,
                videoTokens: chunk.videoTokens,
                cost: chunk.cost,
                toolCalls: roundToolCalls,
                api_transport: preparedRequest.transport,
                responseId: chunk.responseId,
              });
              updateThreadUpstreamState(chunk.upstreamThreadId);

              const toolResults = [];
              for (const tc of chunk.toolCalls) {
                agentState.addMessage(threadId, {
                  role: "tool",
                  content: "",
                  toolName: tc.function.name,
                  toolCallId: tc.id,
                  toolArguments: tc.function.arguments,
                  toolStatus: "requested",
                  inputTokens: 0,
                  outputTokens: 0,
                  totalTokens: 0,
                  isCompactionSummary: false,
                });

                const result = await executeTool(tc);
                toolResults.push(result);
                agentState.addMessage(threadId, {
                  role: "tool",
                  content: result.content,
                  toolName: result.name,
                  toolCallId: result.toolCallId,
                  toolArguments: tc.function.arguments,
                  toolStatus: result.content.startsWith("Error:") ? "error" : "done",
                  inputTokens: 0,
                  outputTokens: 0,
                  totalTokens: 0,
                  isCompactionSummary: false,
                });
              }

              agentState.updateLastAssistantMessage(threadId, accumulated || "Tools executed.", false);

              const currentProviderConfig = getCurrentProviderConfig();
              preparedRequest = prepareOpenAIRequest(
                useAgentStore.getState().getThreadMessages(threadId),
                agentState.agentSettings,
                active_provider,
                currentProviderConfig.model,
                currentProviderConfig.api_transport,
                currentProviderConfig.auth_source,
                currentProviderConfig.assistant_id,
                useAgentStore.getState().threads.find((entry) => entry.id === threadId),
              );

              agentState.addMessage(threadId, {
                role: "assistant",
                content: "",
                provider: active_provider,
                model: providerConfig.model,
                api_transport: preparedRequest.transport,
                inputTokens: 0,
                outputTokens: 0,
                totalTokens: 0,
                isCompactionSummary: false,
                isStreaming: true,
              });
            }
          }

          if (!receivedToolCalls) {
            finalReply = accumulated.trim();
            break;
          }
        }
      } finally {
        clearThreadAbortController(threadId, controller);
      }

      if (Number.isFinite(max_tool_loops) && loopCount >= max_tool_loops) {
        agentState.updateLastAssistantMessage(threadId, "(Tool execution limit reached)", false);
        finalReply = "";
      }

      if (!finalReply) {
        return;
      }

      if (provider === "discord") {
        await amux?.sendDiscordMessage?.({
          token: gatewaySettings.discord_token,
          channelId,
          message: finalReply,
        });
      } else if (provider === "slack") {
        await amux?.sendSlackMessage?.({
          token: gatewaySettings.slack_token,
          channelId,
          message: finalReply,
        });
      } else if (provider === "telegram") {
        await amux?.sendTelegramMessage?.({
          token: gatewaySettings.telegram_token,
          chatId: channelId,
          message: finalReply,
        });
      } else if (provider === "whatsapp") {
        await amux?.whatsappSend?.(params.replyTarget, finalReply);
      }
    } finally {
      gatewayInFlightRef.current.delete(routeKey);
    }
  }, [persistGatewayThreadMap]);

  useEffect(() => {
    const amux = (window as any).tamux ?? (window as any).amux;
    if (!amux?.ensureSlackConnected || !amux?.onSlackMessage) return;
    if (!agentSettings.gateway_enabled || !agentSettings.slack_token) return;

    let disposed = false;
    void amux.ensureSlackConnected({ token: agentSettings.slack_token }).then((result: any) => {
      if (disposed || result?.ok) return;
      console.warn("Slack bridge connection failed:", result?.error ?? "unknown error");
    });

    const unsubscribe = amux.onSlackMessage((payload: any) => {
      const channelId = String(payload?.channelId ?? "").trim();
      const channelName = String(payload?.channelName ?? "").trim();
      void handleInboundGatewayMessage({
        provider: "slack",
        channelId,
        userId: String(payload?.userId ?? ""),
        username: String(payload?.username ?? "slack-user"),
        text: String(payload?.content ?? ""),
        replyTarget: channelName || channelId,
      });
    });

    return () => {
      disposed = true;
      unsubscribe?.();
    };
  }, [handleInboundGatewayMessage, agentSettings.gateway_enabled, agentSettings.slack_token]);

  useEffect(() => {
    const amux = (window as any).tamux ?? (window as any).amux;
    if (!amux?.ensureTelegramConnected || !amux?.onTelegramMessage) return;
    if (!agentSettings.gateway_enabled || !agentSettings.telegram_token) return;

    let disposed = false;
    void amux.ensureTelegramConnected({ token: agentSettings.telegram_token }).then((result: any) => {
      if (disposed || result?.ok) return;
      console.warn("Telegram bridge connection failed:", result?.error ?? "unknown error");
    });

    const unsubscribe = amux.onTelegramMessage((payload: any) => {
      void handleInboundGatewayMessage({
        provider: "telegram",
        channelId: String(payload?.chatId ?? ""),
        userId: String(payload?.userId ?? ""),
        username: String(payload?.username ?? "telegram-user"),
        text: String(payload?.content ?? ""),
        replyTarget: String(payload?.chatId ?? ""),
      });
    });

    return () => {
      disposed = true;
      unsubscribe?.();
    };
  }, [handleInboundGatewayMessage, agentSettings.gateway_enabled, agentSettings.telegram_token]);

  useEffect(() => {
    const amux = (window as any).tamux ?? (window as any).amux;
    if (!amux?.ensureDiscordConnected || !amux?.onDiscordMessage) return;
    if (!agentSettings.gateway_enabled || !agentSettings.discord_token) return;

    let disposed = false;
    void amux.ensureDiscordConnected({ token: agentSettings.discord_token }).then((result: any) => {
      if (disposed || result?.ok) return;
      console.warn("Discord bridge connection failed:", result?.error ?? "unknown error");
    });

    const unsubscribe = amux.onDiscordMessage((payload: any) => {
      void handleInboundGatewayMessage({
        provider: "discord",
        channelId: String(payload?.channelId ?? ""),
        userId: String(payload?.userId ?? ""),
        username: String(payload?.username ?? "discord-user"),
        text: String(payload?.content ?? ""),
        replyTarget: String(payload?.channelId ?? ""),
      });
    });

    return () => {
      disposed = true;
      unsubscribe?.();
    };
  }, [handleInboundGatewayMessage, agentSettings.discord_token, agentSettings.gateway_enabled]);

  useEffect(() => {
    const amux = (window as any).tamux ?? (window as any).amux;
    if (!amux?.onWhatsAppMessage || !amux?.whatsappStatus) return;
    if (!agentSettings.gateway_enabled) return;

    let disposed = false;
    void amux.whatsappStatus().then((status: any) => {
      if (disposed) return;
      if (status?.status !== "connected") return;
    });

    const unsubscribe = amux.onWhatsAppMessage((msg: any) => {
      void handleInboundGatewayMessage({
        provider: "whatsapp",
        channelId: String(msg?.from ?? ""),
        userId: String(msg?.from ?? ""),
        username: String(msg?.pushName ?? "whatsapp-user"),
        text: String(msg?.text ?? ""),
        replyTarget: String(msg?.from ?? ""),
      });
    });

    return () => {
      disposed = true;
      unsubscribe?.();
    };
  }, [handleInboundGatewayMessage, agentSettings.gateway_enabled]);

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
