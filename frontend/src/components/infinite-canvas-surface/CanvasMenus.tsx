import { iconChoices, PANE_ICON_IDS } from "../../lib/iconRegistry";
import { AppConfirmDialog } from "../AppConfirmDialog";
import { contextMenuItemStyle, contextMenuSectionLabelStyle, dangerContextMenuItemStyle } from "./styles";
import type { CanvasContextMenuState, CanvasIconPickerState, CanvasPanelRecord } from "./types";

type CanvasMenusProps = {
  contextMenu: CanvasContextMenuState | null;
  iconPicker: CanvasIconPickerState | null;
  contextPaneIds: string[];
  confirmClosePaneIds: string[];
  panelByPane: Map<string, CanvasPanelRecord>;
  paneNames: Record<string, string>;
  workspaceMoveTargets: Array<{ id: string; name: string }>;
  onZoomIn: () => void;
  onRenamePanel: () => void;
  onDuplicatePanels: () => void;
  onConvertToBsp: () => void;
  onRunSnippet: () => void;
  onOpenIconPicker: () => void;
  onMoveToWorkspace: (workspaceId: string) => void;
  onRequestClosePanels: () => void;
  onCloseIconPicker: () => void;
  onSetCanvasPanelIcon: (paneId: string, iconId: string) => void;
  onConfirmClosePanels: () => void;
  onCancelClosePanels: () => void;
};

export function CanvasMenus({
  contextMenu,
  iconPicker,
  contextPaneIds,
  confirmClosePaneIds,
  panelByPane,
  paneNames,
  workspaceMoveTargets,
  onZoomIn,
  onRenamePanel,
  onDuplicatePanels,
  onConvertToBsp,
  onRunSnippet,
  onOpenIconPicker,
  onMoveToWorkspace,
  onRequestClosePanels,
  onCloseIconPicker,
  onSetCanvasPanelIcon,
  onConfirmClosePanels,
  onCancelClosePanels,
}: CanvasMenusProps) {
  return (
    <>
      {contextMenu ? (
        <div
          data-canvas-menu="true"
          style={{
            position: "fixed",
            left: contextMenu.x,
            top: contextMenu.y,
            zIndex: 120,
            minWidth: 210,
            border: "1px solid var(--glass-border)",
            borderRadius: "var(--radius-md)",
            background: "var(--bg-primary)",
            boxShadow: "var(--shadow-sm)",
            padding: 4,
            display: "grid",
            gap: 2,
          }}
        >
          <button type="button" style={contextMenuItemStyle} onClick={onZoomIn}>Zoom In</button>
          {contextPaneIds.length === 1 ? (
            <button type="button" style={contextMenuItemStyle} onClick={onRenamePanel}>Rename Panel</button>
          ) : null}
          <button type="button" style={contextMenuItemStyle} onClick={onDuplicatePanels}>
            {contextPaneIds.length > 1 ? `Duplicate ${contextPaneIds.length} Panels` : "Duplicate Panel"}
          </button>
          {contextPaneIds.some((id) => panelByPane.get(id)?.panelType !== "browser") ? (
            <button type="button" style={contextMenuItemStyle} onClick={onConvertToBsp}>
              {contextPaneIds.length > 1 ? `Convert ${contextPaneIds.length} to BSP` : "Convert to BSP"}
            </button>
          ) : null}
          {contextPaneIds.some((id) => panelByPane.get(id)?.panelType !== "browser") ? (
            <button type="button" style={contextMenuItemStyle} onClick={onRunSnippet}>Run Snippet</button>
          ) : null}
          <button type="button" style={contextMenuItemStyle} onClick={onOpenIconPicker}>
            {contextPaneIds.length > 1 ? `Change Icon (${contextPaneIds.length} Panels)` : "Change Icon"}
          </button>
          {workspaceMoveTargets.length > 0 ? (
            <>
              <div style={contextMenuSectionLabelStyle}>Move to workspace</div>
              {workspaceMoveTargets.map((workspace) => (
                <button
                  key={workspace.id}
                  type="button"
                  style={contextMenuItemStyle}
                  onClick={() => onMoveToWorkspace(workspace.id)}
                >
                  {workspace.name}
                </button>
              ))}
            </>
          ) : null}
          <button type="button" style={dangerContextMenuItemStyle} onClick={onRequestClosePanels}>
            {contextPaneIds.length > 1 ? `Close ${contextPaneIds.length} Panels` : "Close Panel"}
          </button>
        </div>
      ) : null}

      {iconPicker ? (
        <div
          data-canvas-menu="true"
          style={{
            position: "fixed",
            left: iconPicker.x,
            top: iconPicker.y,
            zIndex: 130,
            minWidth: 180,
            border: "1px solid var(--glass-border)",
            borderRadius: "var(--radius-md)",
            background: "var(--bg-secondary)",
            boxShadow: "var(--shadow-sm)",
            padding: 4,
            display: "grid",
            gap: 2,
          }}
        >
          {iconChoices(PANE_ICON_IDS).map((icon) => (
            <button
              key={icon.id}
              type="button"
              style={contextMenuItemStyle}
              onClick={() => {
                for (const paneId of iconPicker.paneIds) {
                  onSetCanvasPanelIcon(paneId, icon.id);
                }
                onCloseIconPicker();
              }}
            >
              <span style={{ display: "inline-flex", alignItems: "center", gap: 8 }}>
                <span style={{ minWidth: 24, textAlign: "center", fontFamily: "ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace" }}>{icon.glyph}</span>
                <span>{icon.label}</span>
              </span>
            </button>
          ))}
        </div>
      ) : null}

      <AppConfirmDialog
        open={confirmClosePaneIds.length > 0}
        title={confirmClosePaneIds.length > 1
          ? `Close ${confirmClosePaneIds.length} panels?`
          : confirmClosePaneIds.length === 1
            ? `Close '${paneNames[confirmClosePaneIds[0]] ?? "panel"}'?`
            : ""}
        message={confirmClosePaneIds.length > 1
          ? "All selected terminal panels will be closed."
          : "This terminal panel will be closed."}
        confirmLabel={confirmClosePaneIds.length > 1 ? `Close ${confirmClosePaneIds.length} Panels` : "Close Panel"}
        tone="danger"
        onCancel={onCancelClosePanels}
        onConfirm={onConfirmClosePanels}
      />
    </>
  );
}
