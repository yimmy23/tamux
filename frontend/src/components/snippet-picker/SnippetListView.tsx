import type { CSSProperties, RefObject } from "react";
import type { Snippet } from "../../lib/snippetStore";
import { Badge, Button, Input, cn, panelSurfaceClassName } from "../ui";

export function SnippetListView({
  className,
  style,
  inputRef,
  query,
  setQuery,
  ownerFilter,
  setOwnerFilter,
  filtered,
  onCreate,
  onClose,
  onUse,
  onEdit,
  onDelete,
  onToggleFavorite,
}: {
  className?: string;
  style?: CSSProperties;
  inputRef: RefObject<HTMLInputElement | null>;
  query: string;
  setQuery: (value: string) => void;
  ownerFilter: "both" | "user" | "assistant";
  setOwnerFilter: (value: "both" | "user" | "assistant") => void;
  filtered: Snippet[];
  onCreate: () => void;
  onClose: () => void;
  onUse: (snippet: Snippet) => void;
  onEdit: (snippet: Snippet) => void;
  onDelete: (snippet: Snippet) => void;
  onToggleFavorite: (snippet: Snippet) => void;
}) {
  return (
    <div
      style={style}
      className={cn(
        panelSurfaceClassName,
        "flex max-h-[85vh] w-[min(90vw,44rem)] max-w-full flex-col overflow-hidden rounded-[var(--radius-xl)] border-[var(--border-strong)] bg-[var(--card)] shadow-[var(--shadow-lg)]",
        className
      )}
    >
      <div className="flex items-start justify-between gap-[var(--space-3)] border-b border-[var(--border-subtle)] px-[var(--space-5)] py-[var(--space-4)]">
        <div className="grid gap-[var(--space-2)]">
          <div className="flex flex-wrap items-center gap-[var(--space-2)]">
            <Badge variant="accent">Snippet library</Badge>
            <Badge variant="default">{filtered.length} shown</Badge>
          </div>
          <div className="grid gap-[var(--space-1)]">
            <span className="text-[var(--text-lg)] font-semibold text-[var(--text-primary)]">Snippets</span>
            <span className="text-[var(--text-sm)] leading-6 text-[var(--text-secondary)]">
              Search, favorite, and insert reusable commands into the active pane.
            </span>
          </div>
        </div>
        <div className="flex gap-[var(--space-2)]">
          <Button onClick={onCreate} variant="secondary" size="sm" title="New snippet">
            New
          </Button>
          <Button onClick={onClose} variant="ghost" size="sm">
            Close
          </Button>
        </div>
      </div>

      <div className="grid gap-[var(--space-3)] border-b border-[var(--border-subtle)] bg-[var(--panel)]/40 px-[var(--space-5)] py-[var(--space-4)]">
        <Input
          ref={inputRef}
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          placeholder="Search snippets..."
          onKeyDown={(event) => {
            if (event.key === "Escape") onClose();
            if (event.key === "Enter" && filtered.length > 0) onUse(filtered[0]);
          }}
        />
        <div className="flex flex-wrap gap-[var(--space-2)]">
          {([
            ["both", "Both"],
            ["user", "User"],
            ["assistant", "Assistant"],
          ] as const).map(([value, label]) => (
            <Button
              key={value}
              type="button"
              onClick={() => setOwnerFilter(value)}
              variant={ownerFilter === value ? "primary" : "secondary"}
              size="sm"
            >
              {label}
            </Button>
          ))}
        </div>
      </div>

      <div className="max-h-[24rem] flex-1 overflow-auto bg-[var(--panel)]/20">
        {filtered.length === 0 ? (
          <div className="flex min-h-[12rem] items-center justify-center px-[var(--space-6)] text-center text-[var(--text-sm)] text-[var(--text-secondary)]">
            No snippets found.
          </div>
        ) : null}
        {filtered.map((snippet) => (
          <div
            key={snippet.id}
            className="flex cursor-pointer items-center gap-[var(--space-3)] border-b border-[var(--border-subtle)] px-[var(--space-5)] py-[var(--space-3)] transition-colors hover:bg-[var(--muted)]/60"
            onClick={() => onUse(snippet)}
          >
            <button
              onClick={(event) => {
                event.stopPropagation();
                onToggleFavorite(snippet);
              }}
              className={cn(
                "text-[1rem] transition-colors",
                snippet.isFavorite ? "text-[var(--warning)]" : "text-[var(--text-muted)]"
              )}
              title={snippet.isFavorite ? "Unfavorite" : "Favorite"}
            >
              {snippet.isFavorite ? "★" : "☆"}
            </button>

            <div className="min-w-0 flex-1">
              <div className="truncate text-[var(--text-sm)] font-semibold text-[var(--text-primary)]">
                {snippet.name}
              </div>
              <div className="truncate font-mono text-[var(--text-xs)] text-[var(--text-secondary)]">
                {snippet.content}
              </div>
              <div className="mt-[var(--space-1)] flex flex-wrap items-center gap-[var(--space-2)]">
                <Badge variant="default">{snippet.category}</Badge>
                <Badge variant={snippet.owner === "assistant" ? "agent" : "human"}>{snippet.owner}</Badge>
                {snippet.useCount > 0 ? <Badge variant="timeline">used {snippet.useCount}×</Badge> : null}
              </div>
            </div>

            <div className="flex gap-[var(--space-2)]">
              <Button
                onClick={(event) => {
                  event.stopPropagation();
                  onEdit(snippet);
                }}
                variant="secondary"
                size="sm"
                title="Edit"
              >
                Edit
              </Button>
              <Button
                onClick={(event) => {
                  event.stopPropagation();
                  onDelete(snippet);
                }}
                variant="destructive"
                size="sm"
                title="Delete"
              >
                Delete
              </Button>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
