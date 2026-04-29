import { afterEach, describe, expect, it, vi } from "vitest";
import { destroyStatusStore, hydrateStatusStore } from "./statusStore";

const flushMicrotasks = () => new Promise<void>((resolve) => queueMicrotask(resolve));

describe("statusStore polling", () => {
  afterEach(() => {
    destroyStatusStore();
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it("does not queue overlapping daemon status polls", async () => {
    vi.useFakeTimers();
    let resolveStatus: (value: unknown) => void = () => {};
    const pendingStatus = new Promise((resolve) => {
      resolveStatus = resolve;
    });
    const agentGetStatus = vi.fn(() => pendingStatus);

    vi.stubGlobal("window", {
      zorai: { agentGetStatus },
    });

    hydrateStatusStore();

    expect(agentGetStatus).toHaveBeenCalledTimes(1);

    await vi.advanceTimersByTimeAsync(30_000);

    expect(agentGetStatus).toHaveBeenCalledTimes(1);

    resolveStatus({
      activity: "idle",
      active_thread_id: null,
      active_goal_run_id: null,
      active_goal_run_title: null,
      provider_health: {},
      gateway_statuses: {},
      recent_actions: [],
      diagnostics: {},
    });
    await flushMicrotasks();

    await vi.advanceTimersByTimeAsync(10_000);

    expect(agentGetStatus).toHaveBeenCalledTimes(2);
  });
});
