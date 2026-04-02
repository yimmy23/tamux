import type { WorkspaceId } from "../types";
import type { WorkspaceBrowserProjection, WorkspaceBrowserState } from "./types";

export const DEFAULT_WEB_BROWSER_URL = "https://google.com";

export function normalizeBrowserUrl(value: string): string {
  const trimmed = value.trim();
  if (!trimmed) return "about:blank";
  if (/^(about:|https?:\/\/|file:\/\/)/i.test(trimmed)) return trimmed;
  return `https://${trimmed}`;
}

export function createDefaultWorkspaceBrowserState(
  seedUrl: string = DEFAULT_WEB_BROWSER_URL,
): WorkspaceBrowserState {
  const normalizedUrl = normalizeBrowserUrl(seedUrl);
  return {
    open: false,
    fullscreen: false,
    url: normalizedUrl,
    history: [normalizedUrl],
    historyIndex: 0,
    reloadToken: 0,
  };
}

export function normalizeWorkspaceBrowserState(
  input?: Partial<WorkspaceBrowserState>,
): WorkspaceBrowserState {
  const normalizedUrl = normalizeBrowserUrl(typeof input?.url === "string" ? input.url : DEFAULT_WEB_BROWSER_URL);
  const history = Array.isArray(input?.history)
    ? input.history
      .filter((value): value is string => typeof value === "string" && value.trim().length > 0)
      .map((value) => normalizeBrowserUrl(value))
    : [];
  const normalizedHistory = history.length > 0 ? history : [normalizedUrl];
  const normalizedHistoryIndex = typeof input?.historyIndex === "number" && Number.isFinite(input.historyIndex)
    ? Math.max(0, Math.min(normalizedHistory.length - 1, Math.floor(input.historyIndex)))
    : normalizedHistory.length - 1;

  return {
    open: Boolean(input?.open),
    fullscreen: Boolean(input?.fullscreen),
    url: normalizedHistory[normalizedHistoryIndex] ?? normalizedUrl,
    history: normalizedHistory,
    historyIndex: normalizedHistoryIndex,
    reloadToken: typeof input?.reloadToken === "number" && Number.isFinite(input.reloadToken)
      ? Math.max(0, Math.floor(input.reloadToken))
      : 0,
  };
}

export function projectWorkspaceBrowserState(
  browser: WorkspaceBrowserState,
): WorkspaceBrowserProjection {
  return {
    webBrowserOpen: browser.open,
    webBrowserFullscreen: browser.fullscreen,
    webBrowserUrl: browser.url,
    webBrowserHistory: browser.history,
    webBrowserHistoryIndex: browser.historyIndex,
    webBrowserReloadToken: browser.reloadToken,
  };
}

export function browserStateForWorkspace(
  workspaceBrowserState: Record<WorkspaceId, WorkspaceBrowserState>,
  workspaceId: WorkspaceId,
): WorkspaceBrowserState {
  return workspaceBrowserState[workspaceId] ?? createDefaultWorkspaceBrowserState();
}

export function activateWorkspaceBrowserState(
  workspaceBrowserState: Record<WorkspaceId, WorkspaceBrowserState>,
  workspaceId: WorkspaceId | null,
): { workspaceBrowserState: Record<WorkspaceId, WorkspaceBrowserState> } & WorkspaceBrowserProjection {
  if (!workspaceId) {
    const fallback = createDefaultWorkspaceBrowserState();
    return {
      workspaceBrowserState,
      ...projectWorkspaceBrowserState(fallback),
    };
  }

  const browser = browserStateForWorkspace(workspaceBrowserState, workspaceId);
  const normalizedMap = workspaceBrowserState[workspaceId]
    ? workspaceBrowserState
    : { ...workspaceBrowserState, [workspaceId]: browser };

  return {
    workspaceBrowserState: normalizedMap,
    ...projectWorkspaceBrowserState(browser),
  };
}
