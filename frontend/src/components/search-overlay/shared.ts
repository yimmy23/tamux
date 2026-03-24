import type { CSSProperties } from "react";
import type { TerminalSearchOptions } from "../../lib/terminalRegistry";

export type SearchOverlayProps = {
    style?: CSSProperties;
    className?: string;
};

export type SearchActions = {
    activePaneId: string | null;
    query: string;
    searchOpts: TerminalSearchOptions;
    setQuery: (value: string) => void;
    doSearch: (query: string, opts?: TerminalSearchOptions) => void;
    findNext: () => void;
    findPrev: () => void;
    toggle: () => void;
    clearAndClose: () => void;
    caseSensitive: boolean;
    useRegex: boolean;
    setCaseSensitive: React.Dispatch<React.SetStateAction<boolean>>;
    setUseRegex: React.Dispatch<React.SetStateAction<boolean>>;
};

export const navBtn: CSSProperties = {
    background: "rgba(255,255,255,0.04)",
    border: "1px solid rgba(255,255,255,0.08)",
    color: "var(--text-secondary)",
    cursor: "pointer",
    fontSize: 13,
    padding: "6px 8px",
    borderRadius: 0,
    lineHeight: 1,
};

export const toggleBtn: CSSProperties = {
    ...navBtn,
    fontSize: 11,
    fontWeight: 600,
    fontFamily: "var(--font-mono, monospace)",
    padding: "5px 7px",
};

export const activeToggleBtn: CSSProperties = {
    background: "rgba(245, 158, 11, 0.18)",
    borderColor: "rgba(245, 158, 11, 0.45)",
    color: "#f59e0b",
};
