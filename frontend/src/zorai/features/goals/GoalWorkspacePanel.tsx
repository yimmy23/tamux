import { useEffect, useMemo, useState, type ReactNode } from "react";
import { getDataDir, listPersistedDir } from "@/lib/persistence";
import { useThreadFilePreview } from "../threads/ThreadFilePreviewContext";
import {
  controlGoalRun,
  formatGoalRunStatus,
  isGoalRunActive,
  summarizeGoalRunStep,
  type GoalRun,
  type GoalRunControlAction,
} from "@/lib/goalRuns";
import {
  buildGoalWorkspaceModel,
  type GoalProjectionFile,
  type GoalWorkspaceMode,
  type GoalWorkspaceRow,
  type GoalWorkspaceSection,
} from "./goalWorkspaceModel";

export function GoalWorkspacePanel({
  run,
  onRefresh,
  onMessage,
  onOpenThread,
}: {
  run: GoalRun | null;
  onRefresh: () => Promise<void>;
  onMessage: (message: string) => void;
  onOpenThread?: (threadId: string) => void | Promise<void>;
}) {
  const [mode, setMode] = useState<GoalWorkspaceMode>("dossier");
  const [selectedStepId, setSelectedStepId] = useState<string | null>(null);
  const [selectedCenterIndex, setSelectedCenterIndex] = useState(0);
  const [promptExpanded, setPromptExpanded] = useState(false);
  const [expandedStepIds, setExpandedStepIds] = useState<Set<string>>(() => new Set());
  const [projectionFiles, setProjectionFiles] = useState<GoalProjectionFile[]>([]);
  const { openThreadFilePreview } = useThreadFilePreview();

  useEffect(() => {
    let cancelled = false;
    if (!run?.id) {
      setProjectionFiles([]);
      return () => {
        cancelled = true;
      };
    }

    loadGoalProjectionFiles(run.id).then((files) => {
      if (!cancelled) setProjectionFiles(files);
    });
    return () => {
      cancelled = true;
    };
  }, [run?.id]);

  const model = useMemo(() => run ? buildGoalWorkspaceModel(run, {
    mode,
    selectedStepId,
    selectedCenterIndex,
    promptExpanded,
    expandedStepIds,
    projectionFiles,
  }) : null, [expandedStepIds, mode, projectionFiles, promptExpanded, run, selectedCenterIndex, selectedStepId]);

  const activeStepIndex = typeof run?.current_step_index === "number" ? run.current_step_index : null;

  const control = async (action: GoalRunControlAction) => {
    if (!run) return;
    const ok = await controlGoalRun(run.id, action, activeStepIndex);
    onMessage(ok ? `Goal ${action.replace(/_/g, " ")} requested.` : "Goal action failed.");
    await onRefresh();
  };

  if (!run || !model) {
    return (
      <div className="zorai-goal-workspace-shell">
        <div className="zorai-tui-pane">
          <div className="zorai-tui-pane__body zorai-empty-state">Select a goal run to open the TUI-style workspace.</div>
        </div>
      </div>
    );
  }

  const handlePlanRowClick = (row: GoalWorkspaceRow) => {
    if (handleTargetRow(row)) return;
    if (row.id === "goal-prompt") {
      setPromptExpanded((current) => !current);
      return;
    }
    if (row.id.startsWith("step-")) {
      const stepId = row.id.slice("step-".length);
      setSelectedStepId(stepId);
      setExpandedStepIds((current) => {
        const next = new Set(current);
        if (next.has(stepId)) next.delete(stepId);
        else next.add(stepId);
        return next;
      });
    }
  };

  const handleTargetRow = (row: GoalWorkspaceRow) => {
    if (row.targetThreadId) {
      void onOpenThread?.(row.targetThreadId);
      return true;
    }
    if (row.targetFilePath) {
      openThreadFilePreview({
        path: row.targetFilePath,
        kind: "artifact",
        source: "goal",
        goalRunId: run?.id ?? null,
        isText: true,
        updatedAt: Date.now(),
      });
      return true;
    }
    return false;
  };

  return (
    <div className="zorai-goal-workspace-shell" aria-label="Goal Mission Control">
      <section className="zorai-tui-pane zorai-goal-summary-pane">
        <div className="zorai-tui-pane__title">{model.summaryTitle}</div>
        <div className="zorai-goal-mode-tabs" aria-label="Goal workspace modes">
          {model.tabs.map((tab) => (
            <button
              type="button"
              key={tab.id}
              className={["zorai-goal-tab", tab.active ? "zorai-goal-tab--active" : ""].filter(Boolean).join(" ")}
              onClick={() => {
                setMode(tab.id);
                setSelectedCenterIndex(0);
              }}
            >
              {tab.label}
            </button>
          ))}
        </div>
      </section>

      <div className="zorai-goal-workspace-grid">
        <Pane title={model.planTitle} className="zorai-goal-plan-pane">
          <RowList rows={model.planRows} onRowClick={handlePlanRowClick} />
        </Pane>

        <Pane title={model.centerTitle}>
          <RowList
            rows={model.centerRows}
            onRowClick={(row, index) => {
              setSelectedCenterIndex(index);
              handleTargetRow(row);
            }}
          />
        </Pane>

        <Pane title={model.detailTitle}>
          <SectionList sections={model.detailSections} onRowClick={handleTargetRow} />
        </Pane>
      </div>

      <section className="zorai-tui-pane zorai-step-actions-pane">
        <div className="zorai-tui-pane__title">{model.footerTitle}</div>
        <div className="zorai-step-actions">
          {model.footerSegments.map((segment) => (
            <button
              key={segment.id}
              type="button"
              className={["zorai-step-action", `zorai-row-tone--${segment.tone ?? "normal"}`].join(" ")}
              onClick={() => {
                if (segment.id === "toggle") void control(run.status === "paused" ? "resume" : "pause");
                if (segment.id === "retry") void control("retry_step");
                if (segment.id === "rerun") void control("rerun_from_step");
                if (segment.id === "refresh") void onRefresh();
              }}
              disabled={
                (segment.id === "toggle" && !isGoalRunActive(run))
                || ((segment.id === "retry" || segment.id === "rerun") && activeStepIndex === null)
                || segment.id === "actions"
                || segment.id === "step"
                || segment.id === "prompt"
              }
            >
              {segment.text}
            </button>
          ))}
        </div>
      </section>

      <div className="zorai-goal-workspace-status">
        <span>{formatGoalRunStatus(run.status)}</span>
        <span>{summarizeGoalRunStep(run)}</span>
      </div>
    </div>
  );
}

function Pane({
  title,
  className,
  children,
}: {
  title: string;
  className?: string;
  children: ReactNode;
}) {
  return (
    <section className={["zorai-tui-pane", className ?? ""].filter(Boolean).join(" ")}>
      <div className="zorai-tui-pane__title">{title}</div>
      <div className="zorai-tui-pane__body">{children}</div>
    </section>
  );
}

function RowList({
  rows,
  onRowClick,
}: {
  rows: GoalWorkspaceRow[];
  onRowClick?: (row: GoalWorkspaceRow, index: number) => void;
}) {
  return (
    <div className="zorai-tui-row-list">
      {rows.map((row, index) => (
        <button
          key={`${row.id}-${index}`}
          type="button"
          className={[
            "zorai-tui-row",
            `zorai-row-tone--${row.tone ?? "normal"}`,
            row.selected ? "zorai-tui-row--selected" : "",
          ].filter(Boolean).join(" ")}
          style={{ paddingLeft: `${8 + (row.depth ?? 0) * 18}px` }}
          onClick={() => onRowClick?.(row, index)}
        >
          {row.text}
        </button>
      ))}
    </div>
  );
}

function SectionList({
  sections,
  onRowClick,
}: {
  sections: GoalWorkspaceSection[];
  onRowClick?: (row: GoalWorkspaceRow, index: number) => void;
}) {
  return (
    <div className="zorai-goal-detail-sections">
      {sections.map((section) => (
        <section key={section.title} className="zorai-goal-detail-section">
          <h3>{section.title}</h3>
          <RowList rows={section.rows} onRowClick={onRowClick} />
        </section>
      ))}
    </div>
  );
}

async function loadGoalProjectionFiles(goalRunId: string): Promise<GoalProjectionFile[]> {
  const dataDir = await getDataDir();
  if (!dataDir) return [];
  const root = `goals/${goalRunId}`;
  const files: GoalProjectionFile[] = [];
  const visit = async (relativeDir: string) => {
    const entries = await listPersistedDir(relativeDir);
    for (const entry of entries) {
      if (entry.isDirectory) {
        await visit(entry.path);
      } else {
        files.push({
          relativePath: entry.path.startsWith(`${root}/`) ? entry.path.slice(root.length + 1) : entry.path,
          absolutePath: `${dataDir.replace(/\/$/, "")}/${entry.path}`,
          sizeBytes: null,
        });
      }
    }
  };
  await visit(root);
  return files.sort((a, b) => a.relativePath.localeCompare(b.relativePath));
}
