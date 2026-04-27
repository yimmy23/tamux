import { useWorkspaceStore } from "@/lib/workspaceStore";

const tools = [
  { id: "terminal", title: "Terminal", description: "Open managed terminal sessions as a secondary tool." },
  { id: "files", title: "Files", description: "Inspect and preview workspace files." },
  { id: "browser", title: "Browser", description: "Use browser surfaces when an agent workflow needs them." },
  { id: "history", title: "Command History", description: "Review command log and execution history." },
  { id: "system", title: "System Monitor", description: "Inspect host and daemon runtime status." },
  { id: "vault", title: "Session Vault", description: "Restore and review durable session state." },
];

export function ToolsRail() {
  return (
    <div className="zorai-rail-stack">
      <div className="zorai-section-label">Tools</div>
      {tools.map((tool) => (
        <div key={tool.id} className="zorai-rail-card">
          <strong>{tool.title}</strong>
          <span>{tool.description}</span>
        </div>
      ))}
    </div>
  );
}

export function ToolsView() {
  const toggleFileManager = useWorkspaceStore((state) => state.toggleFileManager);
  const toggleCommandLog = useWorkspaceStore((state) => state.toggleCommandLog);
  const toggleSessionVault = useWorkspaceStore((state) => state.toggleSessionVault);
  const toggleSystemMonitor = useWorkspaceStore((state) => state.toggleSystemMonitor);
  const actions = [
    { label: "File Manager", run: toggleFileManager },
    { label: "Command Log", run: toggleCommandLog },
    { label: "Session Vault", run: toggleSessionVault },
    { label: "System Monitor", run: toggleSystemMonitor },
  ];

  return (
    <section className="zorai-feature-surface">
      <div className="zorai-view-header">
        <div>
          <div className="zorai-kicker">Tools</div>
          <h1>Terminal and operator tools are available on demand.</h1>
          <p>
            Zorai keeps terminal multiplexing as a useful capability, but the default shell
            remains centered on threads, goals, and workspace orchestration.
          </p>
        </div>
      </div>
      <div className="zorai-card-grid">
        {actions.map((action) => (
          <button type="button" key={action.label} className="zorai-tool-card" onClick={action.run}>
            <strong>{action.label}</strong>
            <span>Open</span>
          </button>
        ))}
      </div>
    </section>
  );
}
