import { useState, type ReactNode } from "react";
import type { AmuxSettings } from "../../lib/types";
import { Badge, Button, Card, CardContent, CardDescription, CardHeader, CardTitle, Input } from "../ui";
import { NumberInput, SelectInput, SettingRow, type SettingsUpdater, TextInput, Toggle } from "./shared";

type McpServerRow = {
  name: string;
  command: string;
  argsText: string;
};

function SettingsSection({ title, description, badge, children }: { title: string; description?: string; badge?: ReactNode; children: ReactNode }) {
  return (
    <Card>
      <CardHeader>
        <div className="flex flex-wrap items-center gap-[var(--space-2)]">
          <CardTitle>{title}</CardTitle>
          {badge}
        </div>
        {description ? <CardDescription>{description}</CardDescription> : null}
      </CardHeader>
      <CardContent className="grid gap-[var(--space-2)]">{children}</CardContent>
    </Card>
  );
}

export function BehaviorTab({
  settings,
  updateSetting,
}: {
  settings: AmuxSettings;
  updateSetting: SettingsUpdater;
}) {
  const [lspHealth, setLspHealth] = useState<Record<string, boolean> | null>(null);
  const [mcpHealth, setMcpHealth] = useState<Array<{ name: string; command: string; exists: boolean; error?: string }> | null>(null);
  const [mcpError, setMcpError] = useState<string | null>(null);

  const parseMcpServers = () => {
    try {
      const parsed = JSON.parse(settings.mcpServersJson || "{}");
      if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) {
        throw new Error("MCP servers must be a JSON object keyed by server name.");
      }
      return { servers: parsed as Record<string, { command?: string; args?: string[] }>, error: null as string | null };
    } catch (error: any) {
      return { servers: null, error: error?.message || "Invalid MCP servers JSON" };
    }
  };

  const mcpRows: McpServerRow[] = (() => {
    const parsed = parseMcpServers();
    if (!parsed.servers) return [];
    return Object.entries(parsed.servers).map(([name, value]) => ({
      name,
      command: typeof value?.command === "string" ? value.command : "",
      argsText: Array.isArray(value?.args) ? value.args.join(" ") : "",
    }));
  })();
  const parsedMcp = parseMcpServers();

  const saveMcpRows = (rows: McpServerRow[]) => {
    const next: Record<string, { command: string; args?: string[] }> = {};
    for (const row of rows) {
      const key = row.name.trim();
      const command = row.command.trim();
      if (!key || !command) continue;
      const args = row.argsText.split(/\s+/).map((value) => value.trim()).filter(Boolean);
      next[key] = args.length > 0 ? { command, args } : { command };
    }
    setMcpError(null);
    updateSetting("mcpServersJson", JSON.stringify(next, null, 2));
  };

  const updateMcpRow = (index: number, patch: Partial<McpServerRow>) => {
    saveMcpRows(mcpRows.map((row, rowIndex) => (rowIndex === index ? { ...row, ...patch } : row)));
  };

  const runLspHealthCheck = async () => {
    try {
      const result = await ((window as any).tamux ?? (window as any).amux)?.checkLspHealth?.();
      setLspHealth(result ?? null);
    } catch {
      setLspHealth(null);
    }
  };

  const runMcpHealthCheck = async () => {
    const parsed = parseMcpServers();
    if (!parsed.servers) {
      setMcpError(parsed.error || "Invalid MCP servers JSON");
      return;
    }
    setMcpError(null);
    try {
      const result = await ((window as any).tamux ?? (window as any).amux)?.checkMcpHealth?.(parsed.servers);
      setMcpHealth(Array.isArray(result) ? result : null);
    } catch {
      setMcpHealth(null);
    }
  };

  const copyMcpConfig = async () => {
    const parsed = parseMcpServers();
    if (!parsed.servers) {
      setMcpError(parsed.error || "Invalid MCP servers JSON");
      return;
    }
    setMcpError(null);
    const payload = JSON.stringify({ mcpServers: parsed.servers }, null, 2);
    try {
      await navigator.clipboard.writeText(payload);
    } catch {
      setMcpError("Failed to copy MCP config to clipboard.");
    }
  };

  return (
    <div className="grid gap-[var(--space-4)]">
      <div className="grid gap-[var(--space-4)] xl:grid-cols-2">
        <SettingsSection title="Session" description="Startup, shutdown, and persistence controls for operator sessions.">
          <SettingRow label="Restore on Startup">
            <Toggle value={settings.restoreSessionOnStartup} onChange={(value) => updateSetting("restoreSessionOnStartup", value)} />
          </SettingRow>
          <SettingRow label="Confirm on Close">
            <Toggle value={settings.confirmOnClose} onChange={(value) => updateSetting("confirmOnClose", value)} />
          </SettingRow>
          <SettingRow label="Auto-save Interval (s)">
            <NumberInput value={settings.autoSaveIntervalSeconds} min={5} max={300} onChange={(value) => updateSetting("autoSaveIntervalSeconds", value)} />
          </SettingRow>
        </SettingsSection>

        <SettingsSection title="Clipboard" description="Interaction behavior for text selection and links inside terminals.">
          <SettingRow label="Auto-copy on Select">
            <Toggle value={settings.autoCopyOnSelect} onChange={(value) => updateSetting("autoCopyOnSelect", value)} />
          </SettingRow>
          <SettingRow label="Ctrl+Click Opens URLs">
            <Toggle value={settings.ctrlClickOpensUrls} onChange={(value) => updateSetting("ctrlClickOpensUrls", value)} />
          </SettingRow>
        </SettingsSection>
      </div>

      <SettingsSection title="Logging & Transcripts" description="Retention and transcript capture controls for command history and shell replay.">
        <SettingRow label="Capture Transcripts on Close">
          <Toggle value={settings.captureTranscriptsOnClose} onChange={(value) => updateSetting("captureTranscriptsOnClose", value)} />
        </SettingRow>
        <SettingRow label="Capture Transcripts on Clear">
          <Toggle value={settings.captureTranscriptsOnClear} onChange={(value) => updateSetting("captureTranscriptsOnClear", value)} />
        </SettingRow>
        <SettingRow label="Command Log Retention (days)">
          <NumberInput value={settings.commandLogRetentionDays} min={0} max={365} onChange={(value) => updateSetting("commandLogRetentionDays", value)} />
        </SettingRow>
        <SettingRow label="Transcript Retention (days)">
          <NumberInput value={settings.transcriptRetentionDays} min={0} max={365} onChange={(value) => updateSetting("transcriptRetentionDays", value)} />
        </SettingRow>
      </SettingsSection>

      <SettingsSection title="Infrastructure" description="Security, snapshots, Cerbos, MCP wiring, and health checks." badge={<Badge variant="warning">Operational</Badge>}>
        <SettingRow label="Security Level">
          <SelectInput value={settings.securityLevel} options={["highest", "moderate", "lowest", "yolo"]} onChange={(value) => updateSetting("securityLevel", value as AmuxSettings["securityLevel"])} />
        </SettingRow>
        <SettingRow label="Sandbox Isolation">
          <Toggle value={settings.sandboxEnabled} onChange={(value) => updateSetting("sandboxEnabled", value)} />
        </SettingRow>
        <SettingRow label="Sandbox Network Access">
          <Toggle value={settings.sandboxNetworkEnabled} onChange={(value) => updateSetting("sandboxNetworkEnabled", value)} />
        </SettingRow>
        <SettingRow label="Snapshot Backend">
          <SelectInput value={settings.snapshotBackend} options={["tar", "zfs", "btrfs"]} onChange={(value) => updateSetting("snapshotBackend", value as "tar" | "zfs" | "btrfs")} />
        </SettingRow>
        <SettingRow label="Snapshot Max Count">
          <NumberInput value={settings.snapshotMaxCount} min={1} max={1000} onChange={(value) => updateSetting("snapshotMaxCount", Math.max(1, Math.floor(value)))} />
        </SettingRow>
        <SettingRow label="Snapshot Max Size (MB)">
          <NumberInput value={settings.snapshotMaxSizeMb} min={1024} max={500000} step={1024} onChange={(value) => updateSetting("snapshotMaxSizeMb", Math.max(1024, Math.floor(value)))} />
        </SettingRow>
        <SettingRow label="Snapshot Auto Cleanup">
          <Toggle value={settings.snapshotAutoCleanup} onChange={(value) => updateSetting("snapshotAutoCleanup", value)} />
        </SettingRow>
        <SettingRow label="WORM Integrity Checks">
          <Toggle value={settings.wormIntegrityEnabled} onChange={(value) => updateSetting("wormIntegrityEnabled", value)} />
        </SettingRow>
        <SettingRow label="Cerbos Endpoint">
          <TextInput value={settings.cerbosEndpoint} onChange={(value) => updateSetting("cerbosEndpoint", value)} placeholder="http://localhost:3593" />
        </SettingRow>

        <div className="grid gap-[var(--space-3)] rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--muted)]/50 p-[var(--space-3)]">
          <div className="flex flex-wrap items-center justify-between gap-[var(--space-2)]">
            <div>
              <div className="text-[var(--text-sm)] font-medium text-[var(--text-primary)]">MCP Servers</div>
              <div className="text-[var(--text-sm)] text-[var(--text-secondary)]">Edit bridge definitions without changing the JSON store contract.</div>
            </div>
            <Badge variant={parsedMcp.error ? "danger" : "accent"}>{parsedMcp.error ? "Config error" : `${mcpRows.length} configured`}</Badge>
          </div>

          {parsedMcp.error ? <div className="text-[var(--text-sm)] text-[var(--danger)]">{parsedMcp.error}</div> : null}
          {mcpRows.length === 0 ? <div className="text-[var(--text-sm)] text-[var(--text-secondary)]">No MCP servers configured yet.</div> : null}

          <div className="grid gap-[var(--space-3)]">
            {mcpRows.map((row, index) => (
              <div key={`mcp-row-${index}`} className="grid gap-[var(--space-2)] rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--card)] p-[var(--space-3)]">
                <div className="grid gap-[var(--space-2)] md:grid-cols-[1fr_2fr_auto]">
                  <Input value={row.name} onChange={(event) => updateMcpRow(index, { name: event.target.value })} placeholder="name" />
                  <Input value={row.command} onChange={(event) => updateMcpRow(index, { command: event.target.value })} placeholder="command" />
                  <Button variant="destructive" size="sm" onClick={() => saveMcpRows(mcpRows.filter((_, rowIndex) => rowIndex !== index))}>Remove</Button>
                </div>
                <Input value={row.argsText} onChange={(event) => updateMcpRow(index, { argsText: event.target.value })} placeholder="args (space separated)" />
              </div>
            ))}
          </div>

          <div className="flex flex-wrap gap-[var(--space-2)]">
            <Button variant="primary" size="sm" onClick={() => saveMcpRows([...mcpRows, { name: `server${mcpRows.length + 1}`, command: "", argsText: "" }])}>Add MCP Server</Button>
            <Button variant="outline" size="sm" onClick={runLspHealthCheck}>Check LSP Health</Button>
            <Button variant="outline" size="sm" onClick={runMcpHealthCheck}>Check MCP Health</Button>
            <Button variant="ghost" size="sm" onClick={copyMcpConfig}>Copy MCP Config</Button>
          </div>

          {mcpError ? <div className="text-[var(--text-sm)] text-[var(--danger)]">{mcpError}</div> : null}
          {lspHealth ? (
            <div className="flex flex-wrap gap-[var(--space-2)] text-[var(--text-xs)]">
              <Badge variant={lspHealth.rustAnalyzer ? "success" : "danger"}>rust-analyzer {lspHealth.rustAnalyzer ? "ok" : "missing"}</Badge>
              <Badge variant={lspHealth.typescriptLanguageServer ? "success" : "danger"}>typescript-language-server {lspHealth.typescriptLanguageServer ? "ok" : "missing"}</Badge>
              <Badge variant={lspHealth.pyrightLangserver ? "success" : "danger"}>pyright-langserver {lspHealth.pyrightLangserver ? "ok" : "missing"}</Badge>
            </div>
          ) : null}
          {mcpHealth ? (
            <div className="grid gap-[var(--space-2)] text-[var(--text-sm)] text-[var(--text-secondary)]">
              {mcpHealth.map((row) => (
                <div key={`${row.name}:${row.command}`} className="flex flex-wrap items-center gap-[var(--space-2)] rounded-[var(--radius-md)] border border-[var(--border-subtle)] bg-[var(--bg-secondary)]/60 px-[var(--space-3)] py-[var(--space-2)]">
                  <Badge variant={row.exists ? "success" : "danger"}>{row.name}</Badge>
                  <span className="font-mono text-[var(--text-xs)] text-[var(--text-muted)]">{row.command || "(missing command)"}</span>
                  <span>{row.exists ? "ok" : row.error || "missing"}</span>
                </div>
              ))}
            </div>
          ) : null}
        </div>
      </SettingsSection>
    </div>
  );
}
