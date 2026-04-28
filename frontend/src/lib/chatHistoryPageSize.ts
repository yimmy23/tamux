export const DEFAULT_CHAT_HISTORY_PAGE_SIZE = 100;
export const MIN_CHAT_HISTORY_PAGE_SIZE = 25;
export const MAX_TUI_CHAT_HISTORY_PAGE_SIZE = 500;
export const REACT_CHAT_HISTORY_PAGE_SIZE_ALL = 0;

function normalizeFiniteInteger(value: unknown): number | null {
  const parsed = Number.parseInt(String(value), 10);
  return Number.isFinite(parsed) ? parsed : null;
}

export function normalizeTuiChatHistoryPageSize(value: unknown): number {
  const parsed = normalizeFiniteInteger(value);
  if (parsed === null) {
    return DEFAULT_CHAT_HISTORY_PAGE_SIZE;
  }
  return Math.min(
    MAX_TUI_CHAT_HISTORY_PAGE_SIZE,
    Math.max(MIN_CHAT_HISTORY_PAGE_SIZE, parsed),
  );
}

export function normalizeReactChatHistoryPageSize(value: unknown): number {
  const parsed = normalizeFiniteInteger(value);
  if (parsed === null) {
    return DEFAULT_CHAT_HISTORY_PAGE_SIZE;
  }
  if (parsed <= REACT_CHAT_HISTORY_PAGE_SIZE_ALL) {
    return REACT_CHAT_HISTORY_PAGE_SIZE_ALL;
  }
  return Math.max(MIN_CHAT_HISTORY_PAGE_SIZE, parsed);
}

export function resolveReactChatHistoryMessageLimit(
  pageSize: number,
): number {
  const normalized = normalizeReactChatHistoryPageSize(pageSize);
  return normalized === REACT_CHAT_HISTORY_PAGE_SIZE_ALL
    ? DEFAULT_CHAT_HISTORY_PAGE_SIZE
    : normalized;
}
