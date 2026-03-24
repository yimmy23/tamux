import { Badge, Card, ScrollArea, Separator } from "../ui";
import type { Command } from "./shared";

export function CommandPaletteResults({
  filtered,
  grouped,
  categories,
  flatFiltered,
  selectedIndex,
  setSelectedIndex,
  onExecute,
}: {
  filtered: Command[];
  grouped: Record<string, Command[]>;
  categories: string[];
  flatFiltered: Command[];
  selectedIndex: number;
  setSelectedIndex: (index: number) => void;
  onExecute: (command: Command) => void;
}) {
  return (
    <ScrollArea className="min-h-0 flex-1">
      <div className="flex flex-col gap-[var(--space-3)] p-[var(--space-3)]">
        {filtered.length === 0 ? (
          <Card className="border-dashed bg-[var(--surface)]/60 p-[var(--space-6)] text-center text-[var(--text-sm)] text-[var(--text-muted)]">
            No matching commands.
          </Card>
        ) : null}

        {categories.map((category, categoryIndex) => (
          <div key={category} className="flex flex-col gap-[var(--space-2)]">
            {categoryIndex > 0 ? <Separator /> : null}
            <div className="px-[var(--space-1)] pt-[var(--space-1)]">
              <Badge variant="default" className="uppercase tracking-[0.08em] text-[10px] text-[var(--text-muted)]">
                {category}
              </Badge>
            </div>

            <div className="flex flex-col gap-[var(--space-2)]">
              {grouped[category].map((command) => {
                const globalIndex = flatFiltered.indexOf(command);
                const isSelected = globalIndex === selectedIndex;

                return (
                  <button
                    key={command.id}
                    type="button"
                    onClick={() => onExecute(command)}
                    onMouseEnter={() => setSelectedIndex(globalIndex)}
                    className={[
                      "w-full rounded-[var(--radius-lg)] border p-[var(--space-3)] text-left transition-colors duration-100 ease-out",
                      isSelected
                        ? "border-[var(--accent-border)] bg-[var(--accent-soft)]"
                        : "border-[var(--border)] bg-[var(--card)] hover:border-[var(--border-strong)] hover:bg-[var(--surface)]",
                    ].join(" ")}
                  >
                    <div className="flex items-start justify-between gap-[var(--space-3)]">
                      <div className="flex min-w-0 flex-col gap-[2px]">
                        <span className="truncate text-[var(--text-sm)] font-medium text-[var(--text-primary)]">
                          {command.label}
                        </span>
                        <span className="text-[var(--text-xs)] text-[var(--text-muted)]">
                          {command.id.replace(/-/g, " ")}
                        </span>
                      </div>

                      {command.shortcut ? (
                        <Badge variant="default" className="amux-code shrink-0 px-[var(--space-2)] py-[2px]">
                          {command.shortcut}
                        </Badge>
                      ) : null}
                    </div>
                  </button>
                );
              })}
            </div>
          </div>
        ))}
      </div>
    </ScrollArea>
  );
}
