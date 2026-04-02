import { AppConfirmDialog } from "../AppConfirmDialog";
import { iconChoices, PANE_ICON_IDS, WORKSPACE_ICON_IDS } from "../../lib/iconRegistry";
import {
  contextMenuItemStyle,
  dangerContextMenuItemStyle,
} from "./sidebarStyles";
import type { SidebarMenusProps } from "./sidebarTypes";

export function SidebarMenus({
  contextMenu,
  contextMenuRef,
  paneContextSelection,
  surfaceContextSelection,
  handleWorkspaceContextAction,
  handleSurfaceContextAction,
  handlePaneContextAction,
  iconPicker,
  iconPickerRef,
  setWorkspaceIcon,
  setPaneIcon,
  setIconPicker,
  confirmDialog,
  setConfirmDialog,
}: SidebarMenusProps) {
  return (
    <>
      {contextMenu ? (
        <div
          ref={contextMenuRef}
          style={{
            position: "fixed",
            left: contextMenu.x,
            top: contextMenu.y,
            zIndex: 2500,
            minWidth: 190,
            border: "1px solid var(--glass-border)",
            borderRadius: "var(--radius-md)",
            background: "var(--bg-secondary)",
            boxShadow: "var(--shadow-sm)",
            padding: 4,
            display: "grid",
            gap: 2,
          }}
        >
          {contextMenu.kind === "workspace" ? (
            <>
              <button type="button" style={contextMenuItemStyle} onClick={() => handleWorkspaceContextAction("rename", contextMenu.workspaceId)}>Rename Workspace</button>
              <button type="button" style={contextMenuItemStyle} onClick={() => handleWorkspaceContextAction("icon", contextMenu.workspaceId)}>Change Icon</button>
              <button type="button" style={contextMenuItemStyle} onClick={() => handleWorkspaceContextAction("append", contextMenu.workspaceId)}>Append New Terminal</button>
              <button type="button" style={contextMenuItemStyle} onClick={() => handleWorkspaceContextAction("new-canvas", contextMenu.workspaceId)}>New Infinite Canvas</button>
              <button type="button" style={dangerContextMenuItemStyle} onClick={() => handleWorkspaceContextAction("close", contextMenu.workspaceId)}>Close Workspace</button>
            </>
          ) : contextMenu.kind === "surface" ? (
            <button
              type="button"
              style={dangerContextMenuItemStyle}
              onClick={() =>
                handleSurfaceContextAction(
                  "close",
                  contextMenu.workspaceId,
                  contextMenu.surfaceId,
                )
              }
            >
              {surfaceContextSelection.length > 1
                ? `Close ${surfaceContextSelection.length} Surfaces`
                : "Close Surface"}
            </button>
          ) : (
            <>
              {paneContextSelection.length <= 1 ? (
                <button
                  type="button"
                  style={contextMenuItemStyle}
                  onClick={() =>
                    handlePaneContextAction(
                      "rename",
                      contextMenu.workspaceId,
                      contextMenu.surfaceId,
                      contextMenu.paneId,
                    )
                  }
                >
                  Rename Terminal
                </button>
              ) : null}
              <button
                type="button"
                style={contextMenuItemStyle}
                onClick={() =>
                  handlePaneContextAction(
                    "icon",
                    contextMenu.workspaceId,
                    contextMenu.surfaceId,
                    contextMenu.paneId,
                  )
                }
              >
                {paneContextSelection.length > 1
                  ? `Change Icon (${paneContextSelection.length} Terminals)`
                  : "Change Icon"}
              </button>
              {paneContextSelection.length <= 1 ? (
                <button
                  type="button"
                  style={contextMenuItemStyle}
                  onClick={() =>
                    handlePaneContextAction(
                      "append",
                      contextMenu.workspaceId,
                      contextMenu.surfaceId,
                      contextMenu.paneId,
                    )
                  }
                >
                  Append New Terminal
                </button>
              ) : null}
              <button
                type="button"
                style={contextMenuItemStyle}
                onClick={() =>
                  handlePaneContextAction(
                    "duplicate",
                    contextMenu.workspaceId,
                    contextMenu.surfaceId,
                    contextMenu.paneId,
                  )
                }
              >
                {paneContextSelection.length > 1
                  ? `Duplicate ${paneContextSelection.length} Terminals`
                  : "Duplicate Terminal"}
              </button>
              <button
                type="button"
                style={dangerContextMenuItemStyle}
                onClick={() =>
                  handlePaneContextAction(
                    "close",
                    contextMenu.workspaceId,
                    contextMenu.surfaceId,
                    contextMenu.paneId,
                  )
                }
              >
                {paneContextSelection.length > 1
                  ? `Close ${paneContextSelection.length} Terminals`
                  : "Close Terminal"}
              </button>
            </>
          )}
        </div>
      ) : null}

      {iconPicker ? (
        <div
          ref={iconPickerRef}
          style={{
            position: "fixed",
            left: iconPicker.x,
            top: iconPicker.y,
            zIndex: 2600,
            minWidth: 160,
            border: "1px solid var(--glass-border)",
            borderRadius: "var(--radius-md)",
            background: "var(--bg-primary)",
            boxShadow: "var(--shadow-sm)",
            padding: 4,
            display: "grid",
            gap: 2,
          }}
        >
          {iconChoices(
            iconPicker.kind === "workspace" ? WORKSPACE_ICON_IDS : PANE_ICON_IDS,
          ).map((icon) => (
            <button
              key={icon.id}
              type="button"
              style={contextMenuItemStyle}
              onClick={() => {
                if (iconPicker.kind === "workspace") {
                  setWorkspaceIcon(iconPicker.workspaceId, icon.id);
                } else {
                  for (const paneId of iconPicker.paneIds) {
                    setPaneIcon(paneId, icon.id);
                  }
                }
                setIconPicker(null);
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
        open={Boolean(confirmDialog)}
        title={confirmDialog?.title ?? ""}
        message={confirmDialog?.message ?? ""}
        confirmLabel={confirmDialog?.confirmLabel ?? "Confirm"}
        tone={confirmDialog?.tone ?? "danger"}
        onCancel={() => setConfirmDialog(null)}
        onConfirm={() => {
          if (!confirmDialog) return;
          confirmDialog.action();
          setConfirmDialog(null);
        }}
      />
    </>
  );
}
