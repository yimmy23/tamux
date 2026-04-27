import { TasksView } from "@/components/agent-chat-panel/TasksView";
import { useAgentChatPanelRuntime } from "@/components/agent-chat-panel/runtime/context";

export function GoalsRail() {
  const { goalRunsForTrace, setView, setChatBackView } = useAgentChatPanelRuntime();

  return (
    <div className="zorai-rail-stack">
      <button
        type="button"
        className="zorai-primary-button"
        onClick={() => {
          setChatBackView("tasks");
          setView("tasks");
        }}
      >
        New Goal
      </button>
      <div className="zorai-section-label">Active Runs</div>
      {goalRunsForTrace.length === 0 ? (
        <div className="zorai-empty">No goal runs are loaded yet.</div>
      ) : (
        goalRunsForTrace.map((goal) => (
          <div key={goal.id} className="zorai-rail-card">
            <strong>{goal.title || goal.goal}</strong>
            <span>{goal.status}</span>
          </div>
        ))
      )}
    </div>
  );
}

export function GoalsView() {
  const { setView, setChatBackView } = useAgentChatPanelRuntime();

  return (
    <section className="zorai-feature-surface">
      <TasksView
        onOpenThreadView={() => {
          setChatBackView("tasks");
          setView("chat");
        }}
      />
    </section>
  );
}
