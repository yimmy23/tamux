export type InlineMonitorSnapshot = {
    cpu?: { usagePercent?: number };
    memory?: { usedBytes?: number; totalBytes?: number };
    gpus?: Array<{
        memoryUsedMB?: number;
        memoryTotalMB?: number;
        utilizationPercent?: number;
    }>;
};

export type InlineMonitorStats = {
    cpu: number;
    memPercent: number;
    memUsedGB: string;
    memTotalGB: string;
    gpu: number | null;
};

export function buildInlineMonitorStats(snap: InlineMonitorSnapshot): InlineMonitorStats {
    const usedBytes = snap.memory?.usedBytes ?? 0;
    const totalBytes = snap.memory?.totalBytes && snap.memory.totalBytes > 0
        ? snap.memory.totalBytes
        : 1;
    const gpu = snap.gpus?.[0]?.utilizationPercent;

    return {
        cpu: snap.cpu?.usagePercent ?? 0,
        memPercent: (usedBytes / totalBytes) * 100,
        memUsedGB: (usedBytes / (1024 * 1024 * 1024)).toFixed(1),
        memTotalGB: (totalBytes / (1024 * 1024 * 1024)).toFixed(0),
        gpu: Number.isFinite(gpu) ? gpu ?? null : null,
    };
}
