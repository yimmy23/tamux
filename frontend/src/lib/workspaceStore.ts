import { create } from "zustand";
import { DEFAULT_WEB_BROWSER_URL } from "./workspace-store/browser";
import { createCanvasActions } from "./workspace-store/canvas-actions";
import { shortenHomePath } from "./workspace-store/helpers";
import { createPaneActions } from "./workspace-store/pane-actions";
import { createWorkspaceStoreContext } from "./workspace-store/store-context";
import { createSurfaceActions } from "./workspace-store/surface-actions";
import type { WorkspaceState } from "./workspace-store/types";
import { createUiActions } from "./workspace-store/ui-actions";
import { createWorkspaceActions } from "./workspace-store/workspace-actions";

export type { WorkspaceState } from "./workspace-store/types";
export { shortenHomePath };

export const useWorkspaceStore = create<WorkspaceState>((set, get) => {
  const ctx = createWorkspaceStoreContext(set, get);

  return {
    workspaces: [],
    activeWorkspaceId: null,
    sidebarVisible: true,
    sidebarWidth: 400,
    zoomedPaneId: null,
    commandPaletteOpen: false,
    searchOpen: false,
    settingsOpen: false,
    notificationPanelOpen: false,
    sessionVaultOpen: false,
    commandLogOpen: false,
    commandHistoryOpen: false,
    snippetPickerOpen: false,
    agentPanelOpen: true,
    systemMonitorOpen: false,
    fileManagerOpen: false,
    canvasOpen: false,
    timeTravelOpen: false,
    workspaceBrowserState: {},
    webBrowserOpen: false,
    webBrowserFullscreen: false,
    webBrowserUrl: DEFAULT_WEB_BROWSER_URL,
    webBrowserHistory: [DEFAULT_WEB_BROWSER_URL],
    webBrowserHistoryIndex: 0,
    webBrowserReloadToken: 0,
    ...createWorkspaceActions(ctx),
    ...createSurfaceActions(ctx),
    ...createPaneActions(ctx),
    ...createUiActions(ctx),
    ...createCanvasActions(ctx),
    activeWorkspace: () => {
      const { workspaces, activeWorkspaceId } = get();
      return workspaces.find((workspace) => workspace.id === activeWorkspaceId);
    },
    activeSurface: () => ctx.getActiveSurface(),
    activePaneId: () => ctx.getActiveSurface()?.activePaneId ?? null,
  };
});
