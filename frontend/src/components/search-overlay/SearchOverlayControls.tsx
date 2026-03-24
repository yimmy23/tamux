import type { RefObject } from "react";
import { activeToggleBtn, navBtn, toggleBtn, type SearchActions } from "./shared";

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
        <div style={{ display: "flex", alignItems: "center", gap: 6 }}>
            <input
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
                style={{
                    background: "var(--bg-surface)",
                    border: "1px solid var(--border)",
                    borderRadius: 4,
                    color: "var(--text-primary)",
                    fontSize: 12,
                    padding: "3px 8px",
                    width: 220,
                    fontFamily: "inherit",
                    outline: "none",
                    flex: 1,
                }}
            />

            <button
                onClick={() => {
                    setCaseSensitive((value) => !value);
                    if (query) doSearch(query, { regex: useRegex, caseSensitive: !caseSensitive });
                }}
                style={{ ...toggleBtn, ...(caseSensitive ? activeToggleBtn : null) }}
                title="Match Case"
            >
                Aa
            </button>
            <button
                onClick={() => {
                    setUseRegex((value) => !value);
                    if (query) doSearch(query, { regex: !useRegex, caseSensitive });
                }}
                style={{ ...toggleBtn, ...(useRegex ? activeToggleBtn : null) }}
                title="Use Regular Expression"
            >
                .*
            </button>
            <button onClick={findPrev} style={navBtn} title="Previous (Shift+Enter)">↑</button>
            <button onClick={findNext} style={navBtn} title="Next (Enter)">↓</button>
            <button onClick={clearAndClose} style={navBtn} title="Close (Esc)">✕</button>
        </div>
    );
}
