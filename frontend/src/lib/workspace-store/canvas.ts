import type {
  CanvasPanel,
  CanvasPanelStatus,
  CanvasState,
  PersistedCanvasPanel,
  Surface,
} from "../types";
import { allLeafIds, findLeaf } from "../bspTree";
import { buildPaneIcons } from "./pane-metadata";
import { normalizeIconId } from "../iconRegistry";

export const CANVAS_MIN_ZOOM = 0.04;
export const CANVAS_MAX_ZOOM = 2.2;
const CANVAS_GRID_SIZE = 32;
const DEFAULT_CANVAS_PANEL_WIDTH = 760;
const DEFAULT_CANVAS_PANEL_HEIGHT = 440;
const CANVAS_AUTO_GAP_X = 48;
const CANVAS_AUTO_GAP_Y = 40;

export function snapCanvasCoord(value: number): number {
  return Math.round(value / CANVAS_GRID_SIZE) * CANVAS_GRID_SIZE;
}

export function createDefaultCanvasState(): CanvasState {
  return {
    panX: 0,
    panY: 0,
    zoomLevel: 1,
    previousView: null,
    focusRequestNonce: 0,
  };
}

export function sanitizeCanvasState(value: Partial<CanvasState> | undefined): CanvasState {
  const zoom = typeof value?.zoomLevel === "number" ? value.zoomLevel : 1;
  const previousView = value?.previousView && typeof value.previousView === "object"
    ? {
      panX: Number.isFinite(value.previousView.panX) ? value.previousView.panX : 0,
      panY: Number.isFinite(value.previousView.panY) ? value.previousView.panY : 0,
      zoomLevel: Number.isFinite(value.previousView.zoomLevel)
        ? Math.max(CANVAS_MIN_ZOOM, Math.min(CANVAS_MAX_ZOOM, value.previousView.zoomLevel))
        : 1,
    }
    : null;

  return {
    panX: Number.isFinite(value?.panX) ? Number(value?.panX) : 0,
    panY: Number.isFinite(value?.panY) ? Number(value?.panY) : 0,
    zoomLevel: Math.max(CANVAS_MIN_ZOOM, Math.min(CANVAS_MAX_ZOOM, Number.isFinite(zoom) ? zoom : 1)),
    previousView,
    focusRequestNonce: Number.isFinite(value?.focusRequestNonce)
      ? Math.max(0, Math.floor(Number(value?.focusRequestNonce)))
      : 0,
  };
}

function defaultCanvasPanelPosition(index: number): { x: number; y: number } {
  const col = index % 3;
  const row = Math.floor(index / 3);
  return {
    x: snapCanvasCoord(80 + col * (DEFAULT_CANVAS_PANEL_WIDTH + 48)),
    y: snapCanvasCoord(60 + row * (DEFAULT_CANVAS_PANEL_HEIGHT + 48)),
  };
}

export function buildCanvasPanel(opts: {
  paneId: string;
  paneName?: string;
  index: number;
  persisted?: Partial<PersistedCanvasPanel>;
  status?: CanvasPanelStatus;
}): CanvasPanel {
  const fallbackPos = defaultCanvasPanelPosition(opts.index);
  return {
    id: typeof opts.persisted?.id === "string" && opts.persisted.id
      ? opts.persisted.id
      : `cp_${opts.paneId}`,
    paneId: opts.paneId,
    title: opts.paneName ?? `Pane ${opts.index + 1}`,
    icon: normalizeIconId(opts.persisted?.icon),
    x: Number.isFinite(opts.persisted?.x) ? Number(opts.persisted?.x) : fallbackPos.x,
    y: Number.isFinite(opts.persisted?.y) ? Number(opts.persisted?.y) : fallbackPos.y,
    width: Number.isFinite(opts.persisted?.width)
      ? Math.max(320, Number(opts.persisted?.width))
      : DEFAULT_CANVAS_PANEL_WIDTH,
    height: Number.isFinite(opts.persisted?.height)
      ? Math.max(220, Number(opts.persisted?.height))
      : DEFAULT_CANVAS_PANEL_HEIGHT,
    status: opts.status ?? opts.persisted?.status ?? "running",
    sessionId: typeof opts.persisted?.sessionId === "string" ? opts.persisted.sessionId : null,
    panelType: opts.persisted?.panelType ?? "terminal",
    url: opts.persisted?.url ?? null,
    cwd: opts.persisted?.cwd ?? null,
    userRenamed: opts.persisted?.userRenamed ?? false,
    lastActivityAt: Number.isFinite(opts.persisted?.lastActivityAt)
      ? Number(opts.persisted?.lastActivityAt)
      : Date.now(),
  };
}

function isOverlappingPanel(
  panels: CanvasPanel[],
  candidate: { x: number; y: number; width: number; height: number },
): boolean {
  return panels.some((panel) => (
    candidate.x < panel.x + panel.width + 20
    && candidate.x + candidate.width + 20 > panel.x
    && candidate.y < panel.y + panel.height + 20
    && candidate.y + candidate.height + 20 > panel.y
  ));
}

export function findCanvasPlacement(surface: Surface, anchorPaneId?: string | null): { x: number; y: number } {
  const anchor = surface.canvasPanels.find((panel) => panel.paneId === anchorPaneId)
    ?? surface.canvasPanels.find((panel) => panel.paneId === surface.activePaneId)
    ?? surface.canvasPanels[surface.canvasPanels.length - 1];
  const stepX = DEFAULT_CANVAS_PANEL_WIDTH + CANVAS_AUTO_GAP_X;
  const stepY = DEFAULT_CANVAS_PANEL_HEIGHT + CANVAS_AUTO_GAP_Y;
  const baseX = anchor ? anchor.x : 80;
  const baseY = anchor ? anchor.y : 60;
  const candidateSize = { width: DEFAULT_CANVAS_PANEL_WIDTH, height: DEFAULT_CANVAS_PANEL_HEIGHT };

  for (let rowOffset = 0; rowOffset < 18; rowOffset += 1) {
    for (let colOffset = 1; colOffset < 18; colOffset += 1) {
      const candidate = {
        x: snapCanvasCoord(baseX + colOffset * stepX),
        y: snapCanvasCoord(baseY + rowOffset * stepY),
        ...candidateSize,
      };
      if (!isOverlappingPanel(surface.canvasPanels, candidate)) {
        return { x: candidate.x, y: candidate.y };
      }
    }
  }

  for (let rowOffset = 1; rowOffset < 18; rowOffset += 1) {
    for (let colOffset = 0; colOffset < 18; colOffset += 1) {
      const candidate = {
        x: snapCanvasCoord(baseX + colOffset * stepX),
        y: snapCanvasCoord(baseY - rowOffset * stepY),
        ...candidateSize,
      };
      if (!isOverlappingPanel(surface.canvasPanels, candidate)) {
        return { x: candidate.x, y: candidate.y };
      }
    }
  }

  return { x: snapCanvasCoord(baseX), y: snapCanvasCoord(baseY) };
}

export function normalizeCanvasPanels(surface: Surface): Surface {
  if (surface.layoutMode !== "canvas") {
    return {
      ...surface,
      paneIcons: buildPaneIcons(allLeafIds(surface.layout), surface.paneIcons),
      canvasState: sanitizeCanvasState(surface.canvasState),
      canvasPanels: [],
    };
  }

  const paneIds = allLeafIds(surface.layout);
  const panelByPaneId = new Map(surface.canvasPanels.map((panel) => [panel.paneId, panel]));
  const canvasPanels = paneIds.map((paneId, index) => {
    const existing = panelByPaneId.get(paneId);
    const base = buildCanvasPanel({
      paneId,
      paneName: surface.paneNames[paneId],
      index,
      persisted: existing ?? undefined,
      status: existing?.status,
    });

    return {
      ...base,
      title: surface.paneNames[paneId] ?? base.title,
      icon: surface.paneIcons?.[paneId] ?? existing?.icon ?? base.icon,
      status: existing?.status ?? "running",
      sessionId: existing?.sessionId ?? findLeaf(surface.layout, paneId)?.sessionId ?? null,
      panelType: existing?.panelType ?? base.panelType,
      url: existing?.url ?? base.url,
    };
  });

  const activePaneId = surface.activePaneId && paneIds.includes(surface.activePaneId)
    ? surface.activePaneId
    : paneIds[0] ?? null;

  return {
    ...surface,
    activePaneId,
    paneIcons: buildPaneIcons(paneIds, surface.paneIcons),
    canvasState: sanitizeCanvasState(surface.canvasState),
    canvasPanels,
  };
}
