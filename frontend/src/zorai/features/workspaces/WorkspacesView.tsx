import { useMemo } from "react";
import { useWorkspaceStore } from "@/lib/workspaceStore";

const boardColumns = ["Todo", "In Progress", "In Review", "Done"] as const;

export function WorkspacesRail() {
  const workspaces = useWorkspaceStore((state) => state.workspaces);
  const activeWorkspaceId = useWorkspaceStore((state) => state.activeWorkspaceId);
  const setActiveWorkspace = useWorkspaceStore((state) => state.setActiveWorkspace);

  return (
    <div className="zorai-rail-stack">
      <div className="zorai-section-label">Workspaces</div>
      {workspaces.length === 0 ? (
        <div className="zorai-empty">No workspaces are open.</div>
      ) : (
        workspaces.map((workspace) => (
          <button
            type="button"
            key={workspace.id}
            className={[
              "zorai-thread-item",
              workspace.id === activeWorkspaceId ? "zorai-thread-item--active" : "",
            ].filter(Boolean).join(" ")}
            onClick={() => setActiveWorkspace(workspace.id)}
          >
            <span className="zorai-thread-title">{workspace.name}</span>
            <span className="zorai-thread-meta">{workspace.cwd}</span>
          </button>
        ))
      )}
    </div>
  );
}

export function WorkspacesView() {
  const activeWorkspace = useWorkspaceStore((state) => state.activeWorkspace());
  const visibleItems = useMemo(
    () => (activeWorkspace?.surfaces ?? [])
      .slice(0, 8)
      .map((surface) => ({ id: surface.id, title: surface.name })),
    [activeWorkspace?.surfaces],
  );

  return (
    <section className="zorai-feature-surface zorai-board-surface">
      <div className="zorai-view-header">
        <div>
          <div className="zorai-kicker">Workspace</div>
          <h1>{activeWorkspace?.name ?? "Workspace Board"}</h1>
          <p>
            Board-owned tasks belong here. Existing pane/surface records are shown only as
            migration hints until daemon workspace tasks are wired into this view.
          </p>
        </div>
      </div>
      <div className="zorai-board">
        {boardColumns.map((column, index) => (
          <div key={column} className="zorai-board-column">
            <div className="zorai-board-title">{column}</div>
            {index === 0 && visibleItems.length > 0 ? (
              visibleItems.map((item) => (
                <div key={item.id} className="zorai-board-card">
                  <strong>{item.title}</strong>
                  <span>Migration candidate</span>
                </div>
              ))
            ) : (
              <div className="zorai-empty">No cards</div>
            )}
          </div>
        ))}
      </div>
    </section>
  );
}
