import { useMemo, useState } from "react";
import {
  controlGoalRun,
  formatGoalRunDuration,
  formatGoalRunStatus,
  isGoalRunActive,
  latestGoalRunTodoSnapshot,
  summarizeGoalRunStep,
  type GoalRun,
  type GoalRunControlAction,
} from "@/lib/goalRuns";

type GoalWorkspaceMode = "dossier" | "files" | "progress" | "usage" | "active-agent" | "threads" | "attention";

const goalWorkspaceModes: Array<{ id: GoalWorkspaceMode; label: string }> = [
  { id: "dossier", label: "Dossier" },
  { id: "files", label: "Files" },
  { id: "progress", label: "Progress" },
  { id: "usage", label: "Usage" },
  { id: "active-agent", label: "Active agent" },
  { id: "threads", label: "Threads" },
  { id: "attention", label: "Needs attention" },
];

export function GoalWorkspacePanel({
  run,
  onRefresh,
  onMessage,
}: {
  run: GoalRun | null;
  onRefresh: () => Promise<void>;
  onMessage: (message: string) => void;
}) {
  const [mode, setMode] = useState<GoalWorkspaceMode>("dossier");
  const activeStepIndex = typeof run?.current_step_index === "number" ? run.current_step_index : null;
  const activeStep = activeStepIndex !== null ? run?.steps?.[activeStepIndex] : null;
  const todos = useMemo(() => (run ? latestGoalRunTodoSnapshot(run) : []), [run]);

  const control = async (action: GoalRunControlAction) => {
    if (!run) return;
    const ok = await controlGoalRun(run.id, action, activeStepIndex);
    onMessage(ok ? `Goal ${action.replace(/_/g, " ")} requested.` : "Goal action failed.");
    await onRefresh();
  };

  if (!run) {
    return (
      <div className="zorai-panel zorai-goal-workspace">
        <div className="zorai-empty-state">Select a goal run to open the TUI-style workspace.</div>
      </div>
    );
  }

  return (
    <div className="zorai-panel zorai-goal-workspace">
      <div className="zorai-goal-workspace__header">
        <div>
          <div className="zorai-section-label">Goal Workspace</div>
          <h2>{run.title || run.goal}</h2>
          <span>{formatGoalRunStatus(run.status)} / {summarizeGoalRunStep(run)}</span>
        </div>
        <div className="zorai-card-actions">
          <button type="button" className="zorai-ghost-button" onClick={() => void onRefresh()}>Refresh</button>
          {isGoalRunActive(run) ? (
            run.status === "paused" ? (
              <button type="button" className="zorai-ghost-button" onClick={() => void control("resume")}>Resume</button>
            ) : (
              <button type="button" className="zorai-ghost-button" onClick={() => void control("pause")}>Pause</button>
            )
          ) : null}
          <button type="button" className="zorai-ghost-button" onClick={() => void control("retry_step")} disabled={activeStepIndex === null}>Retry step</button>
          <button type="button" className="zorai-ghost-button" onClick={() => void control("rerun_from_step")} disabled={activeStepIndex === null}>Rerun from step</button>
        </div>
      </div>

      <div className="zorai-goal-mode-tabs" aria-label="Goal workspace modes">
        {goalWorkspaceModes.map((item) => (
          <button
            type="button"
            key={item.id}
            className={["zorai-ghost-button", mode === item.id ? "zorai-button--active" : ""].filter(Boolean).join(" ")}
            onClick={() => setMode(item.id)}
          >
            {item.label}
          </button>
        ))}
      </div>

      <div className="zorai-goal-workspace__body">
        {mode === "dossier" ? <DossierMode run={run} activeStepTitle={activeStep?.title ?? null} /> : null}
        {mode === "files" ? <FilesMode run={run} /> : null}
        {mode === "progress" ? <ProgressMode run={run} todos={todos} /> : null}
        {mode === "usage" ? <UsageMode run={run} /> : null}
        {mode === "active-agent" ? <ActiveAgentMode run={run} /> : null}
        {mode === "threads" ? <ThreadsMode run={run} /> : null}
        {mode === "attention" ? <AttentionMode run={run} todos={todos} /> : null}
      </div>
    </div>
  );
}

function DossierMode({ run, activeStepTitle }: { run: GoalRun; activeStepTitle: string | null }) {
  return (
    <div className="zorai-goal-mode-grid">
      <InfoBlock label="Goal" value={run.goal} />
      <InfoBlock label="Current Step" value={activeStepTitle ?? summarizeGoalRunStep(run)} />
      <InfoBlock label="Plan Summary" value={run.plan_summary ?? "No plan summary yet."} />
      <InfoBlock label="Result / Error" value={run.result ?? run.error ?? run.last_error ?? "No result yet."} />
    </div>
  );
}

function FilesMode({ run }: { run: GoalRun }) {
  const entries = [
    ...(run.generated_skill_path ? [`Generated skill: ${run.generated_skill_path}`] : []),
    ...(run.memory_updates ?? []).map((entry) => `Memory update: ${entry}`),
  ];
  return <ListMode empty="No goal files or memory artifacts are reported yet." entries={entries} />;
}

function ProgressMode({ run, todos }: { run: GoalRun; todos: ReturnType<typeof latestGoalRunTodoSnapshot> }) {
  const stepEntries = (run.steps ?? []).map((step, index) => `${index + 1}. ${step.title} / ${step.status ?? "pending"}`);
  const todoEntries = todos.map((todo) => `${todo.status}: ${todo.content}`);
  return <ListMode empty="No plan steps or todos are available yet." entries={[...stepEntries, ...todoEntries]} />;
}

function UsageMode({ run }: { run: GoalRun }) {
  const entries = (run.model_usage ?? []).map((usage) => (
    `${usage.provider}/${usage.model} / ${usage.request_count} req / ${usage.prompt_tokens.toLocaleString()} in / ${usage.completion_tokens.toLocaleString()} out / $${Number(usage.estimated_cost_usd ?? 0).toFixed(6)}`
  ));
  return (
    <div className="zorai-goal-mode-grid">
      <InfoBlock label="Total Prompt" value={(run.total_prompt_tokens ?? 0).toLocaleString()} />
      <InfoBlock label="Total Completion" value={(run.total_completion_tokens ?? 0).toLocaleString()} />
      <InfoBlock label="Estimated Cost" value={`$${Number(run.estimated_cost_usd ?? 0).toFixed(6)}`} />
      <InfoBlock label="Duration" value={formatGoalRunDuration(run.duration_ms)} />
      <div className="zorai-goal-mode-list zorai-goal-mode-list--wide">
        <ListMode empty="No model usage has been reported yet." entries={entries} />
      </div>
    </div>
  );
}

function ActiveAgentMode({ run }: { run: GoalRun }) {
  const owners = [
    run.current_step_owner_profile ? `Current: ${ownerText(run.current_step_owner_profile)}` : null,
    run.planner_owner_profile ? `Planner: ${ownerText(run.planner_owner_profile)}` : null,
    ...((run.runtime_assignment_list ?? run.launch_assignment_snapshot ?? []).map((assignment) =>
      `${assignment.role_id}: ${assignment.enabled ? "enabled" : "disabled"} / ${assignment.provider}/${assignment.model}`,
    )),
  ].filter((entry): entry is string => Boolean(entry));
  return <ListMode empty="No active agent owner or assignment data is available yet." entries={owners} />;
}

function ThreadsMode({ run }: { run: GoalRun }) {
  const entries = [
    ...(run.thread_id ? [`Primary thread: ${run.thread_id}`] : []),
    ...(run.child_task_ids ?? []).map((id) => `Child task: ${id}`),
    ...(run.session_id ? [`Session: ${run.session_id}`] : []),
  ];
  return <ListMode empty="No goal thread links are available yet." entries={entries} />;
}

function AttentionMode({ run, todos }: { run: GoalRun; todos: ReturnType<typeof latestGoalRunTodoSnapshot> }) {
  const entries = [
    ...(run.awaiting_approval_id ? [`Awaiting approval: ${run.awaiting_approval_id}`] : []),
    ...(run.error ? [`Error: ${run.error}`] : []),
    ...(run.last_error ? [`Last error: ${run.last_error}`] : []),
    ...todos.filter((todo) => todo.status === "blocked").map((todo) => `Blocked: ${todo.content}`),
  ];
  return <ListMode empty="No attention items are active for this goal." entries={entries} />;
}

function InfoBlock({ label, value }: { label: string; value: string }) {
  return (
    <div className="zorai-goal-mode-info">
      <div className="zorai-section-label">{label}</div>
      <p>{value}</p>
    </div>
  );
}

function ListMode({ empty, entries }: { empty: string; entries: string[] }) {
  if (entries.length === 0) return <div className="zorai-empty-state">{empty}</div>;
  return (
    <div className="zorai-goal-mode-list">
      {entries.map((entry) => <div key={entry}>{entry}</div>)}
    </div>
  );
}

function ownerText(owner: NonNullable<GoalRun["current_step_owner_profile"]>) {
  return `${owner.agent_label} / ${owner.provider}/${owner.model}${owner.reasoning_effort ? ` / ${owner.reasoning_effort}` : ""}`;
}
