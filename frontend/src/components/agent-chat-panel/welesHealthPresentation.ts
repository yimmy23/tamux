export type WelesHealthState = {
  state: string;
  reason?: string;
  checkedAt: number;
};

export type WelesHealthPresentation = {
  title: "WELES degraded";
  detail: string;
};

export function buildWelesHealthPresentation(
  health: WelesHealthState | null | undefined,
): WelesHealthPresentation | null {
  if (!health || health.state !== "degraded") {
    return null;
  }

  return {
    title: "WELES degraded",
    detail: health.reason?.trim() || "Daemon vitality checks require attention.",
  };
}
