import type { CSSProperties } from "react";
import { getSnippetPlaceholders } from "../../lib/snippetStore";
import type { Snippet } from "../../lib/snippetStore";
import { Badge, Button, Input, cn, panelSurfaceClassName } from "../ui";

export function SnippetResolverDialog({
  style,
  className,
  snippet,
  templateParams,
  setTemplateParams,
  onResolve,
  onCancel,
}: {
  style?: CSSProperties;
  className?: string;
  snippet: Snippet;
  templateParams: Record<string, string>;
  setTemplateParams: (value: Record<string, string>) => void;
  onResolve: () => void;
  onCancel: () => void;
}) {
  const placeholders = getSnippetPlaceholders(snippet.content);

  return (
    <div
      style={style}
      className={cn(
        panelSurfaceClassName,
        "flex max-h-[85vh] w-[min(90vw,36rem)] max-w-full flex-col overflow-hidden rounded-[var(--radius-xl)] border-[var(--border-strong)] bg-[var(--card)] shadow-[var(--shadow-lg)]",
        className
      )}
    >
      <div className="flex items-start justify-between gap-[var(--space-3)] border-b border-[var(--border-subtle)] px-[var(--space-5)] py-[var(--space-4)]">
        <div className="grid gap-[var(--space-2)]">
          <div className="flex flex-wrap items-center gap-[var(--space-2)]">
            <Badge variant="timeline">Template resolver</Badge>
            <Badge variant="accent">{placeholders.length} placeholder{placeholders.length === 1 ? "" : "s"}</Badge>
          </div>
          <div className="grid gap-[var(--space-1)]">
            <span className="text-[var(--text-lg)] font-semibold text-[var(--text-primary)]">
              Fill Placeholders: {snippet.name}
            </span>
            <span className="text-[var(--text-sm)] leading-6 text-[var(--text-secondary)]">
              Provide values before inserting the snippet into the active pane.
            </span>
          </div>
        </div>
        <Button onClick={onCancel} variant="ghost" size="sm">
          Close
        </Button>
      </div>

      <div className="grid gap-[var(--space-4)] px-[var(--space-5)] py-[var(--space-4)]">
        <div className="rounded-[var(--radius-lg)] border border-[var(--border-subtle)] bg-[var(--panel)]/55 px-[var(--space-4)] py-[var(--space-3)] text-[var(--text-xs)] text-[var(--text-secondary)]">
          Template: <code className="text-[var(--text-primary)]">{snippet.content}</code>
        </div>
        {placeholders.map((placeholder, index) => (
          <label key={placeholder} className="grid gap-[var(--space-2)]">
            <span className="text-[var(--text-xs)] font-medium uppercase tracking-[0.08em] text-[var(--text-muted)]">
              {`{{${placeholder}}}`}
            </span>
            <Input
              type="text"
              value={templateParams[placeholder] ?? ""}
              onChange={(event) =>
                setTemplateParams({ ...templateParams, [placeholder]: event.target.value })
              }
              autoFocus={index === 0}
              onKeyDown={(event) => {
                if (event.key === "Enter") onResolve();
              }}
            />
          </label>
        ))}
      </div>

      <div className="flex justify-end gap-[var(--space-2)] border-t border-[var(--border-subtle)] px-[var(--space-5)] py-[var(--space-4)]">
        <Button onClick={onCancel} variant="secondary">
          Cancel
        </Button>
        <Button onClick={onResolve}>Insert &amp; Close</Button>
      </div>
    </div>
  );
}
