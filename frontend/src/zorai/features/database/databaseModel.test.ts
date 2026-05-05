import { describe, expect, it } from "vitest";
import { applyDatabaseSelectionDraftValue, buildDatabaseRowUpdates, getDatabaseSelectedDraftKeys, getLastDatabasePageOffset, getNextDatabaseSort, normalizeDatabasePageSize, sortDatabaseRowsForDisplay } from "./databaseModel";
import type { DatabaseTablePage } from "./databaseTypes";

const page: DatabaseTablePage = {
  tableName: "agent_messages",
  totalRows: 2,
  offset: 0,
  limit: 100,
  editable: true,
  columns: [
    { name: "id", declaredType: "INTEGER", nullable: false, primaryKey: true, editable: true },
    { name: "role", declaredType: "TEXT", nullable: true, primaryKey: false, editable: true },
    { name: "token_count", declaredType: "INTEGER", nullable: true, primaryKey: false, editable: true },
  ],
  rows: [
    { rowid: 1, values: { id: 1, role: "user", token_count: 12 } },
    { rowid: 2, values: { id: 2, role: "assistant", token_count: 24 } },
  ],
};

describe("databaseModel", () => {
  it("builds row updates with only changed values per column", () => {
    const updates = buildDatabaseRowUpdates(page, {
      "1:role": "operator",
      "1:token_count": "12",
      "2:token_count": "31",
    });

    expect(updates).toEqual([
      { rowid: 1, values: { role: "operator" } },
      { rowid: 2, values: { token_count: 31 } },
    ]);
  });

  it("serializes nullable numeric database drafts as null when cleared", () => {
    const nullablePage: DatabaseTablePage = {
      ...page,
      columns: [
        { name: "deleted_at", declaredType: "INTEGER", nullable: true, primaryKey: false, editable: true },
      ],
      rows: [
        { rowid: 7, values: { deleted_at: 1775659246145 } },
      ],
    };

    expect(buildDatabaseRowUpdates(nullablePage, { "7:deleted_at": "" })).toEqual([
      { rowid: 7, values: { deleted_at: null } },
    ]);
    expect(buildDatabaseRowUpdates(nullablePage, { "7:deleted_at": "null" })).toEqual([
      { rowid: 7, values: { deleted_at: null } },
    ]);
  });

  it("keeps pagination page size bounded with a 100 row default", () => {
    expect(normalizeDatabasePageSize(undefined)).toBe(100);
    expect(normalizeDatabasePageSize(0)).toBe(1);
    expect(normalizeDatabasePageSize(9999)).toBe(500);
  });

  it("calculates the offset for the last database page", () => {
    expect(getLastDatabasePageOffset(0, 100)).toBe(0);
    expect(getLastDatabasePageOffset(1, 100)).toBe(0);
    expect(getLastDatabasePageOffset(100, 100)).toBe(0);
    expect(getLastDatabasePageOffset(101, 100)).toBe(100);
    expect(getLastDatabasePageOffset(250, 100)).toBe(200);
  });

  it("cycles column sorting from none to descending to ascending and back to none", () => {
    expect(getNextDatabaseSort(null, "created_at")).toEqual({
      column: "created_at",
      direction: "desc",
    });
    expect(getNextDatabaseSort({ column: "created_at", direction: "desc" }, "created_at")).toEqual({
      column: "created_at",
      direction: "asc",
    });
    expect(getNextDatabaseSort({ column: "created_at", direction: "asc" }, "created_at")).toBeNull();
    expect(getNextDatabaseSort({ column: "name", direction: "desc" }, "created_at")).toEqual({
      column: "created_at",
      direction: "desc",
    });
  });

  it("sorts visible rows as a renderer fallback when backend rows arrive unsorted", () => {
    expect(sortDatabaseRowsForDisplay(page, { column: "role", direction: "asc" }).map((row) => row.values.role)).toEqual([
      "assistant",
      "user",
    ]);
    expect(sortDatabaseRowsForDisplay(page, { column: "token_count", direction: "desc" }).map((row) => row.values.token_count)).toEqual([
      24,
      12,
    ]);
    expect(sortDatabaseRowsForDisplay(page, null)).toBe(page.rows);
  });

  it("collects editable draft keys inside a rectangular database cell selection", () => {
    const keys = getDatabaseSelectedDraftKeys(page, page.rows, page.columns, {
      anchor: { rowIndex: 0, columnIndex: 1 },
      focus: { rowIndex: 1, columnIndex: 2 },
    });

    expect(keys).toEqual([
      "1:role",
      "1:token_count",
      "2:role",
      "2:token_count",
    ]);
  });

  it("applies a typed value to every editable cell in the database selection", () => {
    const drafts = applyDatabaseSelectionDraftValue(
      page,
      page.rows,
      page.columns,
      {
        anchor: { rowIndex: 0, columnIndex: 1 },
        focus: { rowIndex: 1, columnIndex: 2 },
      },
      { "1:role": "operator" },
      "bulk",
    );

    expect(drafts).toEqual({
      "1:role": "bulk",
      "1:token_count": "bulk",
      "2:role": "bulk",
      "2:token_count": "bulk",
    });
  });

  it("clears every editable cell in the database selection", () => {
    const drafts = applyDatabaseSelectionDraftValue(
      page,
      page.rows,
      page.columns,
      {
        anchor: { rowIndex: 0, columnIndex: 1 },
        focus: { rowIndex: 1, columnIndex: 2 },
      },
      {},
      "",
    );

    expect(drafts).toEqual({
      "1:role": "",
      "1:token_count": "",
      "2:role": "",
      "2:token_count": "",
    });
  });
});
