import { useCallback, useEffect, useMemo, useState } from "react";
import { useAgentChatPanelRuntime } from "@/components/agent-chat-panel/runtime/context";
import {
  controlGoalRun,
  fetchGoalRuns,
  formatGoalRunDuration,
  formatGoalRunStatus,
  goalRunChildTaskCount,
  goalRunSupportAvailable,
  isGoalRunActive,
  latestGoalRunTodoSnapshot,
  startGoalRun,
  summarizeGoalRunStep,
  type GoalRun,
} from "@/lib/goalRuns";

const activeStatuses = new Set(["queued", "planning", "running", "awaiting_approval", "paused"]);

export function GoalsRail() {
  const { goalRunsForTrace } = useAgentChatPanelRuntime();
  const activeGoals = goalRunsForTrace.filter((goal) => activeStatuses.has(goal.status));

  return (
    <div className="zorai-rail-stack">
      <div className="zorai-section-label">Active Goals</div>
      {activeGoals.length === 0 ? (
        <div className="zorai-empty">No goal runs are active.</div>
      ) : (
        activeGoals.slice(0, 6).map((goal) => (
          <div key={goal.id} className="zorai-rail-card">
            <strong>{goal.title || goal.goal}</strong>
            <span>{formatGoalRunStatus(goal.status)}</span>
          </div>
        ))
      )}
    </div>
  );
}

export function GoalsView() {
  const { goalRunsForTrace } = useAgentChatPanelRuntime();
  const [goalRuns, setGoalRuns] = useState<GoalRun[]>([]);
  const [title, setTitle] = useState("");
  const [goal, setGoal] = useState("");
  const [starting, setStarting] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const supported = goalRunSupportAvailable();

  const visibleGoalRuns = useMemo(() => {
    const byId = new Map<string, GoalRun>();
    for (const item of goalRunsForTrace) byId.set(item.id, item);
    for (const item of goalRuns) byId.set(item.id, item);
    return [...byId.values()].sort((a, b) => b.created_at - a.created_at);
  }, [goalRuns, goalRunsForTrace]);

  const metrics = useMemo(() => {
    const active = visibleGoalRuns.filter(isGoalRunActive).length;
    const waiting = visibleGoalRuns.filter((run) => run.status === "awaiting_approval").length;
    const completed = visibleGoalRuns.filter((run) => run.status === "completed").length;
    return { active, waiting, completed, total: visibleGoalRuns.length };
  }, [visibleGoalRuns]);

  const refresh = useCallback(async () => {
    setGoalRuns(await fetchGoalRuns());
  }, []);

  useEffect(() => {
    void refresh();
    const timer = window.setInterval(() => void refresh(), 5000);
    return () => window.clearInterval(timer);
  }, [refresh]);

  const handleStartGoal = async () => {
    if (!goal.trim() || !supported) return;
    setStarting(true);
    setMessage(null);
    const result = await startGoalRun({
      title: title.trim() || null,
      goal: goal.trim(),
      priority: "normal",
    });
    setStarting(false);
    if (!result) {
      setMessage("Goal runner is not available from this runtime.");
      return;
    }
    setTitle("");
    setGoal("");
    setMessage("Goal queued.");
    await refresh();
  };

  const handleControl = async (run: GoalRun, action: "pause" | "resume" | "cancel") => {
    setMessage(null);
    const ok = await controlGoalRun(run.id, action);
    setMessage(ok ? `${formatGoalRunStatus(run.status)} goal updated.` : "Goal action failed.");
    await refresh();
  };

  return (
    <section className="zorai-feature-surface zorai-goals-surface">
      <div className="zorai-view-header">
        <div>
          <div className="zorai-kicker">Goals</div>
          <h1>Plan, run, and supervise durable agent goals.</h1>
          <p>Goals turn a thread intent into a monitored run with steps, approvals, child tasks, and result memory.</p>
        </div>
      </div>

      <div className="zorai-metric-grid">
        <Metric label="Active" value={metrics.active} />
        <Metric label="Awaiting Approval" value={metrics.waiting} />
        <Metric label="Completed" value={metrics.completed} />
        <Metric label="Total Runs" value={metrics.total} />
      </div>

      <div className="zorai-goals-layout">
        <form className="zorai-panel" onSubmit={(event) => { event.preventDefault(); void handleStartGoal(); }}>
          <div>
            <div className="zorai-section-label">New Goal</div>
            <h2>Start orchestration</h2>
          </div>
          <input
            className="zorai-input"
            value={title}
            onChange={(event) => setTitle(event.target.value)}
            placeholder="Optional goal title"
          />
          <textarea
            className="zorai-textarea"
            value={goal}
            onChange={(event) => setGoal(event.target.value)}
            placeholder="Describe the outcome, constraints, and acceptance criteria..."
          />
          <button type="submit" className="zorai-primary-button" disabled={!supported || !goal.trim() || starting}>
            {starting ? "Starting..." : "Start Goal"}
          </button>
          {message ? <div className="zorai-inline-note">{message}</div> : null}
        </form>

        <div className="zorai-panel zorai-goal-list">
          <div>
            <div className="zorai-section-label">Goal Runs</div>
            <h2>Live supervision</h2>
          </div>
          {visibleGoalRuns.length === 0 ? (
            <div className="zorai-empty-state">No goal runs are loaded yet.</div>
          ) : (
            visibleGoalRuns.map((run) => <GoalRunCard key={run.id} run={run} onControl={handleControl} />)
          )}
        </div>
      </div>
    </section>
  );
}

function Metric({ label, value }: { label: string; value: number }) {
  return (
    <div className="zorai-metric-card">
      <strong>{value}</strong>
      <span>{label}</span>
    </div>
  );
}

function GoalRunCard({
  run,
  onControl,
}: {
  run: GoalRun;
  onControl: (run: GoalRun, action: "pause" | "resume" | "cancel") => void;
}) {
  const todos = latestGoalRunTodoSnapshot(run).slice(0, 4);
  return (
    <article className="zorai-run-card">
      <div className="zorai-run-card__header">
        <div>
          <strong>{run.title || run.goal}</strong>
          <span>{summarizeGoalRunStep(run)}</span>
        </div>
        <span className="zorai-status-pill">{formatGoalRunStatus(run.status)}</span>
      </div>
      <p>{run.result || run.error || run.plan_summary || run.goal}</p>
      <div className="zorai-run-card__meta">
        <span>{goalRunChildTaskCount(run)} child tasks</span>
        <span>{run.replan_count} replans</span>
        <span>{formatGoalRunDuration(run.duration_ms)}</span>
      </div>
      {todos.length > 0 ? (
        <div className="zorai-todo-strip">
          {todos.map((todo) => <span key={todo.id}>{todo.content}</span>)}
        </div>
      ) : null}
      {isGoalRunActive(run) ? (
        <div className="zorai-card-actions">
          {run.status === "paused" ? (
            <button type="button" className="zorai-ghost-button" onClick={() => onControl(run, "resume")}>Resume</button>
          ) : (
            <button type="button" className="zorai-ghost-button" onClick={() => onControl(run, "pause")}>Pause</button>
          )}
          <button type="button" className="zorai-ghost-button" onClick={() => onControl(run, "cancel")}>Cancel</button>
        </div>
      ) : null}
    </article>
  );
}

