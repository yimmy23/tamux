import { useEffect, useRef, useState } from "react";
import { useSnippetStore, resolveSnippetTemplate, getSnippetPlaceholders } from "../lib/snippetStore";
import type { Snippet } from "../lib/snippetStore";
import { useWorkspaceStore } from "../lib/workspaceStore";
import { getTerminalController } from "../lib/terminalRegistry";
import { Overlay } from "./snippet-picker/Overlay";
import { SnippetForm } from "./snippet-picker/SnippetForm";
import { SnippetListView } from "./snippet-picker/SnippetListView";
import { SnippetResolverDialog } from "./snippet-picker/SnippetResolverDialog";
import type { SnippetPickerProps } from "./snippet-picker/shared";

/**
 * Snippet Picker modal — Ctrl+S.
 * Search, create, edit, delete, favorites, template placeholder resolution.
 * Matches amux-windows SnippetPicker.
 */
export function SnippetPicker({ style, className }: SnippetPickerProps = {}) {
  const open = useWorkspaceStore((s) => s.snippetPickerOpen);
  const toggle = useWorkspaceStore((s) => s.toggleSnippetPicker);
  const snippets = useSnippetStore((s) => s.snippets);
  const addSnippet = useSnippetStore((s) => s.addSnippet);
  const updateSnippet = useSnippetStore((s) => s.updateSnippet);
  const deleteSnippet = useSnippetStore((s) => s.deleteSnippet);
  const incrementUseCount = useSnippetStore((s) => s.incrementUseCount);
  const toggleFavorite = useSnippetStore((s) => s.toggleFavorite);
  const search = useSnippetStore((s) => s.search);
  const getCategories = useSnippetStore((s) => s.getCategories);
  const activePaneId = useWorkspaceStore((s) => s.activePaneId());

  const [query, setQuery] = useState("");
  const [ownerFilter, setOwnerFilter] = useState<"both" | "user" | "assistant">("both");
  const [editingSnippet, setEditingSnippet] = useState<Snippet | null>(null);
  const [creating, setCreating] = useState(false);
  const [templateParams, setTemplateParams] = useState<Record<string, string>>({});
  const [resolvingSnippet, setResolvingSnippet] = useState<Snippet | null>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (open) {
      setQuery("");
      setEditingSnippet(null);
      setCreating(false);
      setResolvingSnippet(null);
      setOwnerFilter("both");
      setTimeout(() => inputRef.current?.focus(), 50);
    }
  }, [open]);

  if (!open) return null;
  const resolvedModalStyle = style;

  const ownerScoped = snippets.filter((snippet) => ownerFilter === "both" || snippet.owner === ownerFilter);
  const filtered = query
    ? search(query).filter((snippet) => ownerFilter === "both" || snippet.owner === ownerFilter)
    : [...ownerScoped].sort((a, b) => {
        if (a.isFavorite !== b.isFavorite) return a.isFavorite ? -1 : 1;
        return b.useCount - a.useCount;
      });

  const categories = getCategories();

  function handleUseSnippet(snippet: Snippet) {
    const placeholders = getSnippetPlaceholders(snippet.content);
    if (placeholders.length > 0) {
      setResolvingSnippet(snippet);
      setTemplateParams(Object.fromEntries(placeholders.map((p) => [p, ""])));
    } else {
      executeSnippet(snippet, {});
    }
  }

  async function executeSnippet(snippet: Snippet, params: Record<string, string>) {
    const resolved = resolveSnippetTemplate(snippet.content, params);
    incrementUseCount(snippet.id);
    const controller = getTerminalController(activePaneId);
    if (controller) {
      await controller.sendText(resolved, { trackHistory: false });
    } else {
      navigator.clipboard.writeText(resolved).catch(() => {});
    }
    toggle();
  }

  if (resolvingSnippet) {
    return (
      <Overlay onClose={toggle}>
        <SnippetResolverDialog
          style={resolvedModalStyle}
          className={className}
          snippet={resolvingSnippet}
          templateParams={templateParams}
          setTemplateParams={setTemplateParams}
          onResolve={() => void executeSnippet(resolvingSnippet, templateParams)}
          onCancel={() => setResolvingSnippet(null)}
        />
      </Overlay>
    );
  }

  if (editingSnippet || creating) {
    return (
      <Overlay onClose={toggle}>
        <SnippetForm
          style={resolvedModalStyle}
          className={className}
          snippet={editingSnippet}
          categories={categories}
          onSave={(data) => {
            if (editingSnippet) {
              updateSnippet(editingSnippet.id, data);
            } else {
              addSnippet({ name: data.name!, content: data.content!, owner: "user", ...data });
            }
            setEditingSnippet(null);
            setCreating(false);
          }}
          onCancel={() => {
            setEditingSnippet(null);
            setCreating(false);
          }}
        />
      </Overlay>
    );
  }

  return (
    <Overlay onClose={toggle}>
      <SnippetListView
        className={className}
        style={resolvedModalStyle}
        inputRef={inputRef}
        query={query}
        setQuery={setQuery}
        ownerFilter={ownerFilter}
        setOwnerFilter={setOwnerFilter}
        filtered={filtered}
        onCreate={() => setCreating(true)}
        onClose={toggle}
        onUse={(snippet) => void handleUseSnippet(snippet)}
        onEdit={setEditingSnippet}
        onDelete={(snippet) => deleteSnippet(snippet.id)}
        onToggleFavorite={(snippet) => toggleFavorite(snippet.id)}
      />
    </Overlay>
  );
}
