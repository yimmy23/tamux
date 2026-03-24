import { useEffect, useState, type ReactNode } from "react";
import { useAgentStore } from "../../lib/agentStore";
import { Badge, Button, Card, CardContent, CardDescription, CardHeader, CardTitle, Input } from "../ui";

interface DaemonGatewayConfig {
  slack_token?: string;
  discord_token?: string;
  telegram_token?: string;
  slack_channel_filter?: string;
  discord_channel_filter?: string;
  discord_allowed_users?: string;
  telegram_allowed_chats?: string;
  gateway_electron_bridges_enabled?: boolean;
}

function GatewaySection({ title, description, children }: { title: string; description?: string; children: ReactNode }) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>{title}</CardTitle>
        {description ? <CardDescription>{description}</CardDescription> : null}
      </CardHeader>
      <CardContent className="grid gap-[var(--space-3)]">{children}</CardContent>
    </Card>
  );
}

function GatewayField({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="grid gap-[var(--space-2)] border-t border-[var(--border-subtle)] pt-[var(--space-3)] first:border-t-0 first:pt-0 md:grid-cols-[minmax(0,12rem)_minmax(0,1fr)] md:items-center">
      <span className="text-[var(--text-sm)] text-[var(--text-secondary)]">{label}</span>
      {children}
    </div>
  );
}

export function GatewayHealth() {
  const gatewayStatuses = useAgentStore((s) => s.gatewayStatuses);
  const platforms = Object.entries(gatewayStatuses);

  if (platforms.length === 0) {
    return <div className="text-[var(--text-sm)] text-[var(--text-secondary)]">No gateway status events received yet. Start the daemon with a gateway token to see connection health.</div>;
  }

  const statusVariant = (status: string) => {
    if (status === "connected") return "success" as const;
    if (status === "error") return "danger" as const;
    return "default" as const;
  };

  return (
    <div className="grid gap-[var(--space-2)]">
      {platforms.map(([platform, info]) => (
        <div key={platform} className="flex flex-wrap items-center gap-[var(--space-2)] rounded-[var(--radius-md)] border border-[var(--border)] bg-[var(--muted)]/50 px-[var(--space-3)] py-[var(--space-2)]">
          <Badge variant={statusVariant(info.status)}>{platform}</Badge>
          <span className="text-[var(--text-sm)] text-[var(--text-secondary)]">{info.status}</span>
          {info.consecutiveFailures != null && info.consecutiveFailures > 0 ? <span className="text-[var(--text-xs)] text-[var(--danger)]">({info.consecutiveFailures} failures)</span> : null}
          {info.lastError ? <span className="truncate text-[var(--text-xs)] text-[var(--danger)]">{info.lastError}</span> : null}
        </div>
      ))}
    </div>
  );
}

export function GatewayConfigEditor() {
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [config, setConfig] = useState<DaemonGatewayConfig>({});
  const [dirty, setDirty] = useState(false);

  useEffect(() => {
    void loadConfig();
  }, []);

  async function loadConfig() {
    setLoading(true);
    try {
      const amux = (window as unknown as Record<string, unknown>).tamux ?? (window as unknown as Record<string, unknown>).amux;
      if (amux && typeof (amux as Record<string, unknown>).agentGetConfig === "function") {
        const fullConfig = await ((amux as Record<string, (...args: unknown[]) => Promise<Record<string, unknown>>>).agentGetConfig)();
        const gw = (fullConfig?.gateway ?? {}) as DaemonGatewayConfig;
        setConfig(gw);
      }
    } catch {
    }
    setLoading(false);
  }

  function updateField<K extends keyof DaemonGatewayConfig>(key: K, value: DaemonGatewayConfig[K]) {
    setConfig((prev) => ({ ...prev, [key]: value }));
    setDirty(true);
  }

  async function saveConfig() {
    setSaving(true);
    try {
      const amux = (window as unknown as Record<string, unknown>).tamux ?? (window as unknown as Record<string, unknown>).amux;
      if (amux && typeof (amux as Record<string, unknown>).agentSetConfigItem === "function") {
        const setItem = (amux as Record<string, (keyPath: string, value: unknown) => Promise<unknown>>).agentSetConfigItem;
        await setItem("gateway.slack_token", config.slack_token ?? "");
        await setItem("gateway.discord_token", config.discord_token ?? "");
        await setItem("gateway.telegram_token", config.telegram_token ?? "");
        await setItem("gateway.slack_channel_filter", config.slack_channel_filter ?? "");
        await setItem("gateway.discord_channel_filter", config.discord_channel_filter ?? "");
        await setItem("gateway.discord_allowed_users", config.discord_allowed_users ?? "");
        await setItem("gateway.telegram_allowed_chats", config.telegram_allowed_chats ?? "");
        setDirty(false);
      }
    } catch {
    }
    setSaving(false);
  }

  if (loading) {
    return <div className="text-[var(--text-sm)] text-[var(--text-secondary)]">Loading gateway config from daemon...</div>;
  }

  return (
    <GatewaySection title="Daemon Gateway Tokens" description="Tokens are read from and saved to the daemon config through IPC.">
      <GatewayField label="Slack Token">
        <Input type="password" value={config.slack_token ?? ""} onChange={(event) => updateField("slack_token", event.target.value)} placeholder="xoxb-..." />
      </GatewayField>
      <GatewayField label="Slack Channels">
        <Input value={config.slack_channel_filter ?? ""} onChange={(event) => updateField("slack_channel_filter", event.target.value)} placeholder="general, ops (comma-separated)" />
      </GatewayField>
      <GatewayField label="Discord Token">
        <Input type="password" value={config.discord_token ?? ""} onChange={(event) => updateField("discord_token", event.target.value)} placeholder="Discord bot token" />
      </GatewayField>
      <GatewayField label="Discord Channels">
        <Input value={config.discord_channel_filter ?? ""} onChange={(event) => updateField("discord_channel_filter", event.target.value)} placeholder="channel_id1, channel_id2 (comma-separated)" />
      </GatewayField>
      <GatewayField label="Discord Users">
        <Input value={config.discord_allowed_users ?? ""} onChange={(event) => updateField("discord_allowed_users", event.target.value)} placeholder="user_id1, user_id2 (comma-separated)" />
      </GatewayField>
      <GatewayField label="Telegram Token">
        <Input type="password" value={config.telegram_token ?? ""} onChange={(event) => updateField("telegram_token", event.target.value)} placeholder="123456:ABC-DEF..." />
      </GatewayField>
      <GatewayField label="Telegram Chats">
        <Input value={config.telegram_allowed_chats ?? ""} onChange={(event) => updateField("telegram_allowed_chats", event.target.value)} placeholder="chat_id1, chat_id2 (comma-separated)" />
      </GatewayField>
      {dirty ? (
        <div className="flex flex-wrap gap-[var(--space-2)]">
          <Button variant="primary" size="sm" onClick={saveConfig} disabled={saving}>{saving ? "Saving..." : "Save to Daemon"}</Button>
          <Button variant="outline" size="sm" onClick={() => void loadConfig()}>Discard</Button>
        </div>
      ) : null}
    </GatewaySection>
  );
}

export function GatewaySettingsPanel() {
  return (
    <div className="grid gap-[var(--space-4)]">
      <GatewaySection title="Connection Status" description="Live per-platform health from the agent store.">
        <GatewayHealth />
      </GatewaySection>
      <GatewayConfigEditor />
    </div>
  );
}
