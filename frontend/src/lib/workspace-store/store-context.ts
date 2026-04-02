import { allLeafIds } from "../bspTree";
import type { Surface, SurfaceId, Workspace, PaneId } from "../types";
import type { WorkspaceState } from "./types";
import { activateWorkspaceBrowserState, browserStateForWorkspace } from "./browser";

type WorkspaceSet = (
  partial:
    | Partial<WorkspaceState>
    | ((state: WorkspaceState) => Partial<WorkspaceState>),
) => void;

type WorkspaceGet = () => WorkspaceState;

export interface WorkspaceStoreContext {
  get: WorkspaceGet;
  set: WorkspaceSet;
  findWsAndSurface: (surfaceId: SurfaceId) => { ws: Workspace; sf: Surface } | null;
  findWsSurfaceAndPane: (paneId: PaneId) => { ws: Workspace; sf: Surface } | null;
  updateSurface: (surfaceId: SurfaceId, updater: (surface: Surface) => Surface) => void;
  getActiveSurface: () => Surface | undefined;
  browserStateForWorkspace: typeof browserStateForWorkspace;
  activateWorkspaceBrowserState: typeof activateWorkspaceBrowserState;
}

export function createWorkspaceStoreContext(
  set: WorkspaceSet,
  get: WorkspaceGet,
): WorkspaceStoreContext {
  function findWsAndSurface(surfaceId: SurfaceId) {
    const { workspaces } = get();
    for (const ws of workspaces) {
      const sf = ws.surfaces.find((surface) => surface.id === surfaceId);
      if (sf) return { ws, sf };
    }
    return null;
  }

  function findWsSurfaceAndPane(paneId: PaneId) {
    const { workspaces } = get();
    for (const ws of workspaces) {
      for (const sf of ws.surfaces) {
        if (allLeafIds(sf.layout).includes(paneId)) {
          return { ws, sf };
        }
      }
    }
    return null;
  }

  function updateSurface(surfaceId: SurfaceId, updater: (surface: Surface) => Surface) {
    const { workspaces } = get();
    set({
      workspaces: workspaces.map((ws) => ({
        ...ws,
        surfaces: ws.surfaces.map((sf) => (sf.id === surfaceId ? updater(sf) : sf)),
      })),
    });
  }

  function getActiveSurface(): Surface | undefined {
    const { workspaces, activeWorkspaceId } = get();
    const ws = workspaces.find((workspace) => workspace.id === activeWorkspaceId);
    if (!ws) return undefined;
    return ws.surfaces.find((surface) => surface.id === ws.activeSurfaceId);
  }

  return {
    get,
    set,
    findWsAndSurface,
    findWsSurfaceAndPane,
    updateSurface,
    getActiveSurface,
    browserStateForWorkspace,
    activateWorkspaceBrowserState,
  };
}
