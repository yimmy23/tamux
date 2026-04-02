import { allLeafIds } from "../../lib/bspTree";
import { iconGlyph, normalizeIconId } from "../../lib/iconRegistry";
import { shortenHomePath } from "../../lib/workspaceStore";
import {
  countBadgeStyle,
  paneCountBadgeStyle,
  paneNodeButtonStyle,
  paneRenameInputStyle,
  pendingDotStyle,
  surfaceNodeStyle,
  treeNodeButtonStyle,
  treeToggleStyle,
} from "./sidebarStyles";
import type { SidebarTreeProps } from "./sidebarTypes";

export function SidebarTree({
  filteredWorkspaces,
  activeWorkspaceId,
  collapsedWorkspaces,
  setCollapsedWorkspaces,
  collapsedSurfaces,
  setCollapsedSurfaces,
  contextMenu,
  setContextMenu,
  clearSelections,
  setActiveWorkspace,
  editingWorkspaceId,
  workspaceNameDraft,
  setWorkspaceNameDraft,
  renameWorkspace,
  setEditingWorkspaceId,
  getUnread,
  selectionWorkspaceId,
  selectedSurfaceIds,
  selectSurfaceInWorkspace,
  setActiveSurface,
  selectedPaneIds,
  attentionByPane,
  editingPaneId,
  setEditingPaneId,
  paneNameDraft,
  setPaneNameDraft,
  setPaneName,
  selectPaneInWorkspace,
  setActivePaneId,
  focusCanvasPanel,
}: SidebarTreeProps) {
  return (
    <div style={{ display: "flex", flexDirection: "column", gap: 2 }}>
      {filteredWorkspaces.map((workspace) => {
        const workspaceCollapsed = collapsedWorkspaces[workspace.id] ?? false;
        const workspaceActive = workspace.id === activeWorkspaceId;
        const workspaceContextActive =
          contextMenu?.kind === "workspace" &&
          contextMenu.workspaceId === workspace.id;

        return (
          <div key={workspace.id} style={{ display: "grid", gap: 2 }}>
            <div
              onContextMenu={(event) => {
                event.preventDefault();
                clearSelections();
                setActiveWorkspace(workspace.id);
                setContextMenu({
                  kind: "workspace",
                  workspaceId: workspace.id,
                  x: event.clientX,
                  y: event.clientY,
                });
              }}
              style={{
                display: "flex",
                alignItems: "center",
                gap: 6,
                padding: "3px 4px",
                borderRadius: "var(--radius-sm)",
                background:
                  workspaceActive || workspaceContextActive
                    ? "var(--bg-secondary)"
                    : "transparent",
              }}
            >
              <button
                type="button"
                onClick={() => {
                  setCollapsedWorkspaces((current) => ({
                    ...current,
                    [workspace.id]: !(current[workspace.id] ?? false),
                  }));
                }}
                style={treeToggleStyle}
                title={workspaceCollapsed ? "Expand" : "Collapse"}
              >
                {workspaceCollapsed ? "▸" : "▾"}
              </button>

              <button
                type="button"
                onClick={() => {
                  clearSelections();
                  setActiveWorkspace(workspace.id);
                }}
                style={treeNodeButtonStyle(workspaceActive, workspace.accentColor)}
              >
                <span style={{ opacity: 0.9 }}>{iconGlyph(workspace.icon)}</span>
                {editingWorkspaceId === workspace.id ? (
                  <input
                    autoFocus
                    value={workspaceNameDraft}
                    onChange={(event) => setWorkspaceNameDraft(event.target.value)}
                    onBlur={() => {
                      if (workspaceNameDraft.trim()) {
                        renameWorkspace(workspace.id, workspaceNameDraft);
                      }
                      setEditingWorkspaceId(null);
                    }}
                    onKeyDown={(event) => {
                      if (event.key === "Enter") {
                        if (workspaceNameDraft.trim()) {
                          renameWorkspace(workspace.id, workspaceNameDraft);
                        }
                        setEditingWorkspaceId(null);
                      }
                      if (event.key === "Escape") {
                        setEditingWorkspaceId(null);
                      }
                    }}
                    style={paneRenameInputStyle}
                  />
                ) : (
                  <span
                    style={{
                      overflow: "hidden",
                      textOverflow: "ellipsis",
                      whiteSpace: "nowrap",
                    }}
                  >
                    {workspace.name}
                  </span>
                )}
              </button>

              {getUnread(workspace.id) > 0 ? (
                <span style={countBadgeStyle}>{getUnread(workspace.id)}</span>
              ) : null}
            </div>

            {!workspaceCollapsed ? (
              <div
                style={{
                  marginLeft: 16,
                  borderLeft: "1px solid var(--border)",
                  paddingLeft: 8,
                  display: "grid",
                  gap: 2,
                }}
              >
                {workspace.surfaces.map((surface) => {
                  const surfaceCollapsed = collapsedSurfaces[surface.id] ?? false;
                  const paneIds = allLeafIds(surface.layout);
                  const surfaceSelected =
                    selectionWorkspaceId === workspace.id &&
                    selectedSurfaceIds.includes(surface.id);

                  return (
                    <div key={surface.id} style={{ display: "grid", gap: 2 }}>
                      <div
                        onContextMenu={(event) => {
                          event.preventDefault();
                          const toggle = event.metaKey || event.ctrlKey;
                          const range = event.shiftKey;
                          selectSurfaceInWorkspace(workspace.id, surface.id, {
                            toggle,
                            range,
                            preserveIfAlreadySelected: !toggle && !range,
                          });
                          setContextMenu({
                            kind: "surface",
                            workspaceId: workspace.id,
                            surfaceId: surface.id,
                            x: event.clientX,
                            y: event.clientY,
                          });
                        }}
                        style={{ display: "flex", alignItems: "center", gap: 6, padding: "2px 0" }}
                      >
                        <button
                          type="button"
                          onClick={() => {
                            setCollapsedSurfaces((current) => ({
                              ...current,
                              [surface.id]: !(current[surface.id] ?? false),
                            }));
                          }}
                          style={treeToggleStyle}
                        >
                          {surfaceCollapsed ? "▸" : "▾"}
                        </button>

                        <button
                          type="button"
                          onClick={(event) => {
                            const toggle = event.metaKey || event.ctrlKey;
                            const range = event.shiftKey;
                            selectSurfaceInWorkspace(workspace.id, surface.id, {
                              toggle,
                              range,
                            });
                            if (toggle || range) {
                              return;
                            }
                            setActiveWorkspace(workspace.id);
                            setActiveSurface(surface.id);
                          }}
                          style={surfaceNodeStyle(
                            workspace.activeSurfaceId === surface.id || surfaceSelected,
                          )}
                        >
                          <span style={{ opacity: 0.9 }}>{iconGlyph(surface.icon)}</span>
                          <span style={{ overflow: "hidden", textOverflow: "ellipsis", whiteSpace: "nowrap" }}>
                            {surface.name}
                          </span>
                          <span style={{ marginLeft: "auto", opacity: 0.65 }}>
                            {surface.layoutMode}
                          </span>
                        </button>
                      </div>

                      {!surfaceCollapsed ? (
                        <div
                          style={{
                            marginLeft: 16,
                            borderLeft: "1px dotted var(--border)",
                            paddingLeft: 8,
                            display: "grid",
                            gap: 1,
                          }}
                        >
                          {paneIds.map((paneId) => {
                            const paneActive =
                              workspaceActive &&
                              workspace.activeSurfaceId === surface.id &&
                              surface.activePaneId === paneId;
                            const paneSelected =
                              selectionWorkspaceId === workspace.id &&
                              selectedPaneIds.includes(paneId);
                            const paneName = surface.paneNames[paneId] ?? paneId;
                            const paneIcon = normalizeIconId(surface.paneIcons[paneId]);
                            const paneAttentionCount = attentionByPane.get(paneId) ?? 0;
                            const canvasPanel = surface.canvasPanels?.find(
                              (panel) => panel.paneId === paneId,
                            );
                            const paneCwd = canvasPanel?.cwd ?? null;
                            const panelType = canvasPanel?.panelType ?? "terminal";
                            const needsApproval =
                              paneAttentionCount > 0 ||
                              canvasPanel?.status === "needs_approval";
                            const editing = editingPaneId === paneId;

                            return (
                              <div
                                key={paneId}
                                onContextMenu={(event) => {
                                  event.preventDefault();
                                  const toggle = event.metaKey || event.ctrlKey;
                                  const range = event.shiftKey;
                                  selectPaneInWorkspace(workspace.id, paneId, {
                                    toggle,
                                    range,
                                    preserveIfAlreadySelected: !toggle && !range,
                                  });
                                  setContextMenu({
                                    kind: "pane",
                                    workspaceId: workspace.id,
                                    surfaceId: surface.id,
                                    paneId,
                                    x: event.clientX,
                                    y: event.clientY,
                                  });
                                }}
                                style={{
                                  display: "flex",
                                  alignItems: "center",
                                  gap: 6,
                                  padding: "2px 4px",
                                  borderRadius: "var(--radius-sm)",
                                  background:
                                    paneSelected || paneActive
                                      ? "var(--bg-tertiary)"
                                      : "transparent",
                                }}
                              >
                                <button
                                  type="button"
                                  onClick={(event) => {
                                    const toggle = event.metaKey || event.ctrlKey;
                                    const range = event.shiftKey;
                                    selectPaneInWorkspace(workspace.id, paneId, {
                                      toggle,
                                      range,
                                    });
                                    if (toggle || range) {
                                      return;
                                    }
                                    setActiveWorkspace(workspace.id);
                                    setActiveSurface(surface.id);
                                    setActivePaneId(paneId);
                                    if (surface.layoutMode === "canvas") {
                                      focusCanvasPanel(paneId, {
                                        storePreviousView: true,
                                      });
                                    }
                                  }}
                                  style={paneNodeButtonStyle(needsApproval)}
                                >
                                  <span style={{ opacity: 0.9, flexShrink: 0 }}>
                                    {panelType === "browser"
                                      ? "🌐"
                                      : iconGlyph(paneIcon)}
                                  </span>
                                  {needsApproval ? <span style={pendingDotStyle} /> : null}
                                  <div
                                    style={{
                                      display: "flex",
                                      flexDirection: "column",
                                      overflow: "hidden",
                                      minWidth: 0,
                                      flex: 1,
                                      gap: 0,
                                    }}
                                  >
                                    {editing ? (
                                      <input
                                        autoFocus
                                        value={paneNameDraft}
                                        onChange={(event) =>
                                          setPaneNameDraft(event.target.value)
                                        }
                                        onBlur={() => {
                                          if (paneNameDraft.trim()) {
                                            setPaneName(paneId, paneNameDraft);
                                          }
                                          setEditingPaneId(null);
                                        }}
                                        onKeyDown={(event) => {
                                          if (event.key === "Enter") {
                                            if (paneNameDraft.trim()) {
                                              setPaneName(paneId, paneNameDraft);
                                            }
                                            setEditingPaneId(null);
                                          }
                                          if (event.key === "Escape") {
                                            setEditingPaneId(null);
                                          }
                                        }}
                                        style={paneRenameInputStyle}
                                      />
                                    ) : (
                                      <span
                                        onDoubleClick={() => {
                                          setEditingPaneId(paneId);
                                          setPaneNameDraft(paneName);
                                        }}
                                        style={{
                                          overflow: "hidden",
                                          textOverflow: "ellipsis",
                                          whiteSpace: "nowrap",
                                          lineHeight: 1.3,
                                        }}
                                      >
                                        {paneName}
                                      </span>
                                    )}
                                    {paneCwd ? (
                                      <span
                                        style={{
                                          color: "var(--text-muted)",
                                          fontSize: "9px",
                                          whiteSpace: "nowrap",
                                          overflow: "hidden",
                                          textOverflow: "ellipsis",
                                          lineHeight: 1.2,
                                        }}
                                      >
                                        {shortenHomePath(paneCwd)}
                                      </span>
                                    ) : null}
                                  </div>
                                  <span
                                    style={{
                                      color: "var(--text-muted)",
                                      opacity: 0.4,
                                      fontSize: "8px",
                                      whiteSpace: "nowrap",
                                      flexShrink: 0,
                                    }}
                                  >
                                    {paneId.slice(0, 8)}
                                  </span>
                                  {paneAttentionCount > 0 ? (
                                    <span style={paneCountBadgeStyle(needsApproval)}>
                                      {paneAttentionCount > 9 ? "9+" : paneAttentionCount}
                                    </span>
                                  ) : null}
                                </button>
                              </div>
                            );
                          })}
                        </div>
                      ) : null}
                    </div>
                  );
                })}
              </div>
            ) : null}
          </div>
        );
      })}
    </div>
  );
}
