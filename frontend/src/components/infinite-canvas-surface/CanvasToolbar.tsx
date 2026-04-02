type CanvasToolbarProps = {
  showNewPanelMenu: boolean;
  snapEnabled: boolean;
  hasPreviousView: boolean;
  onToggleNewPanelMenu: () => void;
  onCreatePanel: () => void;
  onCreateBrowserPanel: () => void;
  onArrangePanels: () => void;
  onToggleSnap: () => void;
  onCenterView: () => void;
  onRestorePreviousView: () => void;
};

export function CanvasToolbar({
  showNewPanelMenu,
  snapEnabled,
  hasPreviousView,
  onToggleNewPanelMenu,
  onCreatePanel,
  onCreateBrowserPanel,
  onArrangePanels,
  onToggleSnap,
  onCenterView,
  onRestorePreviousView,
}: CanvasToolbarProps) {
  return (
    <div data-canvas-toolbar="true" style={{ position: "absolute", top: 10, left: 10, display: "flex", gap: 8, zIndex: 40 }}>
      <div style={{ position: "relative" }}>
        <button
          type="button"
          onClick={onToggleNewPanelMenu}
          title="Add panel"
          style={{
            height: 30,
            minWidth: 32,
            borderRadius: "var(--radius-md)",
            border: "1px solid var(--accent)",
            background: "var(--accent-soft)",
            color: "var(--accent)",
            fontSize: 18,
            lineHeight: 1,
            cursor: "pointer",
          }}
        >
          +
        </button>
        {showNewPanelMenu ? (
          <div
            data-canvas-menu="true"
            style={{
              position: "absolute",
              top: 34,
              left: 0,
              zIndex: 50,
              minWidth: 150,
              border: "1px solid var(--glass-border)",
              borderRadius: "var(--radius-md)",
              background: "var(--bg-primary)",
              boxShadow: "var(--shadow-sm)",
              padding: 4,
              display: "grid",
              gap: 2,
            }}
          >
            <button type="button" style={menuButtonStyle} onClick={onCreatePanel}>Terminal</button>
            <button type="button" style={menuButtonStyle} onClick={onCreateBrowserPanel}>Browser</button>
          </div>
        ) : null}
      </div>

      <button type="button" onClick={onArrangePanels} title="Auto arrange panels" style={toolbarButtonStyle}>
        Arrange
      </button>
      <button
        type="button"
        onClick={onToggleSnap}
        title="Toggle grid snap"
        style={{
          ...toolbarButtonStyle,
          border: snapEnabled ? "1px solid var(--accent)" : toolbarButtonStyle.border,
          background: snapEnabled ? "var(--accent-soft)" : toolbarButtonStyle.background,
          color: snapEnabled ? "var(--accent)" : toolbarButtonStyle.color,
        }}
      >
        Snap
      </button>
      <button type="button" onClick={onCenterView} title="Center view" style={toolbarButtonStyle}>
        Center
      </button>
      {hasPreviousView ? (
        <button type="button" onClick={onRestorePreviousView} title="Return to previous view" style={toolbarButtonStyle}>
          Back to previous
        </button>
      ) : null}
    </div>
  );
}

const toolbarButtonStyle = {
  height: 30,
  borderRadius: "var(--radius-md)",
  border: "1px solid var(--border)",
  background: "var(--bg-secondary)",
  color: "var(--text-secondary)",
  fontSize: 12,
  padding: "0 10px",
  cursor: "pointer",
} as const;

const menuButtonStyle = {
  ...toolbarButtonStyle,
  width: "100%",
  height: "auto",
  padding: "6px 8px",
  textAlign: "left",
  border: "none",
  background: "transparent",
  color: "var(--text-primary)",
} as const;
