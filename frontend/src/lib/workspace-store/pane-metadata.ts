import type { PaneId } from "../types";
import { normalizeIconId } from "../iconRegistry";

export function buildPaneNames(
  paneIds: PaneId[],
  existing?: Record<PaneId, string>,
): Record<PaneId, string> {
  const names: Record<PaneId, string> = {};
  let nextIndex = 1;

  for (const paneId of paneIds) {
    const candidate = existing?.[paneId]?.trim();
    if (candidate) {
      names[paneId] = candidate;
      continue;
    }

    names[paneId] = `Pane ${nextIndex}`;
    nextIndex += 1;
  }

  return names;
}

export function buildPaneIcons(
  paneIds: PaneId[],
  existing?: Record<PaneId, string>,
): Record<PaneId, string> {
  const icons: Record<PaneId, string> = {};
  for (const paneId of paneIds) {
    icons[paneId] = normalizeIconId(existing?.[paneId]);
  }
  return icons;
}
