import { Children, useEffect, useState, type ReactNode } from "react";
import { getBridge } from "@/lib/bridge";
import { formatCount, formatDate, type SessionUsageRow, type UsageStats } from "./ActivityUsageStats";

type UsageTab = "overview" | "providers" | "models" | "rankings";

const usageTabs: Array<{ id: UsageTab; label: string }> = [
  { id: "overview", label: "Overview" },
  { id: "providers", label: "Providers" },
  { id: "models", label: "Models" },
  { id: "rankings", label: "Rankings" },
];

const windows: Array<{ id: ZoraiStatisticsWindow; label: string }> = [
  { id: "today", label: "Today" },
  { id: "7d", label: "7d" },
  { id: "30d", label: "30d" },
  { id: "all", label: "All" },
];

export function UsagePanel({ stats }: { stats: UsageStats }) {
  const [tab, setTab] = useState<UsageTab>("overview");
  const [windowId, setWindowId] = useState<ZoraiStatisticsWindow>("all");
  const [snapshot, setSnapshot] = useState<ZoraiAgentStatisticsSnapshot | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const bridge = getBridge();
    if (!bridge?.agentGetStatistics) {
      setSnapshot(null);
      setError("Statistics bridge is unavailable.");
      return;
    }

    let cancelled = false;
    setLoading(true);
    setError(null);
    void bridge.agentGetStatistics(windowId).then((result) => {
      if (!cancelled) setSnapshot((result ?? null) as ZoraiAgentStatisticsSnapshot | null);
    }).catch((fetchError) => {
      if (!cancelled) {
        setSnapshot(null);
        setError(fetchError?.message || "Statistics request failed.");
      }
    }).finally(() => {
      if (!cancelled) setLoading(false);
    });

    return () => {
      cancelled = true;
    };
  }, [windowId]);

  return (
    <div className="zorai-usage-stack">
      <div className="zorai-toolbar">
        {usageTabs.map((item) => (
          <button key={item.id} type="button" className={["zorai-ghost-button", tab === item.id ? "zorai-button--active" : ""].filter(Boolean).join(" ")} onClick={() => setTab(item.id)}>
            {item.label}
          </button>
        ))}
        <span className="zorai-inline-note">Window</span>
        {windows.map((item) => (
          <button key={item.id} type="button" className={["zorai-ghost-button", windowId === item.id ? "zorai-button--active" : ""].filter(Boolean).join(" ")} onClick={() => setWindowId(item.id)}>
            {item.label}
          </button>
        ))}
      </div>

      {loading ? <div className="zorai-empty-state">Loading historical statistics...</div> : null}
      {error ? <div className="zorai-empty-state">{error} Local loaded-message total: {formatTokenValue(stats.totals.totalTokens)} tok.</div> : null}
      {snapshot ? <StatisticsBody snapshot={snapshot} tab={tab} /> : null}
      <SessionUsageTable rows={stats.sessionRows} />
    </div>
  );
}

function StatisticsBody({ snapshot, tab }: { snapshot: ZoraiAgentStatisticsSnapshot; tab: UsageTab }) {
  if (tab === "providers") return <ProviderTable rows={snapshot.providers} />;
  if (tab === "models") return <ModelTable rows={snapshot.models} />;
  if (tab === "rankings") return <Rankings snapshot={snapshot} />;

  return (
    <div className="zorai-usage-grid">
      <div className="zorai-panel zorai-usage-panel--wide">
        <div className="zorai-section-label">Totals</div>
        <div className="zorai-metric-grid">
          <UsageMetric label="Input tokens" value={`${formatTokenValue(snapshot.totals.input_tokens)} tok`} />
          <UsageMetric label="Output tokens" value={`${formatTokenValue(snapshot.totals.output_tokens)} tok`} />
          <UsageMetric label="Total tokens" value={`${formatTokenValue(snapshot.totals.total_tokens)} tok`} />
          <UsageMetric label="Total cost" value={formatCost(snapshot.totals.cost_usd)} />
          <UsageMetric label="Providers" value={String(snapshot.totals.provider_count)} />
          <UsageMetric label="Models" value={String(snapshot.totals.model_count)} />
        </div>
        <p className="zorai-empty-state">Generated at: {formatGeneratedAt(snapshot.generated_at)}</p>
        {snapshot.has_incomplete_cost_history ? (
          <p className="zorai-empty-state">Warning: historical cost is incomplete for this window. Older rows without stored cost are counted as $0.</p>
        ) : null}
      </div>
      <TopModelList title="Top Models By Tokens" rows={snapshot.top_models_by_tokens} value={(row) => `${formatTokenValue(row.total_tokens)} tok  ${formatCost(row.cost_usd)}`} />
      <TopModelList title="Top Models By Cost" rows={snapshot.top_models_by_cost} value={(row) => `${formatCost(row.cost_usd)}  ${formatTokenValue(row.total_tokens)} tok`} />
    </div>
  );
}

function ProviderTable({ rows }: { rows: ZoraiProviderStatisticsRow[] }) {
  return (
    <UsageTable title="Providers" columns={["Provider", "In", "Out", "Total", "Cost"]} empty="No provider statistics for this window.">
      {rows.map((row) => (
        <tr key={row.provider}><td>{row.provider}</td><td>{formatTokenValue(row.input_tokens)} tok</td><td>{formatTokenValue(row.output_tokens)} tok</td><td>{formatTokenValue(row.total_tokens)} tok</td><td>{formatCost(row.cost_usd)}</td></tr>
      ))}
    </UsageTable>
  );
}

function ModelTable({ rows }: { rows: ZoraiModelStatisticsRow[] }) {
  return (
    <UsageTable title="Provider / Model" columns={["Provider / Model", "In", "Out", "Total", "Cost"]} empty="No model statistics for this window.">
      {rows.map((row) => (
        <tr key={`${row.provider}/${row.model}`}><td>{row.provider} / {row.model}</td><td>{formatTokenValue(row.input_tokens)} tok</td><td>{formatTokenValue(row.output_tokens)} tok</td><td>{formatTokenValue(row.total_tokens)} tok</td><td>{formatCost(row.cost_usd)}</td></tr>
      ))}
    </UsageTable>
  );
}

function Rankings({ snapshot }: { snapshot: ZoraiAgentStatisticsSnapshot }) {
  return (
    <div className="zorai-usage-grid">
      <TopModelList title="Top Models By Tokens" rows={snapshot.top_models_by_tokens} value={(row) => `${formatTokenValue(row.total_tokens)} tok  ${formatCost(row.cost_usd)}`} />
      <TopModelList title="Top Models By Cost" rows={snapshot.top_models_by_cost} value={(row) => `${formatCost(row.cost_usd)}  ${formatTokenValue(row.total_tokens)} tok`} />
    </div>
  );
}

function SessionUsageTable({ rows }: { rows: SessionUsageRow[] }) {
  return (
    <UsageTable title="Sessions" columns={["Thread", "Provider models", "Req", "Total", "Audio", "Video", "Cost", "Updated"]} empty="No per-session usage has been loaded yet.">
      {rows.map((row) => (
        <tr key={row.threadId}>
          <td>{row.title}</td>
          <td>{Array.from(row.providerModels).join(", ") || "unknown"}</td>
          <td>{row.requests}</td>
          <td>{formatCount(row.totalTokens)}</td>
          <td>{formatCount(row.audioTokens)}</td>
          <td>{formatCount(row.videoTokens)}</td>
          <td>{formatCost(row.cost)}</td>
          <td>{formatDate(row.updatedAt)}</td>
        </tr>
      ))}
    </UsageTable>
  );
}

function TopModelList({ title, rows, value }: { title: string; rows: ZoraiModelStatisticsRow[]; value: (row: ZoraiModelStatisticsRow) => string }) {
  return (
    <div className="zorai-panel">
      <div className="zorai-section-label">{title}</div>
      {rows.length === 0 ? <div className="zorai-empty-state">No rankings for this window.</div> : rows.slice(0, 5).map((row, index) => (
        <div key={`${title}-${row.provider}-${row.model}`} className="zorai-usage-ranking-row">
          <strong>{index + 1}. {row.provider}/{row.model}</strong>
          <span>{value(row)}</span>
        </div>
      ))}
    </div>
  );
}

function UsageTable({ title, columns, empty, children }: { title: string; columns: string[]; empty: string; children: ReactNode }) {
  const hasRows = Children.count(children) > 0;

  return (
    <div className="zorai-panel zorai-usage-panel--wide">
      <div className="zorai-section-label">{title}</div>
      <div className="zorai-usage-table-wrap">
        <table className="zorai-usage-table">
          <thead><tr>{columns.map((column) => <th key={column}>{column}</th>)}</tr></thead>
          <tbody>{hasRows ? children : <tr><td colSpan={columns.length}>{empty}</td></tr>}</tbody>
        </table>
      </div>
    </div>
  );
}

function UsageMetric({ label, value }: { label: string; value: string }) {
  return <div className="zorai-metric-card"><strong>{value}</strong><span>{label}</span></div>;
}

function formatCost(value: number): string {
  return `$${Number(value || 0).toFixed(6)}`;
}

function formatGeneratedAt(value: number): string {
  return Number.isFinite(value) ? new Date(value).toLocaleString() : "unknown";
}

function formatTokenValue(tokens: number): string {
  const rounded = Math.max(0, Math.round(tokens || 0));
  if (rounded < 1000) return String(rounded);
  const units = ["", "k", "M", "B", "T", "P"];
  let value = rounded;
  let unit = 0;
  while (value >= 999_995 && unit + 1 < units.length) {
    value /= 1000;
    unit += 1;
  }
  return `${(value / 1000).toFixed(2)}${units[unit + 1] ?? ""}`;
}
