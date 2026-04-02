import { allLeafIds } from "../bspTree";
import { normalizeIconId } from "../iconRegistry";
import type { WorkspaceState } from "./types";
import type { WorkspaceStoreContext } from "./store-context";
import {
  captureTranscriptForPane,
  createDefaultSurface,
  stopPaneSessions,
} from "./helpers";

export function createSurfaceActions(
  ctx: WorkspaceStoreContext,
): Pick<
  WorkspaceState,
  | "createSurface"
  | "renameSurface"
  | "setSurfaceIcon"
  | "closeSurface"
  | "nextSurface"
  | "prevSurface"
  | "setActiveSurface"
> {
  return {
    createSurface: (workspaceId, opts) => {
      const wsId = workspaceId ?? ctx.get().activeWorkspaceId;
      if (!wsId) return null;
      const layoutMode = opts?.layoutMode ?? "bsp";
      const makeActive = opts?.makeActive ?? true;
      const sf = createDefaultSurface(wsId, layoutMode);
      ctx.set((state) => ({
        workspaces: state.workspaces.map((ws) => (
          ws.id === wsId
            ? {
              ...ws,
              surfaces: [...ws.surfaces, sf],
              activeSurfaceId: makeActive ? sf.id : ws.activeSurfaceId,
            }
            : ws
        )),
      }));
      return sf.id;
    },

    renameSurface: (surfaceId, name) => {
      const nextName = typeof name === "string" ? name.trim() : "";
      if (!nextName) return;
      ctx.updateSurface(surfaceId, (surface) => ({ ...surface, name: nextName }));
    },

    setSurfaceIcon: (surfaceId, icon) => {
      const nextIcon = normalizeIconId(icon);
      ctx.updateSurface(surfaceId, (surface) => ({ ...surface, icon: nextIcon }));
    },

    closeSurface: (surfaceId) => {
      const { workspaces } = ctx.get();
      const surfacePair = ctx.findWsAndSurface(surfaceId);
      if (surfacePair) {
        const paneIds = allLeafIds(surfacePair.sf.layout);
        for (const paneId of paneIds) {
          captureTranscriptForPane({
            paneId,
            workspaceId: surfacePair.ws.id,
            surfaceId: surfacePair.sf.id,
            cwd: surfacePair.ws.cwd,
            reason: "surface-close",
          });
        }
        stopPaneSessions(paneIds);
      }

      ctx.set({
        workspaces: workspaces.map((ws) => {
          const remaining = ws.surfaces.filter((surface) => surface.id !== surfaceId);
          if (remaining.length === 0) {
            const sf = createDefaultSurface(ws.id);
            return {
              ...ws,
              surfaces: [sf],
              activeSurfaceId: sf.id,
            };
          }
          return {
            ...ws,
            surfaces: remaining,
            activeSurfaceId: ws.activeSurfaceId === surfaceId ? remaining[0].id : ws.activeSurfaceId,
          };
        }),
      });
    },

    nextSurface: () => {
      const ws = ctx.get().activeWorkspace();
      if (!ws) return;
      const idx = ws.surfaces.findIndex((surface) => surface.id === ws.activeSurfaceId);
      const next = ws.surfaces[(idx + 1) % ws.surfaces.length];
      if (!next) return;
      ctx.set((state) => ({
        workspaces: state.workspaces.map((workspace) => (
          workspace.id === ws.id ? { ...workspace, activeSurfaceId: next.id } : workspace
        )),
      }));
    },

    prevSurface: () => {
      const ws = ctx.get().activeWorkspace();
      if (!ws) return;
      const idx = ws.surfaces.findIndex((surface) => surface.id === ws.activeSurfaceId);
      const prev = ws.surfaces[(idx - 1 + ws.surfaces.length) % ws.surfaces.length];
      if (!prev) return;
      ctx.set((state) => ({
        workspaces: state.workspaces.map((workspace) => (
          workspace.id === ws.id ? { ...workspace, activeSurfaceId: prev.id } : workspace
        )),
      }));
    },

    setActiveSurface: (surfaceId) => {
      const pair = ctx.findWsAndSurface(surfaceId);
      if (!pair) return;
      ctx.set((state) => ({
        activeWorkspaceId: pair.ws.id,
        zoomedPaneId: null,
        ...ctx.activateWorkspaceBrowserState(state.workspaceBrowserState, pair.ws.id),
        workspaces: state.workspaces.map((workspace) => (
          workspace.id === pair.ws.id ? { ...workspace, activeSurfaceId: surfaceId } : workspace
        )),
      }));
    },
  };
}
