import type { DatabaseColumnInfo, DatabaseRow, DatabaseRowUpdate, DatabaseSortState, DatabaseTablePage } from "./databaseTypes";

export const DEFAULT_DATABASE_PAGE_SIZE = 100;
export const MAX_DATABASE_PAGE_SIZE = 500;

export function normalizeDatabasePageSize(value: number | undefined): number {
  if (!Number.isFinite(value)) return DEFAULT_DATABASE_PAGE_SIZE;
  return Math.min(MAX_DATABASE_PAGE_SIZE, Math.max(1, Math.trunc(value ?? DEFAULT_DATABASE_PAGE_SIZE)));
}

export function getLastDatabasePageOffset(totalRows: number, pageSize: number): number {
  const normalizedPageSize = normalizeDatabasePageSize(pageSize);
  if (totalRows <= normalizedPageSize) return 0;
  return Math.floor((totalRows - 1) / normalizedPageSize) * normalizedPageSize;
}

export function databaseDraftKey(rowid: number, columnName: string): string {
  return `${rowid}:${columnName}`;
}

export function getNextDatabaseSort(current: DatabaseSortState | null, column: string): DatabaseSortState | null {
  if (!current || current.column !== column) return { column, direction: "desc" };
  if (current.direction === "desc") return { column, direction: "asc" };
  return null;
}

export function sortDatabaseRowsForDisplay(page: DatabaseTablePage | null, sort: DatabaseSortState | null) {
  if (!page || !sort) return page?.rows ?? [];
  const directionMultiplier = sort.direction === "asc" ? 1 : -1;
  return [...page.rows].sort((left, right) => {
    const result = compareDatabaseValues(left.values[sort.column], right.values[sort.column]);
    if (result !== 0) return result * directionMultiplier;
    return (left.rowid ?? 0) - (right.rowid ?? 0);
  });
}

export type DatabaseCellCoordinate = {
  rowIndex: number;
  columnIndex: number;
};

export type DatabaseCellSelection = {
  anchor: DatabaseCellCoordinate;
  focus: DatabaseCellCoordinate;
} | null;

export function isDatabaseCellSelected(
  selection: DatabaseCellSelection,
  rowIndex: number,
  columnIndex: number,
): boolean {
  if (!selection) return false;
  const bounds = getDatabaseSelectionBounds(selection);
  return (
    rowIndex >= bounds.startRowIndex
    && rowIndex <= bounds.endRowIndex
    && columnIndex >= bounds.startColumnIndex
    && columnIndex <= bounds.endColumnIndex
  );
}

export function getDatabaseSelectedDraftKeys(
  page: DatabaseTablePage | null,
  rows: DatabaseRow[],
  columns: DatabaseColumnInfo[],
  selection: DatabaseCellSelection,
): string[] {
  if (!page?.editable || !selection) return [];
  const bounds = getDatabaseSelectionBounds(selection);
  const keys: string[] = [];
  for (let rowIndex = bounds.startRowIndex; rowIndex <= bounds.endRowIndex; rowIndex += 1) {
    const row = rows[rowIndex];
    if (!row || typeof row.rowid !== "number") continue;
    for (let columnIndex = bounds.startColumnIndex; columnIndex <= bounds.endColumnIndex; columnIndex += 1) {
      const column = columns[columnIndex];
      if (!column?.editable) continue;
      const originalValue = row.values[column.name];
      if (isBlobPlaceholder(originalValue)) continue;
      keys.push(databaseDraftKey(row.rowid, column.name));
    }
  }
  return keys;
}

export function applyDatabaseSelectionDraftValue(
  page: DatabaseTablePage | null,
  rows: DatabaseRow[],
  columns: DatabaseColumnInfo[],
  selection: DatabaseCellSelection,
  drafts: Record<string, string>,
  nextValue: string,
): Record<string, string> {
  const keys = getDatabaseSelectedDraftKeys(page, rows, columns, selection);
  if (keys.length === 0) return drafts;
  const nextDrafts = { ...drafts };
  for (const key of keys) nextDrafts[key] = nextValue;
  return nextDrafts;
}

function getDatabaseSelectionBounds(selection: NonNullable<DatabaseCellSelection>) {
  return {
    startRowIndex: Math.min(selection.anchor.rowIndex, selection.focus.rowIndex),
    endRowIndex: Math.max(selection.anchor.rowIndex, selection.focus.rowIndex),
    startColumnIndex: Math.min(selection.anchor.columnIndex, selection.focus.columnIndex),
    endColumnIndex: Math.max(selection.anchor.columnIndex, selection.focus.columnIndex),
  };
}

function compareDatabaseValues(left: unknown, right: unknown): number {
  if (left === right) return 0;
  if (left === null || left === undefined) return -1;
  if (right === null || right === undefined) return 1;
  if (typeof left === "number" && typeof right === "number") return left - right;
  if (typeof left === "boolean" && typeof right === "boolean") return Number(left) - Number(right);
  return String(left).localeCompare(String(right), undefined, { numeric: true, sensitivity: "base" });
}

export function displayDatabaseValue(value: unknown): string {
  if (value === null || value === undefined) return "";
  if (isBlobPlaceholder(value)) return `<BLOB ${value.bytes} bytes>`;
  if (typeof value === "object") return JSON.stringify(value);
  return String(value);
}

export function isBlobPlaceholder(value: unknown): value is { type: "blob"; bytes: number; preview?: string } {
  return Boolean(
    value
    && typeof value === "object"
    && (value as { type?: unknown }).type === "blob"
    && typeof (value as { bytes?: unknown }).bytes === "number",
  );
}

export function parseDatabaseDraftValue(originalValue: unknown, draftValue: string, nullable = false): unknown {
  const normalizedDraft = draftValue.trim().toLowerCase();
  if (nullable && (normalizedDraft === "" || normalizedDraft === "null")) return null;
  if (originalValue === null) return normalizedDraft === "" ? null : draftValue;
  if (typeof originalValue === "number") {
    const next = Number(draftValue);
    return Number.isFinite(next) ? next : draftValue;
  }
  if (typeof originalValue === "boolean") {
    if (normalizedDraft === "true" || normalizedDraft === "1") return true;
    if (normalizedDraft === "false" || normalizedDraft === "0") return false;
  }
  return draftValue;
}

export function buildDatabaseRowUpdates(
  page: DatabaseTablePage | null,
  drafts: Record<string, string>,
): DatabaseRowUpdate[] {
  if (!page?.editable) return [];
  const updatesByRow = new Map<number, Record<string, unknown>>();
  for (const row of page.rows) {
    if (typeof row.rowid !== "number") continue;
    for (const column of page.columns) {
      if (!column.editable) continue;
      const originalValue = row.values[column.name];
      if (isBlobPlaceholder(originalValue)) continue;
      const key = databaseDraftKey(row.rowid, column.name);
      if (!Object.prototype.hasOwnProperty.call(drafts, key)) continue;
      const parsedValue = parseDatabaseDraftValue(originalValue, drafts[key], column.nullable);
      if (JSON.stringify(parsedValue) === JSON.stringify(originalValue)) continue;
      const rowUpdate = updatesByRow.get(row.rowid) ?? {};
      rowUpdate[column.name] = parsedValue;
      updatesByRow.set(row.rowid, rowUpdate);
    }
  }
  return Array.from(updatesByRow.entries()).map(([rowid, values]) => ({ rowid, values }));
}
