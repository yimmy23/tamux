import { useMemo, useState } from "react";
import { ActivityRail, ActivityView } from "../features/activity/ActivityView";
import { GoalsRail, GoalsView } from "../features/goals/GoalsView";
import { SettingsRail, SettingsView } from "../features/settings/SettingsView";
import { getDefaultZoraiSettingsTab, type ZoraiSettingsTabId } from "../features/settings/settingsTabs";
import { ThreadsContext } from "../features/threads/ThreadsContextPanel";
import { ThreadsRail, ThreadsView } from "../features/threads/ThreadsView";
import { ToolsRail, ToolsView } from "../features/tools/ToolsView";
import { getDefaultZoraiTool, type ZoraiToolId } from "../features/tools/tools";
import { WorkspacesRail, WorkspacesView } from "../features/workspaces/WorkspacesView";
import { getDefaultZoraiView, zoraiNavItems, type ZoraiViewId } from "./navigation";
import { ZoraiContextPanel } from "./ZoraiContextPanel";
import { ZoraiBrandMark, ZoraiNavIcon } from "./ZoraiIcons";

export function ZoraiShell() {
  const [activeView, setActiveView] = useState<ZoraiViewId>(getDefaultZoraiView);
  const [activeTool, setActiveTool] = useState<ZoraiToolId>(getDefaultZoraiTool);
  const [activeSettingsTab, setActiveSettingsTab] = useState<ZoraiSettingsTabId>(getDefaultZoraiSettingsTab);
  const [contextOpen, setContextOpen] = useState(false);
  const activeItem = useMemo(
    () => zoraiNavItems.find((item) => item.id === activeView) ?? zoraiNavItems[0],
    [activeView],
  );

  return (
    <div className="zorai-shell">
      <nav className="zorai-global-rail" aria-label="Zorai navigation">
        <div className="zorai-brand" title="Zorai">
          <ZoraiBrandMark />
        </div>
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
              aria-label={item.label}
            >
              <ZoraiNavIcon icon={item.icon} />
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
        {renderRail(activeView, activeTool, setActiveTool, activeSettingsTab, setActiveSettingsTab)}
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
        <div className="zorai-main-body">{renderMain(activeView, activeTool, setActiveTool, activeSettingsTab, setActiveSettingsTab)}</div>
      </main>

      <ZoraiContextPanel
        title="Orchestration Context"
        subtitle={activeItem.railLabel}
        open={contextOpen}
        onToggle={() => setContextOpen((current) => !current)}
      >
        {activeView === "threads" ? (
          <ThreadsContext />
        ) : (
          renderRail(activeView, activeTool, setActiveTool, activeSettingsTab, setActiveSettingsTab)
        )}
      </ZoraiContextPanel>
    </div>
  );
}

function renderRail(
  view: ZoraiViewId,
  activeTool: ZoraiToolId,
  setActiveTool: (toolId: ZoraiToolId) => void,
  activeSettingsTab: ZoraiSettingsTabId,
  setActiveSettingsTab: (tabId: ZoraiSettingsTabId) => void,
) {
  if (view === "threads") return <ThreadsRail />;
  if (view === "goals") return <GoalsRail />;
  if (view === "workspaces") return <WorkspacesRail />;
  if (view === "tools") return <ToolsRail activeTool={activeTool} onSelectTool={setActiveTool} />;
  if (view === "activity") return <ActivityRail />;
  return <SettingsRail activeTab={activeSettingsTab} onSelectTab={setActiveSettingsTab} />;
}

function renderMain(
  view: ZoraiViewId,
  activeTool: ZoraiToolId,
  setActiveTool: (toolId: ZoraiToolId) => void,
  activeSettingsTab: ZoraiSettingsTabId,
  setActiveSettingsTab: (tabId: ZoraiSettingsTabId) => void,
) {
  if (view === "threads") return <ThreadsView />;
  if (view === "goals") return <GoalsView />;
  if (view === "workspaces") return <WorkspacesView />;
  if (view === "tools") return <ToolsView activeTool={activeTool} onSelectTool={setActiveTool} />;
  if (view === "activity") return <ActivityView />;
  return <SettingsView activeTab={activeSettingsTab} onSelectTab={setActiveSettingsTab} />;
}
