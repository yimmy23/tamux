import { Badge } from "../ui";

export function CommandPaletteHeader({ commandCount }: { commandCount: number }) {
  return (
    <div className="flex flex-col gap-[var(--space-3)] p-[var(--space-4)] pr-[calc(var(--space-4)+var(--space-6))]">
      <div className="flex flex-wrap items-center gap-[var(--space-2)]">
        <Badge variant="mission">Action Launcher</Badge>
        <Badge variant="default">{commandCount} commands</Badge>
      </div>
      <div className="flex flex-col gap-[var(--space-1)]">
        <div className="text-[var(--text-xl)] font-bold text-[var(--text-primary)]">Mission Command Palette</div>
        <div className="text-[var(--text-sm)] text-[var(--text-secondary)]">
          Search layouts, tools, views, and workspace actions.
        </div>
      </div>
    </div>
  );
}
