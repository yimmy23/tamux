import { useState, type CSSProperties, type ReactNode } from "react";
import type { Snippet } from "../../lib/snippetStore";
import { Badge, Button, Input, TextArea, cn, panelSurfaceClassName } from "../ui";
import type { SnippetFormData } from "./shared";

export function SnippetForm({
  style,
  className,
  snippet,
  categories,
  onSave,
  onCancel,
}: {
  style?: CSSProperties;
  className?: string;
  snippet: Snippet | null;
  categories: string[];
  onSave: (data: SnippetFormData) => void;
  onCancel: () => void;
}) {
  const [name, setName] = useState(snippet?.name ?? "");
  const [content, setContent] = useState(snippet?.content ?? "");
  const [category, setCategory] = useState(snippet?.category ?? "General");
  const [description, setDescription] = useState(snippet?.description ?? "");
  const [tags, setTags] = useState(snippet?.tags.join(", ") ?? "");

  return (
    <div
      style={style}
      className={cn(
        panelSurfaceClassName,
        "flex max-h-[85vh] w-[min(90vw,40rem)] max-w-full flex-col overflow-hidden rounded-[var(--radius-xl)] border-[var(--border-strong)] bg-[var(--card)] shadow-[var(--shadow-lg)]",
        className
      )}
    >
      <div className="flex items-start justify-between gap-[var(--space-3)] border-b border-[var(--border-subtle)] px-[var(--space-5)] py-[var(--space-4)]">
        <div className="grid gap-[var(--space-2)]">
          <div className="flex flex-wrap items-center gap-[var(--space-2)]">
            <Badge variant="accent">Snippet editor</Badge>
            <Badge variant={snippet ? "timeline" : "success"}>{snippet ? "Update" : "Create"}</Badge>
          </div>
          <div className="grid gap-[var(--space-1)]">
            <span className="text-[var(--text-lg)] font-semibold text-[var(--text-primary)]">
              {snippet ? "Edit Snippet" : "New Snippet"}
            </span>
            <span className="text-[var(--text-sm)] leading-6 text-[var(--text-secondary)]">
              Capture reusable commands, templates, and operator shortcuts for quick insertion.
            </span>
          </div>
        </div>
        <Button onClick={onCancel} variant="ghost" size="sm">
          Close
        </Button>
      </div>

      <div className="grid gap-[var(--space-4)] px-[var(--space-5)] py-[var(--space-4)]">
        <Field label="Name">
          <Input type="text" value={name} onChange={(event) => setName(event.target.value)} autoFocus />
        </Field>

        <Field label="Content (command)">
          <TextArea
            value={content}
            onChange={(event) => setContent(event.target.value)}
            rows={5}
            className="font-mono text-[var(--text-xs)]"
          />
        </Field>

        <Field label="Category">
          <div className="grid gap-[var(--space-2)] md:grid-cols-[12rem_minmax(0,1fr)]">
            <select
              value={category}
              onChange={(event) => setCategory(event.target.value)}
              className="flex w-full rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--input)] px-[var(--space-3)] py-[var(--space-2)] text-[var(--text-sm)] text-[var(--input-foreground)]"
            >
              {[...new Set([...categories, "General", "Git", "Docker", "System", "Network"])].map((value) => (
                <option key={value} value={value}>
                  {value}
                </option>
              ))}
            </select>
            <Input
              type="text"
              value={category}
              onChange={(event) => setCategory(event.target.value)}
              placeholder="or type new..."
            />
          </div>
        </Field>

        <Field label="Description">
          <Input type="text" value={description} onChange={(event) => setDescription(event.target.value)} />
        </Field>

        <Field label="Tags (comma-separated)">
          <Input type="text" value={tags} onChange={(event) => setTags(event.target.value)} />
        </Field>
      </div>

      <div className="flex flex-wrap justify-end gap-[var(--space-2)] border-t border-[var(--border-subtle)] px-[var(--space-5)] py-[var(--space-4)]">
        <Button onClick={onCancel} variant="secondary">
          Cancel
        </Button>
        <Button
          onClick={() => {
            if (!name.trim() || !content.trim()) return;
            onSave({
              name,
              content,
              category,
              description,
              tags: tags
                .split(",")
                .map((tag) => tag.trim())
                .filter(Boolean),
            });
          }}
        >
          {snippet ? "Save" : "Create"}
        </Button>
      </div>
    </div>
  );
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="grid gap-[var(--space-2)]">
      <span className="text-[var(--text-xs)] font-medium uppercase tracking-[0.08em] text-[var(--text-muted)]">
        {label}
      </span>
      {children}
    </label>
  );
}
