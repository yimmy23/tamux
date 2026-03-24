import { useRef, useEffect, useState, useCallback } from "react";
import { useWorkspaceStore } from "../lib/workspaceStore";
import { clearTerminalSearch, searchTerminal, type TerminalSearchOptions } from "../lib/terminalRegistry";
import { Card, Separator } from "./ui";
import { SearchOverlayControls } from "./search-overlay/SearchOverlayControls";
import { SearchOverlayHeader } from "./search-overlay/SearchOverlayHeader";
import { SearchOverlayStatus } from "./search-overlay/SearchOverlayStatus";
import type { SearchActions, SearchOverlayProps } from "./search-overlay/shared";

/**
 * In-buffer search overlay (Ctrl+Shift+F).
 * Floats at the top-right of the terminal area.
 * Uses the xterm.js SearchAddon for highlighting and navigation.
 */
export function SearchOverlay({ style, className }: SearchOverlayProps = {}) {
  const open = useWorkspaceStore((s) => s.searchOpen);
  const toggle = useWorkspaceStore((s) => s.toggleSearch);
  const activePaneId = useWorkspaceStore((s) => s.activePaneId());
  const inputRef = useRef<HTMLInputElement>(null);
  const [query, setQuery] = useState("");
  const [matchCount, setMatchCount] = useState(0);
  const [currentIndex, setCurrentIndex] = useState(0);
  const [useRegex, setUseRegex] = useState(false);
  const [caseSensitive, setCaseSensitive] = useState(false);

  const searchOpts: TerminalSearchOptions = { regex: useRegex, caseSensitive };

  useEffect(() => {
    if (open) {
      setQuery("");
      setMatchCount(0);
      setCurrentIndex(0);
      setTimeout(() => inputRef.current?.focus(), 0);
    }
  }, [open]);

  const doSearch = useCallback(
    (q: string, opts?: TerminalSearchOptions) => {
      setQuery(q);
      const result = searchTerminal(activePaneId, q, "next", true, opts);
      setMatchCount(result.matchCount);
      setCurrentIndex(result.currentIndex);
    },
    [activePaneId]
  );

  const findNext = useCallback(() => {
    const result = searchTerminal(activePaneId, query, "next", false, searchOpts);
    setMatchCount(result.matchCount);
    setCurrentIndex(result.currentIndex);
  }, [activePaneId, query, searchOpts]);

  const findPrev = useCallback(() => {
    const result = searchTerminal(activePaneId, query, "prev", false, searchOpts);
    setMatchCount(result.matchCount);
    setCurrentIndex(result.currentIndex);
  }, [activePaneId, query, searchOpts]);

  useEffect(() => {
    if (!open) {
      clearTerminalSearch(activePaneId);
    }
  }, [activePaneId, open]);

  if (!open) return null;
  const clearAndClose = () => {
    clearTerminalSearch(activePaneId);
    toggle();
  };
  const actions: SearchActions = {
    activePaneId,
    query,
    searchOpts,
    setQuery,
    doSearch,
    findNext,
    findPrev,
    toggle,
    clearAndClose,
    caseSensitive,
    useRegex,
    setCaseSensitive,
    setUseRegex,
  };

  return (
    <Card
      style={{
        position: "absolute",
        top: 14,
        right: 16,
        zIndex: 100,
        minWidth: 320,
        ...(style ?? {}),
      }}
      className={[
        "amux-shell-card grid gap-[var(--space-3)] p-[var(--space-3)]",
        className ?? "",
      ].join(" ")}
    >
      <SearchOverlayHeader query={query} matchCount={matchCount} currentIndex={currentIndex} />
      <Separator />
      <SearchOverlayControls inputRef={inputRef} actions={actions} />
      <Separator />
      <SearchOverlayStatus activePaneId={activePaneId} />
    </Card>
  );
}
