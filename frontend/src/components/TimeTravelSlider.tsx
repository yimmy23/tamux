import { useState, useEffect, useCallback } from "react";
import { useWorkspaceStore } from "../lib/workspaceStore";
import { useAgentMissionStore } from "../lib/agentMissionStore";
import { getTerminalController } from "../lib/terminalRegistry";
import { TimeTravelContent } from "./time-travel-slider/TimeTravelContent";
import { TimeTravelHeader } from "./time-travel-slider/TimeTravelHeader";
import type { SnapshotEntry, TimeTravelSliderProps } from "./time-travel-slider/shared";

function toSnapshotEntry(snapshot: any): SnapshotEntry {
  return {
    snapshot_id: snapshot.snapshotId ?? snapshot.snapshot_id,
    label: snapshot.label ?? `Snapshot ${String(snapshot.snapshotId ?? snapshot.snapshot_id ?? "").slice(0, 8)}`,
    command: snapshot.command ?? null,
    created_at: snapshot.createdAt ?? snapshot.created_at ?? Date.now(),
    status: snapshot.status ?? "ready",
    workspace_id: snapshot.workspaceId ?? snapshot.workspace_id ?? null,
  };
}

/**
 * Time-Travel Scrubbing Slider — floating toolbar for browsing
 * and restoring daemon-side workspace snapshots.
 */
export function TimeTravelSlider({ style, className }: TimeTravelSliderProps = {}) {
  const open = useWorkspaceStore((s) => s.timeTravelOpen);
  const toggle = useWorkspaceStore((s) => s.toggleTimeTravel);
  const activePaneId = useWorkspaceStore((s) => s.activePaneId());
  const missionSnapshots = useAgentMissionStore((s) => s.snapshots);

  const [snapshots, setSnapshots] = useState<SnapshotEntry[]>([]);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [isRestoring, setIsRestoring] = useState(false);
  const [confirmRestore, setConfirmRestore] = useState(false);

  const refreshSnapshots = useCallback(async () => {
    const controller = getTerminalController(activePaneId);
    try {
      if (controller) {
        const ok = await controller.listSnapshots(null);
        if (ok) return;
      }
    } catch {
      // Fall back to the persisted snapshot index when no live pane bridge exists.
    }

    const amux = (window as any).tamux ?? (window as any).amux;
    const rows = await amux?.dbListSnapshotIndex?.(null);
    if (Array.isArray(rows)) {
      setSnapshots(rows.map((snapshot: any) => toSnapshotEntry(snapshot)));
    }
  }, [activePaneId]);

  // Fetch snapshots from daemon when panel opens
  useEffect(() => {
    if (!open) return;

    // Use mission store snapshots if available
    if (missionSnapshots.length > 0) {
      setSnapshots(
        missionSnapshots.map((s) => toSnapshotEntry(s))
      );
      setSelectedIndex(0);
    } else {
      void refreshSnapshots();
    }
  }, [open, missionSnapshots, refreshSnapshots]);

  const handleRestore = useCallback(async () => {
    const target = snapshots[selectedIndex];
    if (!target || isRestoring) return;

    if (!confirmRestore) {
      setConfirmRestore(true);
      return;
    }

    setIsRestoring(true);
    setConfirmRestore(false);

    const controller = getTerminalController(activePaneId);
    if (controller) {
      await controller.restoreSnapshot(target.snapshot_id);
    }

    setIsRestoring(false);
  }, [snapshots, selectedIndex, isRestoring, confirmRestore, activePaneId]);

  useEffect(() => {
    if (!open) {
      setConfirmRestore(false);
    }
  }, [open]);

  if (!open) return null;

  return (
    <div
      style={{
        position: "fixed",
        bottom: 12,
        left: "50%",
        transform: "translateX(-50%)",
        zIndex: 200,
        minWidth: 520,
        maxWidth: 720,
        background: "var(--bg-secondary)",
        border: "1px solid var(--glass-border)",
        borderRadius: 0,
        padding: "14px 18px",
        boxShadow: "none",
        backdropFilter: "none",
        ...(style ?? {}),
      }}
      className={className ? `amux-shell-card ${className}` : "amux-shell-card"}
    >
      <TimeTravelHeader
        snapshotCount={snapshots.length}
        onRefresh={() => {
          void refreshSnapshots();
        }}
        toggle={toggle}
      />
      <TimeTravelContent
        snapshots={snapshots}
        selectedIndex={selectedIndex}
        setSelectedIndex={setSelectedIndex}
        confirmRestore={confirmRestore}
        setConfirmRestore={setConfirmRestore}
        isRestoring={isRestoring}
        handleRestore={() => {
          void handleRestore();
        }}
      />
    </div>
  );
}
