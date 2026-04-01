import {
  buildWelesHealthPresentation,
  type WelesHealthState,
} from "./welesHealthPresentation";

function expect(condition: boolean, message: string): void {
  if (!condition) {
    throw new Error(message);
  }
}

const healthy: WelesHealthState = {
  state: "healthy",
  checkedAt: 100,
};

const degraded: WelesHealthState = {
  state: "degraded",
  reason: "WELES review unavailable for guarded actions",
  checkedAt: 101,
};

expect(
  buildWelesHealthPresentation(healthy) === null,
  "healthy WELES state should not render a banner",
);

const degradedPresentation = buildWelesHealthPresentation(degraded);
expect(
  degradedPresentation?.title === "WELES degraded",
  "degraded WELES state should render the degraded title",
);
expect(
  Boolean(degradedPresentation?.detail.includes("review unavailable")),
  "degraded WELES state should preserve the daemon reason",
);
