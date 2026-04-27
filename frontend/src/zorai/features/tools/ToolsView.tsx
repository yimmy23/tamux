import { useEffect, type CSSProperties } from "react";
import { CommandLogPanel } from "@/components/CommandLogPanel";
import { FileManagerPanel } from "@/components/FileManagerPanel";
import { InfiniteCanvasSurface } from "@/components/InfiniteCanvasSurface";
import { SessionVaultPanel } from "@/components/SessionVaultPanel";
import { SystemMonitorPanel } from "@/components/SystemMonitorPanel";
import { TerminalPane } from "@/components/TerminalPane";
import { WebBrowserPanel } from "@/components/WebBrowserPanel";
import type { Surface } from "@/lib/types";
import { useWorkspaceStore } from "@/lib/workspaceStore";
import { zoraiTools, type ZoraiToolId } from "./tools";

type ToolsProps = {
  activeTool: ZoraiToolId;
  onSelectTool: (toolId: ZoraiToolId) => void;
};

const embeddedPanelStyle: CSSProperties = {
  position: "relative",
  inset: "auto",
  zIndex: "auto",
  width: "100%",
  minWidth: 0,
  maxWidth: "none",
  height: "100%",
  minHeight: 0,
  maxHeight: "none",
  padding: 0,
  background: "transparent",
  border: "1px solid var(--zorai-border)",
  borderRadius: 7,
  overflow: "hidden",
};

export function ToolsRail({ activeTool, onSelectTool }: ToolsProps) {
  return (
    <div className="zorai-rail-stack">
      <div className="zorai-section-label">Tools</div>
      {zoraiTools.map((tool) => (
        <button
          type="button"
          key={tool.id}
          className={[
            "zorai-rail-card",
            "zorai-rail-card--button",
            tool.id === activeTool ? "zorai-rail-card--active" : "",
          ].filter(Boolean).join(" ")}
          onClick={() => onSelectTool(tool.id)}
        >
          <strong>{tool.title}</strong>
          <span>{tool.description}</span>
        </button>
      ))}
    </div>
  );
}

export function ToolsView({ activeTool, onSelectTool }: ToolsProps) {
  const selectedTool = zoraiTools.find((tool) => tool.id === activeTool) ?? zoraiTools[0];

  return (
    <section className="zorai-feature-surface zorai-tools-surface">
      <div className="zorai-view-header">
        <div>
          <div className="zorai-kicker">Tools</div>
          <h1>{selectedTool.title}</h1>
          <p>
            Zorai keeps terminal multiplexing as a useful capability, while the default
            shell remains centered on threads, goals, and workspace orchestration.
          </p>
        </div>
      </div>

      <div className="zorai-tool-tab-strip" aria-label="Tool sections">
        {zoraiTools.map((tool) => (
          <button
            type="button"
            key={tool.id}
            className={["zorai-ghost-button", tool.id === activeTool ? "zorai-button--active" : ""].filter(Boolean).join(" ")}
            onClick={() => onSelectTool(tool.id)}
          >
            {tool.title}
          </button>
        ))}
      </div>

      <div className="zorai-tool-layout">
        <div className="zorai-tool-frame">
          <ToolSurface activeTool={activeTool} />
        </div>
      </div>
    </section>
  );
}

function ToolSurface({ activeTool }: { activeTool: ZoraiToolId }) {
  useEmbeddedToolState(activeTool);

  if (activeTool === "terminal") return <TerminalTool />;
  if (activeTool === "canvas") return <CanvasTool />;
  if (activeTool === "files") return <FileManagerPanel style={embeddedPanelStyle} className="zorai-embedded-tool-panel" />;
  if (activeTool === "browser") return <WebBrowserPanel style={embeddedPanelStyle} className="zorai-embedded-tool-panel" />;
  if (activeTool === "history") return <CommandLogPanel style={embeddedPanelStyle} className="zorai-embedded-tool-panel" />;
  if (activeTool === "system") return <SystemMonitorPanel style={embeddedPanelStyle} className="zorai-embedded-tool-panel" />;

  return <SessionVaultPanel style={embeddedPanelStyle} className="zorai-embedded-tool-panel" />;
}

function TerminalTool() {
  const activePaneId = useWorkspaceStore((state) => state.activePaneId());
  const activeWorkspace = useWorkspaceStore((state) => state.activeWorkspace());
  const activeSurface = useWorkspaceStore((state) => state.activeSurface());
  const createWorkspace = useWorkspaceStore((state) => state.createWorkspace);
  const createSurface = useWorkspaceStore((state) => state.createSurface);
  const splitActive = useWorkspaceStore((state) => state.splitActive);

  const createTerminalSurface = () => {
    if (!activeWorkspace) {
      createWorkspace("Workspace", { layoutMode: "bsp" });
      return;
    }
    createSurface(activeWorkspace.id, { layoutMode: "bsp" });
  };

  const splitTerminal = (direction: "horizontal" | "vertical") => {
    if (!activeSurface) {
      createTerminalSurface();
      return;
    }
    splitActive(direction, direction === "horizontal" ? "Right Terminal" : "Down Terminal");
  };

  return (
    <div className="zorai-tool-workbench">
      <div className="zorai-tool-actionbar">
        <div>
          <div className="zorai-section-label">Terminal surfaces</div>
          <strong>{activeSurface?.layoutMode === "canvas" ? "Canvas terminal panel" : "BSP terminal surface"}</strong>
        </div>
        <div className="zorai-card-actions">
          <button type="button" className="zorai-primary-button" onClick={createTerminalSurface}>New terminal surface</button>
          <button type="button" className="zorai-ghost-button" onClick={() => splitTerminal("horizontal")}>Split right</button>
          <button type="button" className="zorai-ghost-button" onClick={() => splitTerminal("vertical")}>Split down</button>
          <CanvasCreateButton />
        </div>
      </div>

      <div className="zorai-terminal-tool">
        {activePaneId ? (
          <TerminalPane paneId={activePaneId} hideHeader />
        ) : (
          <div className="zorai-tool-empty">
            <strong>No active terminal pane</strong>
            <span>Create a terminal surface or split the current surface to attach the terminal tool.</span>
          </div>
        )}
      </div>
    </div>
  );
}

function CanvasTool() {
  const activeWorkspace = useWorkspaceStore((state) => state.activeWorkspace());
  const activeSurface = useWorkspaceStore((state) => state.activeSurface());
  const createWorkspace = useWorkspaceStore((state) => state.createWorkspace);
  const createSurface = useWorkspaceStore((state) => state.createSurface);
  const setActiveSurface = useWorkspaceStore((state) => state.setActiveSurface);
  const createCanvasPanel = useWorkspaceStore((state) => state.createCanvasPanel);
  const arrangeCanvasPanels = useWorkspaceStore((state) => state.arrangeCanvasPanels);
  const canvasSurface = resolveCanvasSurface(activeSurface, activeWorkspace?.surfaces ?? []);

  const ensureCanvasSurface = () => {
    if (canvasSurface) {
      setActiveSurface(canvasSurface.id);
      return canvasSurface;
    }
    if (!activeWorkspace) {
      createWorkspace("Workspace", { layoutMode: "canvas" });
      return getActiveCanvasSurface();
    }
    const surfaceId = createSurface(activeWorkspace.id, { layoutMode: "canvas" });
    return getSurfaceById(surfaceId);
  };

  const createCanvasSurface = () => {
    ensureCanvasSurface();
  };

  const createCanvasTerminal = () => {
    const surface = ensureCanvasSurface();
    if (surface) createCanvasPanel(surface.id, { paneName: "Terminal", paneIcon: "terminal" });
  };

  const createCanvasBrowser = () => {
    const surface = ensureCanvasSurface();
    if (surface) {
      createCanvasPanel(surface.id, {
        panelType: "browser",
        paneName: "Browser",
        paneIcon: "web",
        url: "https://google.com",
      });
    }
  };

  return (
    <div className="zorai-tool-workbench">
      <div className="zorai-tool-actionbar">
        <div>
          <div className="zorai-section-label">Infinite Canvas</div>
          <strong>{canvasSurface ? canvasSurface.name : "No canvas surface open"}</strong>
        </div>
        <div className="zorai-card-actions">
          <button type="button" className="zorai-primary-button" onClick={createCanvasSurface}>New infinite canvas</button>
          <button type="button" className="zorai-ghost-button" onClick={createCanvasTerminal}>Add terminal panel</button>
          <button type="button" className="zorai-ghost-button" onClick={createCanvasBrowser}>Add browser panel</button>
          <button type="button" className="zorai-ghost-button" onClick={() => canvasSurface && arrangeCanvasPanels(canvasSurface.id)} disabled={!canvasSurface}>Arrange canvas</button>
        </div>
      </div>

      <div className="zorai-canvas-tool">
        {canvasSurface ? (
          <InfiniteCanvasSurface surface={canvasSurface} />
        ) : (
          <div className="zorai-tool-empty">
            <strong>Start an infinite canvas</strong>
            <span>Create a canvas surface to arrange terminal and browser panels freely.</span>
          </div>
        )}
      </div>
    </div>
  );
}

function CanvasCreateButton() {
  const activeWorkspace = useWorkspaceStore((state) => state.activeWorkspace());
  const createWorkspace = useWorkspaceStore((state) => state.createWorkspace);
  const createSurface = useWorkspaceStore((state) => state.createSurface);

  const createCanvasSurface = () => {
    if (!activeWorkspace) {
      createWorkspace("Workspace", { layoutMode: "canvas" });
      return;
    }
    createSurface(activeWorkspace.id, { layoutMode: "canvas" });
  };

  return <button type="button" className="zorai-ghost-button" onClick={createCanvasSurface}>New infinite canvas</button>;
}

function resolveCanvasSurface(activeSurface: Surface | undefined, surfaces: Surface[]): Surface | null {
  if (activeSurface?.layoutMode === "canvas") return activeSurface;
  return surfaces.find((surface) => surface.layoutMode === "canvas") ?? null;
}

function getActiveCanvasSurface(): Surface | null {
  const surface = useWorkspaceStore.getState().activeSurface();
  return surface?.layoutMode === "canvas" ? surface : null;
}

function getSurfaceById(surfaceId: string | null): Surface | null {
  if (!surfaceId) return null;
  return useWorkspaceStore.getState().workspaces
    .flatMap((workspace) => workspace.surfaces)
    .find((surface) => surface.id === surfaceId) ?? null;
}

function useEmbeddedToolState(activeTool: ZoraiToolId) {
  const fileManagerOpen = useWorkspaceStore((state) => state.fileManagerOpen);
  const commandLogOpen = useWorkspaceStore((state) => state.commandLogOpen);
  const sessionVaultOpen = useWorkspaceStore((state) => state.sessionVaultOpen);
  const systemMonitorOpen = useWorkspaceStore((state) => state.systemMonitorOpen);
  const webBrowserOpen = useWorkspaceStore((state) => state.webBrowserOpen);
  const setWebBrowserOpen = useWorkspaceStore((state) => state.setWebBrowserOpen);
  const setWebBrowserFullscreen = useWorkspaceStore((state) => state.setWebBrowserFullscreen);

  useEffect(() => {
    useWorkspaceStore.setState({
      fileManagerOpen: activeTool === "files",
      commandLogOpen: activeTool === "history",
      sessionVaultOpen: activeTool === "vault",
      systemMonitorOpen: activeTool === "system",
    });
    setWebBrowserOpen(activeTool === "browser");
    if (activeTool === "browser") setWebBrowserFullscreen(false);

    return () => {
      useWorkspaceStore.setState({
        fileManagerOpen: false,
        commandLogOpen: false,
        sessionVaultOpen: false,
        systemMonitorOpen: false,
      });
      setWebBrowserOpen(false);
    };
  }, [activeTool, setWebBrowserFullscreen, setWebBrowserOpen]);

  useEffect(() => {
    if (activeTool === "files" && !fileManagerOpen) {
      useWorkspaceStore.setState({ fileManagerOpen: true });
    }
    if (activeTool === "history" && !commandLogOpen) {
      useWorkspaceStore.setState({ commandLogOpen: true });
    }
    if (activeTool === "vault" && !sessionVaultOpen) {
      useWorkspaceStore.setState({ sessionVaultOpen: true });
    }
    if (activeTool === "system" && !systemMonitorOpen) {
      useWorkspaceStore.setState({ systemMonitorOpen: true });
    }
    if (activeTool === "browser" && !webBrowserOpen) {
      setWebBrowserOpen(true);
    }
  }, [
    activeTool,
    commandLogOpen,
    fileManagerOpen,
    sessionVaultOpen,
    setWebBrowserOpen,
    systemMonitorOpen,
    webBrowserOpen,
  ]);
}
