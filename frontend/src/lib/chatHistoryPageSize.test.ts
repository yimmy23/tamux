import { describe, expect, it } from "vitest";
import { buildDaemonAgentConfig } from "./agentDaemonConfig.ts";
import {
  DEFAULT_CHAT_HISTORY_PAGE_SIZE,
  resolveReactChatHistoryMessageLimit,
} from "./chatHistoryPageSize.ts";
import {
  DEFAULT_AGENT_SETTINGS,
  normalizeAgentSettingsFromSource,
} from "./agentStore/settings.ts";

describe("daemon-backed chat history page size settings", () => {
  it("defaults both client page sizes to 100", () => {
    expect(DEFAULT_AGENT_SETTINGS.react_chat_history_page_size).toBe(100);
    expect(DEFAULT_AGENT_SETTINGS.tui_chat_history_page_size).toBe(100);
  });

  it("preserves normalized overrides including the React unlimited sentinel", () => {
    const normalized = normalizeAgentSettingsFromSource({
      react_chat_history_page_size: 0,
      tui_chat_history_page_size: 222,
    });

    expect(normalized.react_chat_history_page_size).toBe(0);
    expect(normalized.tui_chat_history_page_size).toBe(222);
  });

  it("serializes both daemon-backed page size settings", () => {
    const daemonConfig = buildDaemonAgentConfig({
      ...DEFAULT_AGENT_SETTINGS,
      react_chat_history_page_size: 0,
      tui_chat_history_page_size: 222,
    });

    expect(daemonConfig.react_chat_history_page_size).toBe(0);
    expect(daemonConfig.tui_chat_history_page_size).toBe(222);
  });

  it("keeps React thread fetches paged even when the old All sentinel is configured", () => {
    expect(resolveReactChatHistoryMessageLimit(0)).toBe(DEFAULT_CHAT_HISTORY_PAGE_SIZE);
  });
});
