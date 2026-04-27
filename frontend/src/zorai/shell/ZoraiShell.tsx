import { useMemo, useState } from "react";
import { ActivityRail, ActivityView } from "../features/activity/ActivityView";
import { GoalsRail, GoalsView } from "../features/goals/GoalsView";
import { SettingsRail, SettingsView } from "../features/settings/SettingsView";
import { ThreadsContext, ThreadsRail, ThreadsView } from "../features/threads/ThreadsView";
import { ToolsRail, ToolsView } from "../features/tools/ToolsView";
import { WorkspacesRail, WorkspacesView } from "../features/workspaces/WorkspacesView";
import { getDefaultZoraiView, zoraiNavItems, type ZoraiViewId } from "./navigation";
import { ZoraiContextPanel } from "./ZoraiContextPanel";

export function ZoraiShell() {
  const [activeView, setActiveView] = useState<ZoraiViewId>(getDefaultZoraiView);
  const [contextOpen, setContextOpen] = useState(false);
  const activeItem = useMemo(
    () => zoraiNavItems.find((item) => item.id === activeView) ?? zoraiNavItems[0],
    [activeView],
  );

  return (
    <div className="zorai-shell">
      <nav className="zorai-global-rail" aria-label="Zorai navigation">
        <div className="zorai-brand" title="Zorai">Z</div>
        <div className="zorai-global-items">
          {zoraiNavItems.map((item) => (
            <button
              type="button"
              key={item.id}
              className={[
                "zorai-global-item",
                item.id === activeView ? "zorai-global-item--active" : "",
              ].filter(Boolean).join(" ")}
              onClick={() => setActiveView(item.id)}
              title={item.label}
            >
              {item.shortLabel}
            </button>
          ))}
        </div>
      </nav>

      <aside className="zorai-contextual-rail" aria-label={activeItem.railLabel}>
        <div className="zorai-rail-heading">
          <div className="zorai-kicker">{activeItem.label}</div>
          <h2>{activeItem.railLabel}</h2>
          <p>{activeItem.description}</p>
        </div>
        {renderRail(activeView)}
      </aside>

      <main className="zorai-main">
        <header className="zorai-topbar">
          <div>
            <div className="zorai-kicker">Zorai</div>
            <h1>{activeItem.label}</h1>
          </div>
          <button
            type="button"
            className="zorai-ghost-button"
            onClick={() => setContextOpen((current) => !current)}
          >
            {contextOpen ? "Hide Context" : "Show Context"}
          </button>
        </header>
        <div className="zorai-main-body">{renderMain(activeView)}</div>
      </main>

      <ZoraiContextPanel
        title="Orchestration Context"
        subtitle={activeItem.railLabel}
        open={contextOpen}
        onToggle={() => setContextOpen((current) => !current)}
      >
        {activeView === "threads" ? <ThreadsContext /> : renderRail(activeView)}
      </ZoraiContextPanel>
    </div>
  );
}

function renderRail(view: ZoraiViewId) {
  if (view === "threads") return <ThreadsRail />;
  if (view === "goals") return <GoalsRail />;
  if (view === "workspaces") return <WorkspacesRail />;
  if (view === "tools") return <ToolsRail />;
  if (view === "activity") return <ActivityRail />;
  return <SettingsRail />;
}

function renderMain(view: ZoraiViewId) {
  if (view === "threads") return <ThreadsView />;
  if (view === "goals") return <GoalsView />;
  if (view === "workspaces") return <WorkspacesView />;
  if (view === "tools") return <ToolsView />;
  if (view === "activity") return <ActivityView />;
  return <SettingsView />;
}
