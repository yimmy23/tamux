import { allLeafIds } from "../bspTree";
import { useWorkspaceStore } from "../workspaceStore";

export function resolveActivePaneId(): string | null {
  const store = useWorkspaceStore.getState();
  const surface = store.activeSurface();
  if (!surface) {
    return null;
  }
  return surface.activePaneId ?? allLeafIds(surface.layout)[0] ?? null;
}

export function resolvePaneIdByRef(paneRef?: string): string | null {
  const store = useWorkspaceStore.getState();
  const workspace = store.activeWorkspace();
  if (!workspace) {
    return null;
  }

  const ref = (paneRef ?? "").trim().toLowerCase();
  if (!ref) {
    return resolveActivePaneId();
  }

  const activeSurface = store.activeSurface();
  if (activeSurface) {
    const activeSurfacePaneIds = allLeafIds(activeSurface.layout);
    if (activeSurfacePaneIds.includes(paneRef!)) return paneRef!;
    for (const paneId of activeSurfacePaneIds) {
      const paneName = activeSurface.paneNames[paneId]?.trim().toLowerCase();
      if (paneName && paneName === ref) return paneId;
    }
  }

  for (const surface of workspace.surfaces) {
    const paneIds = allLeafIds(surface.layout);
    if (paneIds.includes(paneRef!)) return paneRef!;
    for (const paneId of paneIds) {
      const paneName = surface.paneNames[paneId]?.trim().toLowerCase();
      if (paneName && paneName === ref) return paneId;
    }
  }
  return null;
}

export function resolveWorkspaceIdByRef(workspaceRef?: string): string | null {
  const store = useWorkspaceStore.getState();
  const workspaces = store.workspaces;
  const ref = (workspaceRef ?? "").trim();
  if (!ref) return store.activeWorkspaceId;

  const byId = workspaces.find((workspace) => workspace.id === ref);
  if (byId) return byId.id;

  const lower = ref.toLowerCase();
  const byName = workspaces.find((workspace) => workspace.name.trim().toLowerCase() === lower);
  return byName?.id ?? null;
}

export function resolveSurfaceIdByRef(surfaceRef?: string, workspaceRef?: string): string | null {
  const store = useWorkspaceStore.getState();
  const workspaceId = resolveWorkspaceIdByRef(workspaceRef);
  const workspace = workspaceId
    ? store.workspaces.find((entry) => entry.id === workspaceId)
    : store.activeWorkspace();
  if (!workspace) {
    return null;
  }

  const ref = (surfaceRef ?? "").trim();
  if (!ref) return workspace.activeSurfaceId;

  const byId = workspace.surfaces.find((surface) => surface.id === ref);
  if (byId) return byId.id;

  const lower = ref.toLowerCase();
  const byName = workspace.surfaces.find((surface) => surface.name.trim().toLowerCase() === lower);
  return byName?.id ?? null;
}
