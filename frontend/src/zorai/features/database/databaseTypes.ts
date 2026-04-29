export type DatabaseTableSummary = {
  name: string;
  tableType: string;
  rowCount: number | null;
  editable: boolean;
};

export type DatabaseColumnInfo = {
  name: string;
  declaredType: string;
  nullable: boolean;
  primaryKey: boolean;
  editable: boolean;
};

export type DatabaseRow = {
  rowid: number | null;
  values: Record<string, unknown>;
};

export type DatabaseTablePage = {
  tableName: string;
  columns: DatabaseColumnInfo[];
  rows: DatabaseRow[];
  totalRows: number;
  offset: number;
  limit: number;
  editable: boolean;
};

export type DatabaseRowUpdate = {
  rowid: number;
  values: Record<string, unknown>;
};

export type DatabaseSortDirection = "asc" | "desc";

export type DatabaseSortState = {
  column: string;
  direction: DatabaseSortDirection;
};
