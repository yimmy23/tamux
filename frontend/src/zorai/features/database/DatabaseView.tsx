import { useEffect, useMemo, useState } from "react";
import { buildDatabaseRowUpdates, databaseDraftKey, displayDatabaseValue, getLastDatabasePageOffset, getNextDatabaseSort, isBlobPlaceholder, normalizeDatabasePageSize, sortDatabaseRowsForDisplay } from "./databaseModel";
import { listDatabaseTables, queryDatabaseRows, updateDatabaseRows } from "./databaseBridge";
import type { DatabaseSortState, DatabaseTablePage, DatabaseTableSummary } from "./databaseTypes";

type DatabaseViewProps = {
  activeTable: string | null;
  onSelectTable: (tableName: string) => void;
};

type DatabaseRailProps = DatabaseViewProps;

export function DatabaseRail({ activeTable, onSelectTable }: DatabaseRailProps) {
  const [tables, setTables] = useState<DatabaseTableSummary[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    listDatabaseTables()
      .then((nextTables) => {
        if (cancelled) return;
        setTables(nextTables);
        if (!activeTable && nextTables[0]) onSelectTable(nextTables[0].name);
      })
      .catch(() => {
        if (!cancelled) setTables([]);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [activeTable, onSelectTable]);

  return (
    <div className="zorai-rail-stack">
      {loading ? <div className="zorai-empty">Loading tables...</div> : null}
      {tables.map((table) => (
        <button
          type="button"
          key={table.name}
          className={[
            "zorai-rail-card",
            "zorai-rail-card--button",
            table.name === activeTable ? "zorai-rail-card--active" : "",
          ].filter(Boolean).join(" ")}
          onClick={() => onSelectTable(table.name)}
        >
          <strong>{table.name}</strong>
          <span>{table.rowCount ?? "-"} rows · {table.editable ? "editable" : table.tableType}</span>
        </button>
      ))}
      {!loading && tables.length === 0 ? <div className="zorai-empty">No database tables found.</div> : null}
    </div>
  );
}

export function DatabaseView({ activeTable }: DatabaseViewProps) {
  const [page, setPage] = useState<DatabaseTablePage | null>(null);
  const [offset, setOffset] = useState(0);
  const [pageSize, setPageSize] = useState(100);
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const [loading, setLoading] = useState(false);
  const [status, setStatus] = useState<string | null>(null);
  const [sort, setSort] = useState<DatabaseSortState | null>(null);

  useEffect(() => {
    setOffset(0);
    setDrafts({});
    setSort(null);
  }, [activeTable]);

  useEffect(() => {
    if (!activeTable) return;
    let cancelled = false;
    setLoading(true);
    setStatus(null);
    queryDatabaseRows(activeTable, offset, pageSize, sort)
      .then((nextPage) => {
        if (!cancelled) setPage(nextPage);
      })
      .catch((error) => {
        if (!cancelled) {
          setPage(null);
          setStatus(error?.message || "Database table could not be loaded.");
        }
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [activeTable, offset, pageSize, sort]);

  const updates = useMemo(() => buildDatabaseRowUpdates(page, drafts), [page, drafts]);
  const displayRows = useMemo(() => sortDatabaseRowsForDisplay(page, sort), [page, sort]);
  const displayColumns = page?.columns ?? [];
  const tableEditable = page?.editable ?? false;
  const dirtyCount = updates.reduce((total, update) => total + Object.keys(update.values).length, 0);
  const canGoPrevious = offset > 0;
  const canGoNext = page ? offset + page.limit < page.totalRows : false;
  const lastOffset = page ? getLastDatabasePageOffset(page.totalRows, pageSize) : 0;

  const refreshVisibleRows = async () => {
    if (!activeTable) return;
    setLoading(true);
    setStatus(null);
    setDrafts({});
    try {
      const nextPage = await queryDatabaseRows(activeTable, offset, pageSize, sort);
      setPage(nextPage);
      setStatus("Refreshed visible rows.");
    } catch (error: any) {
      setPage(null);
      setStatus(error?.message || "Refresh failed.");
    } finally {
      setLoading(false);
    }
  };

  const pushChanges = async () => {
    if (!activeTable || updates.length === 0) return;
    setLoading(true);
    setStatus(null);
    try {
      const updatedRows = await updateDatabaseRows(activeTable, updates);
      setDrafts({});
      const nextPage = await queryDatabaseRows(activeTable, offset, pageSize, sort);
      setPage(nextPage);
      setStatus(`Pushed ${updatedRows} row${updatedRows === 1 ? "" : "s"}.`);
    } catch (error: any) {
      setStatus(error?.message || "Push failed.");
    } finally {
      setLoading(false);
    }
  };

  if (!activeTable) {
    return <div className="zorai-empty-main"><h1>Database</h1><p>Select a table to inspect rows.</p></div>;
  }

  return (
    <section className="zorai-database-surface">
      <div className="zorai-database-toolbar">
        <div>
          <div className="zorai-kicker">SQLite Table</div>
          <h2>{activeTable}</h2>
        </div>
        <div className="zorai-database-controls">
          <button type="button" className="zorai-primary-button" onClick={pushChanges} disabled={loading || dirtyCount === 0}>
            Push{dirtyCount > 0 ? ` ${dirtyCount}` : ""}
          </button>
          <button type="button" className="zorai-ghost-button" disabled={loading || !page} onClick={refreshVisibleRows}>Refresh</button>
          <button type="button" className="zorai-ghost-button" disabled={!canGoPrevious || loading} onClick={() => setOffset(0)}>First</button>
          <button type="button" className="zorai-ghost-button" disabled={!canGoPrevious || loading} onClick={() => setOffset(Math.max(0, offset - pageSize))}>Prev</button>
          <span>{page ? `${offset + 1}-${Math.min(offset + page.limit, page.totalRows)} / ${page.totalRows}` : "0 / 0"}</span>
          <button type="button" className="zorai-ghost-button" disabled={!canGoNext || loading} onClick={() => setOffset(offset + pageSize)}>Next</button>
          <button type="button" className="zorai-ghost-button" disabled={loading || !page || offset === lastOffset} onClick={() => setOffset(lastOffset)}>Last</button>
          <label>
            <span>Rows</span>
            <input
              className="zorai-database-page-size"
              type="number"
              min={1}
              max={500}
              value={pageSize}
              onChange={(event) => {
                setPageSize(normalizeDatabasePageSize(Number(event.target.value)));
                setOffset(0);
              }}
            />
          </label>
        </div>
      </div>

      {status ? <div className="zorai-inline-note">{status}</div> : null}
      <div className="zorai-database-table-wrap">
        <table className="zorai-database-table">
          <thead>
            <tr>
              <th>rowid</th>
              {displayColumns.map((column) => (
                <th key={column.name}>
                  <button
                    type="button"
                    className="zorai-database-sort-button"
                    onClick={() => {
                      setSort((current) => getNextDatabaseSort(current, column.name));
                      setOffset(0);
                      setDrafts({});
                    }}
                    title={`Sort by ${column.name}`}
                  >
                    <span>{column.name}</span>
                    <span className="zorai-database-sort-icon" aria-hidden="true">
                      {sort?.column === column.name ? (sort.direction === "desc" ? "⌄" : "⌃") : ""}
                    </span>
                  </button>
                  <small>{column.primaryKey ? "PK" : column.declaredType || "value"}</small>
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {displayRows.map((row, rowIndex) => (
              <tr key={row.rowid ?? rowIndex}>
                <td className="zorai-database-rowid">{row.rowid ?? "-"}</td>
                {displayColumns.map((column) => {
                  const originalValue = row.values[column.name];
                  const editable = tableEditable && typeof row.rowid === "number" && column.editable && !isBlobPlaceholder(originalValue);
                  const key = typeof row.rowid === "number" ? databaseDraftKey(row.rowid, column.name) : `${rowIndex}:${column.name}`;
                  const isDirty = Object.prototype.hasOwnProperty.call(drafts, key);
                  const value = isDirty ? drafts[key] : displayDatabaseValue(originalValue);
                  return (
                    <DatabaseCellEditor
                      key={column.name}
                      editable={editable}
                      loading={loading}
                      isDirty={isDirty}
                      value={value}
                      onChange={(nextValue) => setDrafts((current) => ({ ...current, [key]: nextValue }))}
                    />
                  );
                })}
              </tr>
            ))}
          </tbody>
        </table>
        {!loading && page?.rows.length === 0 ? <div className="zorai-empty">No rows on this page.</div> : null}
        {loading ? <div className="zorai-empty">Loading database rows...</div> : null}
      </div>
    </section>
  );
}

type DatabaseCellEditorProps = {
  editable: boolean;
  loading: boolean;
  isDirty: boolean;
  value: string;
  onChange: (value: string) => void;
};

export function DatabaseCellEditor({ editable, loading, isDirty, value, onChange }: DatabaseCellEditorProps) {
  return (
    <td className={isDirty ? "zorai-database-cell--dirty" : ""}>
      <textarea
        className="zorai-database-cell-editor"
        value={value}
        disabled={!editable || loading}
        rows={2}
        spellCheck={false}
        onChange={(event) => onChange(event.target.value)}
      />
    </td>
  );
}
