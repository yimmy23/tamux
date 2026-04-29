import { useCallback, useEffect, useMemo, useState } from "react";
import { ActivityRail, ActivityView } from "../features/activity/ActivityView";
import { DatabaseRail, DatabaseView } from "../features/database/DatabaseView";
import { GoalsContext, GoalsRail, GoalsView } from "../features/goals/GoalsView";
import { SettingsRail, SettingsView } from "../features/settings/SettingsView";
import { getDefaultZoraiSettingsTab, type ZoraiSettingsTabId } from "../features/settings/settingsTabs";
import { ThreadsContext } from "../features/threads/ThreadsContextPanel";
import { ThreadFilePreviewProvider } from "../features/threads/ThreadFilePreviewProvider";
import { ThreadsRail, ThreadsView } from "../features/threads/ThreadsView";
import { ToolsContext, ToolsRail, ToolsView } from "../features/tools/ToolsView";
import { getDefaultZoraiTool, type ZoraiToolId } from "../features/tools/tools";
import { WorkspacesRail, WorkspacesView } from "../features/workspaces/WorkspacesView";
import { getDefaultZoraiView, zoraiNavItems, type ZoraiViewId } from "./navigation";
import { ZoraiContextPanel } from "./ZoraiContextPanel";
import { ZoraiBrandMark, ZoraiNavIcon } from "./ZoraiIcons";
import { ZORAI_NAVIGATE_EVENT, type ZoraiNavigateDetail, type ZoraiReturnTarget } from "./zoraiNavigationEvents";

type GoalOpenRequest = {
  id: string;
  nonce: number;
};

export function ZoraiShell() {
  const [activeView, setActiveView] = useState<ZoraiViewId>(getDefaultZoraiView);
  const [activeTool, setActiveTool] = useState<ZoraiToolId>(getDefaultZoraiTool);
  const [activeSettingsTab, setActiveSettingsTab] = useState<ZoraiSettingsTabId>(getDefaultZoraiSettingsTab);
  const [activeDatabaseTable, setActiveDatabaseTable] = useState<string | null>(null);
  const [contextOpen, setContextOpen] = useState(false);
  const [returnTarget, setReturnTarget] = useState<ZoraiReturnTarget | null>(null);
  const [goalOpenRequest, setGoalOpenRequest] = useState<GoalOpenRequest | null>(null);
  const activeItem = useMemo(
    () => zoraiNavItems.find((item) => item.id === activeView) ?? zoraiNavItems[0],
    [activeView],
  );

  useEffect(() => {
    const onNavigate = (event: Event) => {
      const detail = (event as CustomEvent<ZoraiNavigateDetail>).detail;
      if (detail.view) setActiveView(detail.view);
      if (detail.tool) setActiveTool(detail.tool);
      if (detail.returnTarget !== undefined) setReturnTarget(detail.returnTarget);
      if (detail.goalRunId) {
        setGoalOpenRequest((current) => ({ id: detail.goalRunId ?? "", nonce: (current?.nonce ?? 0) + 1 }));
      }
    };
    window.addEventListener(ZORAI_NAVIGATE_EVENT, onNavigate);
    return () => window.removeEventListener(ZORAI_NAVIGATE_EVENT, onNavigate);
  }, []);

  const selectView = (view: ZoraiViewId) => {
    setActiveView(view);
    setReturnTarget(null);
  };
  const selectDatabaseTable = useCallback((tableName: string) => {
    setActiveDatabaseTable(tableName);
  }, []);

  return (
    <ThreadFilePreviewProvider>
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
                onClick={() => selectView(item.id)}
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
          {renderRail(activeView, activeTool, setActiveTool, activeSettingsTab, setActiveSettingsTab, activeDatabaseTable, selectDatabaseTable)}
        </aside>

        <main className="zorai-main">
          <header className="zorai-topbar">
            <div>
              <div className="zorai-kicker">Zorai</div>
              <h1>{activeItem.label}</h1>
            </div>
            <div className="zorai-card-actions">
              {returnTarget ? (
                <button
                  type="button"
                  className="zorai-ghost-button"
                  onClick={() => {
                    setActiveView(returnTarget.view);
                    setReturnTarget(null);
                  }}
                >
                  {returnTarget.label}
                </button>
              ) : null}
              <button
                type="button"
                className="zorai-ghost-button"
                onClick={() => setContextOpen((current) => !current)}
              >
                {contextOpen ? "Hide Context" : "Show Context"}
              </button>
            </div>
          </header>
          <div className="zorai-main-body">{renderMain(activeView, activeTool, setActiveTool, activeSettingsTab, setActiveSettingsTab, goalOpenRequest, activeDatabaseTable, selectDatabaseTable)}</div>
        </main>

        <ZoraiContextPanel
          title="Orchestration Context"
          subtitle={activeItem.railLabel}
          open={contextOpen}
          onToggle={() => setContextOpen((current) => !current)}
        >
          {renderContext(activeView, activeTool, setActiveTool)}
        </ZoraiContextPanel>
      </div>
    </ThreadFilePreviewProvider>
  );
}

function renderRail(
  view: ZoraiViewId,
  activeTool: ZoraiToolId,
  setActiveTool: (toolId: ZoraiToolId) => void,
  activeSettingsTab: ZoraiSettingsTabId,
  setActiveSettingsTab: (tabId: ZoraiSettingsTabId) => void,
  activeDatabaseTable: string | null,
  setActiveDatabaseTable: (tableName: string) => void,
) {
  if (view === "threads") return <ThreadsRail />;
  if (view === "goals") return <GoalsRail />;
  if (view === "workspaces") return <WorkspacesRail />;
  if (view === "database") return <DatabaseRail activeTable={activeDatabaseTable} onSelectTable={setActiveDatabaseTable} />;
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
  goalOpenRequest: GoalOpenRequest | null,
  activeDatabaseTable: string | null,
  setActiveDatabaseTable: (tableName: string) => void,
) {
  if (view === "threads") return <ThreadsView />;
  if (view === "goals") return <GoalsView openGoalRunRequest={goalOpenRequest} />;
  if (view === "workspaces") return <WorkspacesView />;
  if (view === "database") return <DatabaseView activeTable={activeDatabaseTable} onSelectTable={setActiveDatabaseTable} />;
  if (view === "tools") return <ToolsView activeTool={activeTool} onSelectTool={setActiveTool} />;
  if (view === "activity") return <ActivityView />;
  return <SettingsView activeTab={activeSettingsTab} onSelectTab={setActiveSettingsTab} />;
}

function renderContext(
  view: ZoraiViewId,
  activeTool: ZoraiToolId,
  setActiveTool: (toolId: ZoraiToolId) => void,
) {
  if (view === "threads") return <ThreadsContext />;
  if (view === "goals") return <GoalsContext />;
  if (view === "tools") return <ToolsContext activeTool={activeTool} onSelectTool={setActiveTool} />;
  return <GenericContext view={view} />;
}

function GenericContext({ view }: { view: ZoraiViewId }) {
  const item = zoraiNavItems.find((entry) => entry.id === view) ?? zoraiNavItems[0];
  return (
    <div className="zorai-context-summary">
      <div className="zorai-section-label">{item.label}</div>
      <div className="zorai-context-block">
        <strong>{item.railLabel}</strong>
        <span>{item.description}</span>
      </div>
    </div>
  );
}
