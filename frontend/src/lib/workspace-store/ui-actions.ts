import type { WorkspaceState, WorkspaceBrowserState } from "./types";
import type { WorkspaceStoreContext } from "./store-context";
import { normalizeBrowserUrl } from "./browser";

export function createUiActions(
  ctx: WorkspaceStoreContext,
): Pick<
  WorkspaceState,
  | "toggleSidebar"
  | "setSidebarWidth"
  | "toggleCommandPalette"
  | "toggleSearch"
  | "toggleSettings"
  | "toggleNotificationPanel"
  | "toggleSessionVault"
  | "toggleCommandLog"
  | "toggleCommandHistory"
  | "toggleSnippetPicker"
  | "toggleAgentPanel"
  | "toggleSystemMonitor"
  | "toggleFileManager"
  | "toggleCanvas"
  | "toggleTimeTravel"
  | "toggleWebBrowser"
  | "setWebBrowserOpen"
  | "navigateWebBrowser"
  | "webBrowserBack"
  | "webBrowserForward"
  | "webBrowserReload"
  | "toggleWebBrowserFullscreen"
  | "setWebBrowserFullscreen"
> {
  function updateActiveWorkspaceBrowser(
    updater: (browser: WorkspaceBrowserState) => WorkspaceBrowserState,
  ) {
    ctx.set((state) => {
      const workspaceId = state.activeWorkspaceId;
      if (!workspaceId) return {};
      const current = ctx.browserStateForWorkspace(state.workspaceBrowserState, workspaceId);
      const nextWorkspaceBrowserState = {
        ...state.workspaceBrowserState,
        [workspaceId]: updater(current),
      };
      return ctx.activateWorkspaceBrowserState(nextWorkspaceBrowserState, workspaceId);
    });
  }

  return {
    toggleSidebar: () => ctx.set((state) => ({ sidebarVisible: !state.sidebarVisible })),
    setSidebarWidth: (width) => ctx.set((state) => ({
      sidebarWidth: Math.max(180, Math.min(540, Math.round(width))),
      sidebarVisible: true,
      workspaces: state.workspaces,
    })),
    toggleCommandPalette: () => ctx.set((state) => ({ commandPaletteOpen: !state.commandPaletteOpen })),
    toggleSearch: () => ctx.set((state) => ({ searchOpen: !state.searchOpen })),
    toggleSettings: () => ctx.set((state) => ({ settingsOpen: !state.settingsOpen })),
    toggleNotificationPanel: () => ctx.set((state) => ({ notificationPanelOpen: !state.notificationPanelOpen })),
    toggleSessionVault: () => ctx.set((state) => ({ sessionVaultOpen: !state.sessionVaultOpen })),
    toggleCommandLog: () => ctx.set((state) => ({ commandLogOpen: !state.commandLogOpen })),
    toggleCommandHistory: () => ctx.set((state) => ({ commandHistoryOpen: !state.commandHistoryOpen })),
    toggleSnippetPicker: () => ctx.set((state) => ({ snippetPickerOpen: !state.snippetPickerOpen })),
    toggleAgentPanel: () => ctx.set((state) => ({ agentPanelOpen: !state.agentPanelOpen })),
    toggleSystemMonitor: () => ctx.set((state) => ({ systemMonitorOpen: !state.systemMonitorOpen })),
    toggleFileManager: () => ctx.set((state) => ({ fileManagerOpen: !state.fileManagerOpen })),
    toggleCanvas: () => ctx.set((state) => ({ canvasOpen: !state.canvasOpen })),
    toggleTimeTravel: () => ctx.set((state) => ({ timeTravelOpen: !state.timeTravelOpen })),
    toggleWebBrowser: () => updateActiveWorkspaceBrowser((browser) => ({
      ...browser,
      open: !browser.open,
    })),
    setWebBrowserOpen: (open) => updateActiveWorkspaceBrowser((browser) => ({
      ...browser,
      open,
    })),
    navigateWebBrowser: (url) => {
      const normalized = normalizeBrowserUrl(url);
      updateActiveWorkspaceBrowser((browser) => {
        if (browser.url === normalized) {
          return { ...browser, open: true };
        }
        const prefix = browser.history.slice(0, browser.historyIndex + 1);
        const history = [...prefix, normalized];
        return {
          ...browser,
          open: true,
          url: normalized,
          history,
          historyIndex: history.length - 1,
        };
      });
    },
    webBrowserBack: () => updateActiveWorkspaceBrowser((browser) => {
      const nextIndex = Math.max(0, browser.historyIndex - 1);
      return {
        ...browser,
        historyIndex: nextIndex,
        url: browser.history[nextIndex] ?? browser.url,
      };
    }),
    webBrowserForward: () => updateActiveWorkspaceBrowser((browser) => {
      const nextIndex = Math.min(browser.history.length - 1, browser.historyIndex + 1);
      return {
        ...browser,
        historyIndex: nextIndex,
        url: browser.history[nextIndex] ?? browser.url,
      };
    }),
    webBrowserReload: () => updateActiveWorkspaceBrowser((browser) => ({
      ...browser,
      reloadToken: browser.reloadToken + 1,
    })),
    toggleWebBrowserFullscreen: () => updateActiveWorkspaceBrowser((browser) => ({
      ...browser,
      fullscreen: !browser.fullscreen,
    })),
    setWebBrowserFullscreen: (fullscreen) => updateActiveWorkspaceBrowser((browser) => ({
      ...browser,
      fullscreen,
    })),
  };
}
