import type { RefObject } from "react";
import { Button, Input } from "../ui";
import type { SearchActions } from "./shared";

export function SearchOverlayControls({
  inputRef,
  actions,
}: {
  inputRef: RefObject<HTMLInputElement | null>;
  actions: SearchActions;
}) {
  const {
    query,
    searchOpts,
    doSearch,
    findNext,
    findPrev,
    caseSensitive,
    useRegex,
    setCaseSensitive,
    setUseRegex,
    clearAndClose,
  } = actions;

  return (
    <div className="flex flex-col gap-[var(--space-2)]">
      <Input
        ref={inputRef}
        type="text"
        value={query}
        onChange={(event) => doSearch(event.target.value, searchOpts)}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            clearAndClose();
          } else if (event.key === "Enter") {
            if (event.shiftKey) findPrev();
            else findNext();
          }
        }}
        placeholder="Search in buffer..."
        className="h-9"
      />

      <div className="flex flex-wrap items-center gap-[var(--space-2)]">
        <Button
          variant={caseSensitive ? "primary" : "outline"}
          size="sm"
          onClick={() => {
            setCaseSensitive((value) => !value);
            if (query) doSearch(query, { regex: useRegex, caseSensitive: !caseSensitive });
          }}
          title="Match Case"
          className="font-mono"
        >
          Aa
        </Button>
        <Button
          variant={useRegex ? "primary" : "outline"}
          size="sm"
          onClick={() => {
            setUseRegex((value) => !value);
            if (query) doSearch(query, { regex: !useRegex, caseSensitive });
          }}
          title="Use Regular Expression"
          className="font-mono"
        >
          .*
        </Button>
        <Button variant="outline" size="sm" onClick={findPrev} title="Previous (Shift+Enter)">
          ↑
        </Button>
        <Button variant="outline" size="sm" onClick={findNext} title="Next (Enter)">
          ↓
        </Button>
        <Button variant="ghost" size="sm" onClick={clearAndClose} title="Close (Esc)">
          ✕
        </Button>
      </div>
    </div>
  );
}
