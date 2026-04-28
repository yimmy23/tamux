import type { ReactNode } from "react";

type ZoraiContextPanelProps = {
  title: string;
  subtitle?: string;
  open: boolean;
  onToggle: () => void;
  children: ReactNode;
};

export function ZoraiContextPanel({
  title,
  subtitle,
  open,
  onToggle,
  children,
}: ZoraiContextPanelProps) {
  if (!open) {
    return (
      <button
        type="button"
        className="zorai-context-tab"
        onClick={onToggle}
        title="Open context"
      >
        Context
      </button>
    );
  }

  return (
    <aside className="zorai-context-panel" aria-label={title}>
      <div className="zorai-context-header">
        <div>
          <div className="zorai-context-title">{title}</div>
          {subtitle && <div className="zorai-context-subtitle">{subtitle}</div>}
        </div>
        <button
          type="button"
          className="zorai-icon-button"
          onClick={onToggle}
          title="Collapse context"
        >
          x
        </button>
      </div>
      <div className="zorai-context-body">{children}</div>
    </aside>
  );
}
