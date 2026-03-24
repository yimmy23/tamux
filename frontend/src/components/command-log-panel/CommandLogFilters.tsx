import { Input, cn, fieldClassName } from "../ui";

export function CommandLogFilters({
  query,
  setQuery,
  workspaceFilter,
  setWorkspaceFilter,
  surfaceFilter,
  setSurfaceFilter,
  paneFilter,
  setPaneFilter,
  statusFilter,
  setStatusFilter,
  dateFilter,
  setDateFilter,
  workspaceOptions,
  surfaceOptions,
  paneOptions,
  close,
}: {
  query: string;
  setQuery: (value: string) => void;
  workspaceFilter: string;
  setWorkspaceFilter: (value: string) => void;
  surfaceFilter: string;
  setSurfaceFilter: (value: string) => void;
  paneFilter: string;
  setPaneFilter: (value: string) => void;
  statusFilter: string;
  setStatusFilter: (value: string) => void;
  dateFilter: string;
  setDateFilter: (value: string) => void;
  workspaceOptions: Array<{ id: string; name: string }>;
  surfaceOptions: Array<{ id: string; name: string }>;
  paneOptions: string[];
  close: () => void;
}) {
  return (
    <div className="grid gap-[var(--space-3)] border-b border-[var(--border-subtle)] bg-[var(--panel)]/45 px-[var(--space-5)] py-[var(--space-4)] xl:grid-cols-[minmax(0,2fr)_repeat(5,minmax(0,1fr))]">
      <Input
        type="text"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
        placeholder="Filter commands..."
        autoFocus
        onKeyDown={(e) => e.key === "Escape" && close()}
      />
      <select
        value={workspaceFilter}
        onChange={(e) => setWorkspaceFilter(e.target.value)}
        className={cn(fieldClassName, "appearance-none")}
      >
        <option value="all">All workspaces</option>
        {workspaceOptions.map(({ id, name }) => (
          <option key={id} value={id}>
            {name}
          </option>
        ))}
      </select>
      <select
        value={surfaceFilter}
        onChange={(e) => setSurfaceFilter(e.target.value)}
        className={cn(fieldClassName, "appearance-none")}
      >
        <option value="all">All surfaces</option>
        {surfaceOptions.map(({ id, name }) => (
          <option key={id} value={id}>
            {name}
          </option>
        ))}
      </select>
      <select
        value={paneFilter}
        onChange={(e) => setPaneFilter(e.target.value)}
        className={cn(fieldClassName, "appearance-none")}
      >
        <option value="all">All panes</option>
        {paneOptions.map((id) => (
          <option key={id} value={id}>
            {id}
          </option>
        ))}
      </select>
      <select
        value={statusFilter}
        onChange={(e) => setStatusFilter(e.target.value)}
        className={cn(fieldClassName, "appearance-none")}
      >
        <option value="all">Any status</option>
        <option value="running">Running</option>
        <option value="success">Success</option>
        <option value="failed">Failed</option>
      </select>
      <select
        value={dateFilter}
        onChange={(e) => setDateFilter(e.target.value)}
        className={cn(fieldClassName, "appearance-none")}
      >
        <option value="all">All dates</option>
        <option value="today">Today</option>
        <option value="7d">Last 7 days</option>
        <option value="30d">Last 30 days</option>
      </select>
    </div>
  );
}
