import {
  allLeafIds,
  buildPresetLayout,
  equalizeLayoutRatios,
  findAdjacentPane,
  removePane,
  setSessionId,
  splitPane,
  updateRatio,
} from "../bspTree";
import { createLeaf } from "../bspTree";
import { normalizeIconId } from "../iconRegistry";
import type { WorkspaceState } from "./types";
import type { WorkspaceStoreContext } from "./store-context";
import {
  applyPaneSnapshots,
  captureTranscriptForPane,
  collectPaneSnapshots,
  hasAnotherPaneForSession,
  resolvePaneSessionId,
  stopPaneSessions,
} from "./helpers";
import { buildCanvasPanel, normalizeCanvasPanels } from "./canvas";
import { buildPaneIcons, buildPaneNames } from "./pane-metadata";

export function createPaneActions(
  ctx: WorkspaceStoreContext,
): Pick<
  WorkspaceState,
  | "splitActive"
  | "closePane"
  | "setActivePaneId"
  | "clearActivePaneFocus"
  | "setPaneSessionId"
  | "setPaneName"
  | "setPaneIcon"
  | "paneName"
  | "focusDirection"
  | "toggleZoom"
  | "applyPresetLayout"
  | "equalizeLayout"
  | "updateNodeRatio"
> {
  return {
    splitActive: (direction, newPaneName, opts) => {
      const sf = ctx.getActiveSurface();
      if (!sf) return;
      if (sf.layoutMode === "canvas") {
        ctx.get().createCanvasPanel(sf.id, {
          paneName: newPaneName,
          paneIcon: opts?.paneIcon,
          sessionId: opts?.sessionId ?? null,
        });
        return;
      }

      const target = sf.activePaneId ?? allLeafIds(sf.layout)[0];
      if (!target) return;
      const result = splitPane(sf.layout, target, direction);
      const layout = typeof opts?.sessionId === "string" && opts.sessionId
        ? setSessionId(result.tree, result.newPaneId, opts.sessionId)
        : result.tree;
      const paneIds = allLeafIds(layout);
      const trimmedName = (newPaneName ?? "").trim();

      ctx.updateSurface(sf.id, (surface) => ({
        ...surface,
        layout,
        paneNames: buildPaneNames(paneIds, {
          ...surface.paneNames,
          [result.newPaneId]: trimmedName || surface.paneNames[result.newPaneId] || `Pane ${paneIds.length}`,
        }),
        paneIcons: buildPaneIcons(paneIds, {
          ...surface.paneIcons,
          [result.newPaneId]: normalizeIconId(opts?.paneIcon ?? surface.paneIcons[result.newPaneId] ?? "terminal"),
        }),
        activePaneId: result.newPaneId,
      }));
      ctx.set({ zoomedPaneId: null });
    },

    closePane: (paneId, opts) => {
      let shouldStopSession = opts?.stopSession !== false;
      const shouldCaptureTranscript = opts?.captureTranscript !== false;
      if (shouldStopSession) {
        const { workspaces } = ctx.get();
        const sessionId = resolvePaneSessionId(workspaces, paneId);
        if (sessionId && hasAnotherPaneForSession(workspaces, sessionId, paneId)) {
          shouldStopSession = false;
        }
      }

      const sf = ctx.getActiveSurface();
      if (!sf) return;

      const pair = ctx.findWsSurfaceAndPane(paneId);
      if (pair && shouldCaptureTranscript) {
        captureTranscriptForPane({
          paneId,
          workspaceId: pair.ws.id,
          surfaceId: pair.sf.id,
          cwd: pair.ws.cwd,
          reason: "pane-close",
        });
      }

      const newTree = removePane(sf.layout, paneId);
      if (sf.layoutMode === "canvas") {
        if (newTree === null) {
          const leaf = createLeaf();
          ctx.updateSurface(sf.id, (surface) => normalizeCanvasPanels({
            ...surface,
            layout: leaf,
            paneNames: { [leaf.id]: "Pane 1" },
            paneIcons: { [leaf.id]: "terminal" },
            activePaneId: leaf.id,
            canvasPanels: [
              buildCanvasPanel({
                paneId: leaf.id,
                paneName: "Pane 1",
                index: 0,
                persisted: { icon: "terminal" },
                status: "idle",
              }),
            ],
          }));
          stopPaneSessions([paneId], shouldStopSession);
          return;
        }

        const remaining = allLeafIds(newTree);
        ctx.updateSurface(sf.id, (surface) => normalizeCanvasPanels({
          ...surface,
          layout: newTree,
          paneNames: buildPaneNames(remaining, surface.paneNames),
          paneIcons: buildPaneIcons(remaining, surface.paneIcons),
          activePaneId: surface.activePaneId === paneId ? remaining[0] ?? null : surface.activePaneId,
          canvasPanels: surface.canvasPanels.filter((panel) => panel.paneId !== paneId),
        }));
        stopPaneSessions([paneId], shouldStopSession);
        return;
      }

      if (newTree === null) {
        const leaf = createLeaf();
        ctx.updateSurface(sf.id, (surface) => ({
          ...surface,
          layout: leaf,
          paneNames: { [leaf.id]: "Pane 1" },
          paneIcons: { [leaf.id]: "terminal" },
          activePaneId: leaf.id,
        }));
        stopPaneSessions([paneId], shouldStopSession);
        return;
      }

      const remaining = allLeafIds(newTree);
      ctx.updateSurface(sf.id, (surface) => ({
        ...surface,
        layout: newTree,
        paneNames: buildPaneNames(remaining, surface.paneNames),
        paneIcons: buildPaneIcons(remaining, surface.paneIcons),
        activePaneId: surface.activePaneId === paneId ? remaining[0] ?? null : surface.activePaneId,
      }));
      if (ctx.get().zoomedPaneId === paneId) {
        ctx.set({ zoomedPaneId: null });
      }
      stopPaneSessions([paneId], shouldStopSession);
    },

    setActivePaneId: (paneId) => {
      const pair = ctx.findWsSurfaceAndPane(paneId);
      if (!pair) return;
      ctx.set((state) => ({
        activeWorkspaceId: pair.ws.id,
        ...ctx.activateWorkspaceBrowserState(state.workspaceBrowserState, pair.ws.id),
        workspaces: state.workspaces.map((workspace) => {
          if (workspace.id !== pair.ws.id) {
            return workspace;
          }

          return {
            ...workspace,
            activeSurfaceId: pair.sf.id,
            surfaces: workspace.surfaces.map((surface) => (
              surface.id === pair.sf.id ? { ...surface, activePaneId: paneId } : surface
            )),
          };
        }),
      }));
    },

    clearActivePaneFocus: (surfaceId) => {
      const targetSurface = surfaceId ? ctx.findWsAndSurface(surfaceId)?.sf : ctx.getActiveSurface();
      if (!targetSurface) return;
      ctx.updateSurface(targetSurface.id, (surface) => ({
        ...surface,
        activePaneId: null,
      }));
    },

    setPaneSessionId: (paneId, sessionId) => {
      const pair = ctx.findWsSurfaceAndPane(paneId);
      if (!pair) return;
      if (pair.sf.layoutMode === "canvas") {
        ctx.updateSurface(pair.sf.id, (surface) => normalizeCanvasPanels({
          ...surface,
          layout: setSessionId(surface.layout, paneId, sessionId),
          canvasPanels: surface.canvasPanels.map((panel) => (
            panel.paneId === paneId
              ? { ...panel, sessionId, status: "running", lastActivityAt: Date.now() }
              : panel
          )),
        }));
        return;
      }

      ctx.updateSurface(pair.sf.id, (surface) => ({
        ...surface,
        layout: setSessionId(surface.layout, paneId, sessionId),
      }));
    },

    setPaneName: (paneId, name) => {
      const pair = ctx.findWsSurfaceAndPane(paneId);
      if (!pair) return;
      const nextName = typeof name === "string" ? name.trim() : "";
      if (!nextName) return;
      if (pair.sf.layoutMode === "canvas") {
        ctx.updateSurface(pair.sf.id, (surface) => normalizeCanvasPanels({
          ...surface,
          paneNames: {
            ...surface.paneNames,
            [paneId]: nextName,
          },
          canvasPanels: surface.canvasPanels.map((panel) => (
            panel.paneId === paneId ? { ...panel, title: nextName } : panel
          )),
        }));
        return;
      }

      ctx.updateSurface(pair.sf.id, (surface) => ({
        ...surface,
        paneNames: {
          ...surface.paneNames,
          [paneId]: nextName,
        },
      }));
    },

    setPaneIcon: (paneId, icon) => {
      const pair = ctx.findWsSurfaceAndPane(paneId);
      if (!pair) return;
      const nextIcon = normalizeIconId(icon);

      if (pair.sf.layoutMode === "canvas") {
        ctx.updateSurface(pair.sf.id, (surface) => normalizeCanvasPanels({
          ...surface,
          paneIcons: {
            ...surface.paneIcons,
            [paneId]: nextIcon,
          },
          canvasPanels: surface.canvasPanels.map((panel) => (
            panel.paneId === paneId ? { ...panel, icon: nextIcon } : panel
          )),
        }));
        return;
      }

      ctx.updateSurface(pair.sf.id, (surface) => ({
        ...surface,
        paneIcons: {
          ...surface.paneIcons,
          [paneId]: nextIcon,
        },
      }));
    },

    paneName: (paneId) => {
      const pair = ctx.findWsSurfaceAndPane(paneId);
      if (!pair) return null;
      return pair.sf.paneNames[paneId] ?? null;
    },

    focusDirection: (direction) => {
      const sf = ctx.getActiveSurface();
      if (!sf || !sf.activePaneId) return;
      if (sf.layoutMode === "canvas") {
        const activePanel = sf.canvasPanels.find((panel) => panel.paneId === sf.activePaneId);
        if (!activePanel) return;
        const activeCenterX = activePanel.x + activePanel.width / 2;
        const activeCenterY = activePanel.y + activePanel.height / 2;

        const candidate = sf.canvasPanels
          .filter((panel) => panel.paneId !== sf.activePaneId)
          .map((panel) => {
            const centerX = panel.x + panel.width / 2;
            const centerY = panel.y + panel.height / 2;
            const dx = centerX - activeCenterX;
            const dy = centerY - activeCenterY;
            const isEligible = direction === "left"
              ? dx < 0
              : direction === "right"
                ? dx > 0
                : direction === "up"
                  ? dy < 0
                  : dy > 0;
            if (!isEligible) return null;
            const primary = direction === "left" || direction === "right" ? Math.abs(dx) : Math.abs(dy);
            const secondary = direction === "left" || direction === "right" ? Math.abs(dy) : Math.abs(dx);
            return { paneId: panel.paneId, score: primary * 10 + secondary };
          })
          .filter((entry): entry is { paneId: string; score: number } => Boolean(entry))
          .sort((a, b) => a.score - b.score)[0];

        if (candidate) {
          ctx.updateSurface(sf.id, (surface) => ({ ...surface, activePaneId: candidate.paneId }));
        }
        return;
      }

      const next = findAdjacentPane(sf.layout, sf.activePaneId, direction);
      if (next) {
        ctx.updateSurface(sf.id, (surface) => ({ ...surface, activePaneId: next }));
      }
    },

    toggleZoom: () => {
      const sf = ctx.getActiveSurface();
      if (!sf || sf.layoutMode === "canvas") return;
      const { zoomedPaneId } = ctx.get();
      if (zoomedPaneId) {
        ctx.set({ zoomedPaneId: null });
      } else if (sf.activePaneId) {
        ctx.set({ zoomedPaneId: sf.activePaneId });
      }
    },

    applyPresetLayout: (preset) => {
      const sf = ctx.getActiveSurface();
      if (!sf || sf.layoutMode === "canvas") return;

      const existingPanes = collectPaneSnapshots(sf.layout, sf.activePaneId);
      const layout = applyPaneSnapshots(buildPresetLayout(preset), existingPanes);
      const panes = allLeafIds(layout);

      ctx.updateSurface(sf.id, (surface) => ({
        ...surface,
        layout,
        paneNames: buildPaneNames(panes, surface.paneNames),
        paneIcons: buildPaneIcons(panes, surface.paneIcons),
        activePaneId: surface.activePaneId && panes.includes(surface.activePaneId)
          ? surface.activePaneId
          : panes[0] ?? null,
      }));

      const orphanedPaneIds = existingPanes
        .map((pane) => pane.id)
        .filter((paneId) => !panes.includes(paneId));

      stopPaneSessions(orphanedPaneIds);
      ctx.set({ zoomedPaneId: null });
    },

    equalizeLayout: () => {
      const sf = ctx.getActiveSurface();
      if (!sf || sf.layoutMode === "canvas") return;
      ctx.updateSurface(sf.id, (surface) => ({
        ...surface,
        layout: equalizeLayoutRatios(surface.layout),
      }));
    },

    updateNodeRatio: (paneId, newRatio) => {
      const sf = ctx.getActiveSurface();
      if (!sf || sf.layoutMode === "canvas") return;
      ctx.updateSurface(sf.id, (surface) => ({
        ...surface,
        layout: updateRatio(surface.layout, paneId, newRatio),
      }));
    },
  };
}
