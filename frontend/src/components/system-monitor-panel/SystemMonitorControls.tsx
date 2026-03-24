import { Badge, Input, cn, fieldClassName } from "../ui";
import { INTERVAL_OPTIONS } from "./shared";

export function SystemMonitorControls({
  processQuery,
  setProcessQuery,
  intervalMs,
  setIntervalMs,
  processLimit,
  setProcessLimit,
  visibleProcesses,
  timestampLabel,
}: {
  processQuery: string;
  setProcessQuery: (value: string) => void;
  intervalMs: number;
  setIntervalMs: (value: number) => void;
  processLimit: number;
  setProcessLimit: (value: number) => void;
  visibleProcesses: number;
  timestampLabel: string;
}) {
  return (
    <div className="grid items-center gap-[var(--space-3)] border-b border-[var(--border-subtle)] bg-[var(--panel)]/45 px-[var(--space-5)] py-[var(--space-4)] xl:grid-cols-[1.2fr_1.2fr_1fr_auto_auto]">
      <Input
        type="text"
        value={processQuery}
        onChange={(event) => setProcessQuery(event.target.value)}
        placeholder="Filter processes by pid, name, or command..."
      />
      <select
        value={intervalMs}
        onChange={(event) => setIntervalMs(Number(event.target.value))}
        className={cn(fieldClassName, "appearance-none")}
      >
        {INTERVAL_OPTIONS.map((value) => (
          <option key={value} value={value}>
            {value < 1000 ? `${value} ms` : `${value / 1000} s`}
          </option>
        ))}
      </select>
      <select
        value={processLimit}
        onChange={(event) => setProcessLimit(Number(event.target.value))}
        className={cn(fieldClassName, "appearance-none")}
      >
        {[12, 24, 36, 48, 64].map((value) => (
          <option key={value} value={value}>
            Top {value} processes
          </option>
        ))}
      </select>
      <Badge variant="default">{visibleProcesses} visible</Badge>
      <Badge variant="timeline">{timestampLabel}</Badge>
    </div>
  );
}
