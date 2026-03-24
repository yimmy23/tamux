import type { MonitorSnapshot } from "./shared";
import { Badge, cn, panelSurfaceClassName } from "../ui";
import { formatBytes, formatMegabytes, percentage } from "./shared";

export function SystemMonitorContent({
  snapshot,
  loading,
  error,
  filteredProcesses,
}: {
  snapshot: MonitorSnapshot | null;
  loading: boolean;
  error: string | null;
  filteredProcesses: MonitorSnapshot["processes"];
}) {
  const memoryUsagePercent = snapshot
    ? percentage(snapshot.memory.usedBytes, snapshot.memory.totalBytes)
    : 0;
  const swapUsagePercent =
    snapshot && snapshot.memory.swapTotalBytes
      ? percentage(snapshot.memory.swapUsedBytes ?? 0, snapshot.memory.swapTotalBytes)
      : 0;

  return (
    <div className="grid min-h-0 grid-cols-[26rem_minmax(0,1fr)]">
      <div className="grid content-start gap-[var(--space-3)] overflow-auto border-r border-[var(--border-subtle)] bg-[var(--panel)]/20 p-[var(--space-4)]">
        <ResourceCard
          title="CPU"
          value={snapshot ? `${snapshot.cpu.usagePercent.toFixed(1)}%` : loading ? "sampling" : "n/a"}
          subtitle={snapshot ? `${snapshot.cpu.coreCount} cores` : ""}
          meterValue={snapshot?.cpu.usagePercent ?? 0}
          detail={snapshot ? `${snapshot.cpu.model} · load ${snapshot.cpu.loadAverage.join(" / ")}` : error ?? "Waiting for metrics..."}
          tone="accent"
        />
        <ResourceCard
          title="Memory"
          value={snapshot ? `${memoryUsagePercent.toFixed(1)}%` : loading ? "sampling" : "n/a"}
          subtitle={
            snapshot
              ? `${formatBytes(snapshot.memory.usedBytes)} / ${formatBytes(snapshot.memory.totalBytes)}`
              : ""
          }
          meterValue={memoryUsagePercent}
          detail={snapshot ? `${formatBytes(snapshot.memory.freeBytes)} free` : error ?? "Waiting for metrics..."}
          tone="agent"
        />
        {snapshot?.memory.swapTotalBytes ? (
          <ResourceCard
            title="Swap"
            value={`${swapUsagePercent.toFixed(1)}%`}
            subtitle={`${formatBytes(snapshot.memory.swapUsedBytes ?? 0)} / ${formatBytes(snapshot.memory.swapTotalBytes)}`}
            meterValue={swapUsagePercent}
            detail={`${formatBytes(snapshot.memory.swapFreeBytes ?? 0)} free`}
            tone="warning"
          />
        ) : null}
        {snapshot?.gpus.length ? (
          snapshot.gpus.map((gpu) => (
            <ResourceCard
              key={gpu.id}
              title={gpu.name}
              value={`${gpu.utilizationPercent.toFixed(0)}%`}
              subtitle={`${formatMegabytes(gpu.memoryUsedMB)} / ${formatMegabytes(gpu.memoryTotalMB)} VRAM`}
              meterValue={percentage(gpu.memoryUsedMB, gpu.memoryTotalMB)}
              detail="GPU utilization and VRAM usage"
              tone="timeline"
            />
          ))
        ) : (
          <ResourceCard
            title="GPU"
            value="n/a"
            subtitle="No GPU telemetry"
            meterValue={0}
            detail="nvidia-smi was not available or no supported discrete GPU was detected."
            tone="default"
          />
        )}
      </div>

      <div className="grid min-h-0 grid-rows-[auto_1fr]">
        <div className="flex flex-wrap items-center justify-between gap-[var(--space-3)] border-b border-[var(--border-subtle)] bg-[var(--card)]/80 px-[var(--space-4)] py-[var(--space-4)]">
          <div>
            <div className="text-[var(--text-base)] font-semibold text-[var(--text-primary)]">
              Process Table
            </div>
            <div className="mt-[var(--space-1)] text-[var(--text-xs)] text-[var(--text-secondary)]">
              Sorted by CPU usage from the native runtime snapshot.
            </div>
          </div>
          {error ? <Badge variant="danger">{error}</Badge> : null}
        </div>
        <div className="overflow-auto bg-[var(--panel)]/15">
          {loading && !snapshot ? (
            <EmptyState message="Sampling host telemetry..." />
          ) : filteredProcesses.length === 0 ? (
            <EmptyState message="No process metrics available." />
          ) : (
            <table className="w-full border-collapse text-left text-[var(--text-xs)] text-[var(--text-primary)]">
              <thead className="sticky top-0 z-[1] bg-[var(--panel)]/95 backdrop-blur-[var(--panel-blur)]">
                <tr className="border-b border-[var(--border-subtle)] text-[var(--text-muted)]">
                  <th className="px-[var(--space-3)] py-[var(--space-3)] font-medium">PID</th>
                  <th className="px-[var(--space-3)] py-[var(--space-3)] font-medium">Process</th>
                  <th className="px-[var(--space-3)] py-[var(--space-3)] font-medium">CPU</th>
                  <th className="px-[var(--space-3)] py-[var(--space-3)] font-medium">Memory</th>
                  <th className="px-[var(--space-3)] py-[var(--space-3)] font-medium">State</th>
                  <th className="px-[var(--space-3)] py-[var(--space-3)] font-medium">Command</th>
                </tr>
              </thead>
              <tbody>
                {filteredProcesses.map((processEntry) => (
                  <tr
                    key={`${processEntry.pid}-${processEntry.command}`}
                    className="border-b border-[var(--border-subtle)]/70 transition-colors hover:bg-[var(--muted)]/60"
                  >
                    <td className="px-[var(--space-3)] py-[var(--space-3)]">{processEntry.pid}</td>
                    <td className="px-[var(--space-3)] py-[var(--space-3)]">
                      <div className="grid gap-[var(--space-1)]">
                        <span className="font-semibold text-[var(--text-primary)]">{processEntry.name}</span>
                        <span className="text-[10px] text-[var(--text-muted)]">
                          {processEntry.command.split(" ")[0]}
                        </span>
                      </div>
                    </td>
                    <td className="px-[var(--space-3)] py-[var(--space-3)]">
                      {processEntry.cpuPercent === null ? "n/a" : `${processEntry.cpuPercent.toFixed(1)}%`}
                    </td>
                    <td className="px-[var(--space-3)] py-[var(--space-3)]">
                      {formatBytes(processEntry.memoryBytes)}
                    </td>
                    <td className="px-[var(--space-3)] py-[var(--space-3)]">{processEntry.state}</td>
                    <td className="max-w-0 px-[var(--space-3)] py-[var(--space-3)]">
                      <div
                        title={processEntry.command}
                        className="truncate font-mono text-[var(--text-xs)] text-[var(--text-secondary)]"
                      >
                        {processEntry.command}
                      </div>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
      </div>
    </div>
  );
}

function ResourceCard({
  title,
  value,
  subtitle,
  detail,
  meterValue,
  tone,
}: {
  title: string;
  value: string;
  subtitle: string;
  detail: string;
  meterValue: number;
  tone: "default" | "accent" | "agent" | "warning" | "timeline";
}) {
  const accentClasses =
    tone === "accent"
      ? "border-[var(--accent-border)]/80"
      : tone === "agent"
        ? "border-[var(--agent-border)]/80"
        : tone === "warning"
          ? "border-[var(--warning-border)]/80"
          : tone === "timeline"
            ? "border-[var(--timeline-border)]/80"
            : "";

  return (
    <div
      className={cn(
        panelSurfaceClassName,
        "grid gap-[var(--space-3)] rounded-[var(--radius-lg)] bg-[var(--card)]/80 px-[var(--space-4)] py-[var(--space-4)] shadow-none",
        accentClasses
      )}
    >
      <div className="flex items-start justify-between gap-[var(--space-3)]">
        <div className="grid gap-[var(--space-1)]">
          <span className="text-[var(--text-xs)] font-medium uppercase tracking-[0.08em] text-[var(--text-muted)]">
            {title}
          </span>
          <span className="text-[1.25rem] font-semibold text-[var(--text-primary)]">{value}</span>
        </div>
        {subtitle ? <Badge variant="default">{subtitle}</Badge> : null}
      </div>
      <div className="overflow-hidden rounded-[var(--radius-full)] border border-[var(--border-subtle)] bg-[var(--muted)]">
        <div
          className="h-[0.45rem] bg-[var(--accent)] transition-[width] duration-300 ease-out"
          style={{ width: `${Math.max(0, Math.min(100, meterValue))}%` }}
        />
      </div>
      <span className="text-[var(--text-xs)] leading-6 text-[var(--text-secondary)]">{detail}</span>
    </div>
  );
}

function EmptyState({ message }: { message: string }) {
  return (
    <div className="flex min-h-[14rem] items-center justify-center px-[var(--space-6)] text-center text-[var(--text-sm)] text-[var(--text-secondary)]">
      {message}
    </div>
  );
}
