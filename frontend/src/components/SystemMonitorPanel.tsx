import { useEffect, useMemo, useState, type CSSProperties } from "react";
import { cn, overlayClassName, panelSurfaceClassName } from "./ui";
import { useWorkspaceStore } from "../lib/workspaceStore";
import { SystemMonitorContent } from "./system-monitor-panel/SystemMonitorContent";
import { SystemMonitorControls } from "./system-monitor-panel/SystemMonitorControls";
import { SystemMonitorHeader } from "./system-monitor-panel/SystemMonitorHeader";
import type { MonitorSnapshot } from "./system-monitor-panel/shared";

type SystemMonitorPanelProps = {
  style?: CSSProperties;
  className?: string;
};

export function SystemMonitorPanel({ style, className }: SystemMonitorPanelProps = {}) {
  const open = useWorkspaceStore((s) => s.systemMonitorOpen);
  const toggle = useWorkspaceStore((s) => s.toggleSystemMonitor);
  const [snapshot, setSnapshot] = useState<MonitorSnapshot | null>(null);
  const [intervalMs, setIntervalMs] = useState(1000);
  const [processLimit, setProcessLimit] = useState(24);
  const [processQuery, setProcessQuery] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;

    let active = true;
    let timeoutId: number | undefined;

    const fetchSnapshot = async () => {
      const amux = (window as any).tamux ?? (window as any).amux;
      if (!amux?.getSystemMonitorSnapshot) {
        if (active) {
          setError("Native system monitoring is available in the desktop runtime.");
          setSnapshot(null);
        }
        return;
      }

      if (active && !snapshot) {
        setLoading(true);
      }

      try {
        const next = await amux.getSystemMonitorSnapshot({ processLimit });
        if (!active) return;
        setSnapshot(next);
        setError(null);
      } catch (fetchError) {
        if (!active) return;
        setError(fetchError instanceof Error ? fetchError.message : "Unable to fetch system metrics.");
      } finally {
        if (active) {
          setLoading(false);
          timeoutId = window.setTimeout(fetchSnapshot, intervalMs);
        }
      }
    };

    void fetchSnapshot();

    return () => {
      active = false;
      if (timeoutId !== undefined) {
        window.clearTimeout(timeoutId);
      }
    };
  }, [open, intervalMs, processLimit]);

  const filteredProcesses = useMemo(() => {
    const lower = processQuery.trim().toLowerCase();
    if (!lower) {
      return snapshot?.processes ?? [];
    }

    return (snapshot?.processes ?? []).filter(
      (processEntry) =>
        processEntry.name.toLowerCase().includes(lower) ||
        processEntry.command.toLowerCase().includes(lower) ||
        String(processEntry.pid).includes(lower)
    );
  }, [processQuery, snapshot]);

  if (!open) return null;

  return (
    <div
      onClick={toggle}
      style={style}
      className={cn(
        overlayClassName,
        "fixed inset-0 z-[945] flex items-start justify-center px-[2vw] py-[4vh]",
        className
      )}
    >
      <div
        onClick={(event) => event.stopPropagation()}
        className={cn(
          panelSurfaceClassName,
          "grid max-h-[88vh] w-[min(1440px,96vw)] grid-rows-[auto_auto_1fr] overflow-hidden rounded-[var(--radius-xl)] border-[var(--border-strong)] bg-[var(--card)]"
        )}
      >
        <SystemMonitorHeader
          hostname={snapshot?.hostname ?? "pending"}
          platform={snapshot?.platform ?? "pending"}
          intervalMs={intervalMs}
          uptimeSeconds={snapshot?.uptimeSeconds ?? null}
          clear={() => setSnapshot(null)}
          close={toggle}
        />

        <SystemMonitorControls
          processQuery={processQuery}
          setProcessQuery={setProcessQuery}
          intervalMs={intervalMs}
          setIntervalMs={setIntervalMs}
          processLimit={processLimit}
          setProcessLimit={setProcessLimit}
          visibleProcesses={filteredProcesses.length}
          timestampLabel={snapshot ? new Date(snapshot.timestamp).toLocaleTimeString() : loading ? "sampling" : "idle"}
        />

        <SystemMonitorContent
          snapshot={snapshot}
          loading={loading}
          error={error}
          filteredProcesses={filteredProcesses}
        />
      </div>
    </div>
  );
}
