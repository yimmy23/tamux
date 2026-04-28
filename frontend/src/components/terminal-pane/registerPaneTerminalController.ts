import type { MutableRefObject } from "react";
import type { SearchAddon } from "@xterm/addon-search";
import type { SerializeAddon } from "@xterm/addon-serialize";
import type { Terminal } from "@xterm/xterm";
import { getBridge } from "@/lib/bridge";
import { registerTerminalController } from "@/lib/terminalRegistry";
import type { TerminalSendOptions } from "@/lib/terminalRegistry";
import {
  countSearchMatches,
  getRenderedTerminalText,
  getSearchableBufferText,
  stripAnsi,
} from "./utils";

export function registerPaneTerminalController({
  paneId,
  term,
  containerRef,
  searchAddon,
  serializeAddon,
  sessionReadyRef,
  sendTextInput,
}: {
  paneId: string;
  term: Terminal;
  containerRef: MutableRefObject<HTMLDivElement | null>;
  searchAddon: SearchAddon;
  serializeAddon: SerializeAddon;
  sessionReadyRef: MutableRefObject<boolean>;
  sendTextInput: (text: string, options?: TerminalSendOptions) => Promise<boolean>;
}) {
  let searchState = { query: "", matchCount: 0, currentIndex: 0 };

  searchAddon.onDidChangeResults((event) => {
    if (event) {
      searchState = {
        ...searchState,
        matchCount: event.resultCount,
        currentIndex: event.resultIndex,
      };
    }
  });

  return registerTerminalController(paneId, {
    sendText: (text, options) => sendTextInput(text, options),
    getSnapshot: () => stripAnsi(serializeAddon.serialize()),
    search: (query, direction = "next", reset = false, searchOptions) => {
      const normalizedQuery = query.trim();
      if (!normalizedQuery) {
        searchState = { query: "", matchCount: 0, currentIndex: 0 };
        searchAddon.clearDecorations();
        return searchState;
      }

      const shouldReset = reset || searchState.query !== normalizedQuery;
      const options = {
        incremental: shouldReset,
        regex: searchOptions?.regex ?? false,
        caseSensitive: searchOptions?.caseSensitive ?? false,
        decorations: {
          activeMatchBackground: "#f59e0b",
          matchBackground: "rgba(245, 158, 11, 0.28)",
          matchOverviewRuler: "rgba(245, 158, 11, 0.45)",
          activeMatchColorOverviewRuler: "#f59e0b",
        },
      };

      const found = direction === "prev"
        ? searchAddon.findPrevious(normalizedQuery, options)
        : searchAddon.findNext(normalizedQuery, options);

      const bufferSnapshot = getSearchableBufferText(term);
      const serializedSnapshot = stripAnsi(serializeAddon.serialize());
      const renderedSnapshot = getRenderedTerminalText(containerRef.current);

      const bufferCount = countSearchMatches(bufferSnapshot, normalizedQuery, searchOptions);
      const serializedCount = countSearchMatches(serializedSnapshot, normalizedQuery, searchOptions);
      const renderedCount = countSearchMatches(renderedSnapshot, normalizedQuery, searchOptions);
      const matchCount = Math.max(bufferCount, serializedCount, renderedCount, found ? 1 : 0);
      let currentIndex = searchState.currentIndex;

      if (shouldReset) {
        currentIndex = 0;
      } else if (matchCount > 0) {
        currentIndex = direction === "prev"
          ? (currentIndex - 1 + matchCount) % matchCount
          : (currentIndex + 1) % matchCount;
      } else {
        currentIndex = 0;
      }

      searchState = {
        query: normalizedQuery,
        matchCount,
        currentIndex,
      };
      return searchState;
    },
    clearSearch: () => {
      searchState = { query: "", matchCount: 0, currentIndex: 0 };
      searchAddon.clearDecorations();
    },
    searchHistory: async (query, limit = 8) => {
      const zorai = getBridge();
      if (!zorai?.searchManagedHistory || !sessionReadyRef.current) return false;
      await zorai.searchManagedHistory(paneId, query, limit);
      return true;
    },
    generateSkill: async (query, title) => {
      const zorai = getBridge();
      if (!zorai?.generateManagedSkill || !sessionReadyRef.current) return false;
      await zorai.generateManagedSkill(paneId, query ?? null, title ?? null);
      return true;
    },
    findSymbol: async (workspaceRoot, symbol, limit = 16) => {
      const zorai = getBridge();
      if (!zorai?.findManagedSymbol || !sessionReadyRef.current) return false;
      await zorai.findManagedSymbol(paneId, workspaceRoot, symbol, limit);
      return true;
    },
    listSnapshots: async (workspaceId) => {
      const zorai = getBridge();
      if (!zorai?.listSnapshots || !sessionReadyRef.current) return false;
      await zorai.listSnapshots(paneId, workspaceId ?? null);
      return true;
    },
    restoreSnapshot: async (snapshotId) => {
      const zorai = getBridge();
      if (!zorai?.restoreSnapshot || !sessionReadyRef.current) return false;
      await zorai.restoreSnapshot(paneId, snapshotId);
      return true;
    },
  });
}
