import { describe, expect, it } from "vitest";
import { buildInlineMonitorStats } from "./inlineSystemMonitorStats";

describe("buildInlineMonitorStats", () => {
  it("uses CPU percent, memory percent, and GPU utilization when a GPU exists", () => {
    const stats = buildInlineMonitorStats({
      cpu: { usagePercent: 12.4 },
      memory: {
        usedBytes: 8 * 1024 * 1024 * 1024,
        totalBytes: 32 * 1024 * 1024 * 1024,
      },
      gpus: [
        {
          memoryUsedMB: 4000,
          memoryTotalMB: 8000,
          utilizationPercent: 47.6,
        },
      ],
    });

    expect(stats).toEqual({
      cpu: 12.4,
      memPercent: 25,
      memUsedGB: "8.0",
      memTotalGB: "32",
      gpu: 47.6,
    });
  });

  it("omits GPU stats when no GPU exists", () => {
    const stats = buildInlineMonitorStats({
      cpu: { usagePercent: 4 },
      memory: {
        usedBytes: 512 * 1024 * 1024,
        totalBytes: 2 * 1024 * 1024 * 1024,
      },
      gpus: [],
    });

    expect(stats.gpu).toBeNull();
  });
});
