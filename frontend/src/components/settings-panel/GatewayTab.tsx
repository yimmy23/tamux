import { useEffect, useState, type ReactNode } from "react";
import type { AgentSettings } from "../../lib/agentStore";
import { Badge, Button, Card, CardContent, CardDescription, CardHeader, CardTitle, Input } from "../ui";
import { GatewayHealth } from "./GatewaySettings";

type WhatsAppStatus = "disconnected" | "connecting" | "qr_ready" | "connected" | "error";

function SettingsSection({ title, description, children, badge }: { title: string; description?: string; children: ReactNode; badge?: ReactNode }) {
  return (
    <Card>
      <CardHeader>
        <div className="flex flex-wrap items-center gap-[var(--space-2)]">
          <CardTitle>{title}</CardTitle>
          {badge}
        </div>
        {description ? <CardDescription>{description}</CardDescription> : null}
      </CardHeader>
      <CardContent className="grid gap-[var(--space-3)]">{children}</CardContent>
    </Card>
  );
}

function SettingField({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className="grid gap-[var(--space-2)] border-t border-[var(--border-subtle)] pt-[var(--space-3)] first:border-t-0 first:pt-0 md:grid-cols-[minmax(0,12rem)_minmax(0,1fr)] md:items-center">
      <span className="text-[var(--text-sm)] text-[var(--text-secondary)]">{label}</span>
      {children}
    </div>
  );
}

function WhatsAppConnector() {
  const [status, setStatus] = useState<WhatsAppStatus>("disconnected");
  const [qrDataUrl, setQrDataUrl] = useState<string | null>(null);
  const [phoneInfo, setPhoneInfo] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void checkStatus();
    const amux = (window as any).tamux ?? (window as any).amux;
    const unsubQr = amux?.onWhatsAppQR?.((dataUrl: string) => {
      setQrDataUrl(dataUrl);
      setStatus("qr_ready");
      setError(null);
    });
    const unsubConnected = amux?.onWhatsAppConnected?.((info: { phone: string }) => {
      setPhoneInfo(info.phone);
      setStatus("connected");
      setQrDataUrl(null);
      setError(null);
    });
    const unsubDisconnected = amux?.onWhatsAppDisconnected?.(() => {
      setStatus("disconnected");
      setQrDataUrl(null);
      setPhoneInfo(null);
    });
    const unsubError = amux?.onWhatsAppError?.((message: string) => {
      setError(message);
      setStatus("error");
    });

    return () => {
      unsubQr?.();
      unsubConnected?.();
      unsubDisconnected?.();
      unsubError?.();
    };
  }, []);

  async function checkStatus() {
    try {
      const amux = (window as any).tamux ?? (window as any).amux;
      if (!amux?.whatsappStatus) return;
      const result = await amux.whatsappStatus();
      setStatus(result.status);
      if (result.phone) setPhoneInfo(result.phone);
    } catch {
    }
  }

  async function connect() {
    setStatus("connecting");
    setError(null);
    try {
      const amux = (window as any).tamux ?? (window as any).amux;
      if (!amux?.whatsappConnect) {
        setError("WhatsApp bridge not available. Install dependencies: npm install @whiskeysockets/baileys qrcode pino @hapi/boom");
        setStatus("error");
        return;
      }
      const result = await amux.whatsappConnect();
      if (result && !result.ok) {
        setError(result.error || "Failed to start WhatsApp bridge");
        setStatus("error");
      }
    } catch (connectError: any) {
      setError(connectError.message || "Failed to start WhatsApp connection");
      setStatus("error");
    }
  }

  async function disconnect() {
    try {
      await (window as any).amux?.whatsappDisconnect?.();
      setStatus("disconnected");
      setQrDataUrl(null);
      setPhoneInfo(null);
    } catch (disconnectError: any) {
      setError(disconnectError.message || "Failed to disconnect");
    }
  }

  const statusVariant = status === "connected" ? "success" : status === "error" ? "danger" : status === "qr_ready" ? "accent" : status === "connecting" ? "warning" : "default";
  const statusLabels: Record<WhatsAppStatus, string> = {
    disconnected: "Not connected",
    connecting: "Initializing...",
    qr_ready: "Scan QR code with WhatsApp",
    connected: "Connected",
    error: "Connection error",
  };

  return (
    <div className="grid gap-[var(--space-3)] rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--muted)]/50 p-[var(--space-3)]">
      <div className="flex flex-wrap items-center justify-between gap-[var(--space-2)]">
        <div className="flex flex-wrap items-center gap-[var(--space-2)]">
          <Badge variant={statusVariant}>{statusLabels[status]}</Badge>
          {phoneInfo && status === "connected" ? <span className="text-[var(--text-sm)] text-[var(--text-secondary)]">{phoneInfo}</span> : null}
        </div>
        <div className="flex flex-wrap gap-[var(--space-2)]">
          {status === "disconnected" || status === "error" ? <Button variant="primary" size="sm" onClick={connect}>Link Device</Button> : null}
          {status === "connected" ? <Button variant="destructive" size="sm" onClick={disconnect}>Disconnect</Button> : null}
          {status === "qr_ready" ? <Button variant="outline" size="sm" onClick={disconnect}>Cancel</Button> : null}
        </div>
      </div>

      {(status === "qr_ready" || status === "connecting") ? (
        <div className="flex flex-col items-center gap-[var(--space-3)] rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--card)] p-[var(--space-4)]">
          {qrDataUrl ? (
            <>
              <div className="rounded-[var(--radius-md)] bg-white p-[var(--space-3)]">
                <img src={qrDataUrl} alt="WhatsApp QR Code" style={{ width: 200, height: 200, imageRendering: "pixelated" }} />
              </div>
              <div className="text-center text-[var(--text-sm)] leading-6 text-[var(--text-secondary)]">Open WhatsApp on your phone → Settings → Linked Devices → Link a Device</div>
            </>
          ) : (
            <div className="py-[var(--space-6)] text-[var(--text-sm)] text-[var(--text-secondary)]">Generating QR code...</div>
          )}
        </div>
      ) : null}

      {status === "connected" ? <div className="rounded-[var(--radius-md)] border border-[var(--success-border)] bg-[var(--success-soft)] px-[var(--space-3)] py-[var(--space-2)] text-[var(--text-sm)] leading-6 text-[var(--text-secondary)]">Session is active. Messages from allowed contacts will be forwarded to amux. The session persists across restarts — no need to re-scan.</div> : null}
      {error ? <div className="rounded-[var(--radius-md)] border border-[var(--danger-border)] bg-[var(--danger-soft)] px-[var(--space-3)] py-[var(--space-2)] text-[var(--text-sm)] text-[var(--danger)]">{error}</div> : null}
    </div>
  );
}

export function GatewayTab({
  settings,
  updateSetting,
}: {
  settings: AgentSettings;
  updateSetting: <K extends keyof AgentSettings>(key: K, value: AgentSettings[K]) => void;
}) {
  return (
    <div className="grid gap-[var(--space-4)]">
      <SettingsSection title="Connection Status" description="Live health for Slack, Discord, Telegram, and WhatsApp bridges.">
        <GatewayHealth />
      </SettingsSection>

      <SettingsSection title="Gateway" description="Keep the gateway command prefix and token-backed wiring aligned with the current agent settings." badge={<Badge variant={settings.gateway_enabled ? "success" : "default"}>{settings.gateway_enabled ? "Enabled" : "Disabled"}</Badge>}>
        <SettingField label="Enable Gateway">
          <Button variant={settings.gateway_enabled ? "primary" : "outline"} size="sm" onClick={() => updateSetting("gateway_enabled", !settings.gateway_enabled)}>
            {settings.gateway_enabled ? "Enabled" : "Disabled"}
          </Button>
        </SettingField>
        <SettingField label="Command Prefix">
          <Input value={settings.gateway_command_prefix} onChange={(event) => updateSetting("gateway_command_prefix", event.target.value)} placeholder="!tamux" />
        </SettingField>
        <div className="text-[var(--text-sm)] leading-6 text-[var(--text-secondary)]">
          The gateway bridges chat platforms to tamux. The <code className="text-[var(--accent)]">tamux-gateway</code> binary reads tokens from environment variables (<code>TAMUX_SLACK_TOKEN</code>, <code>TAMUX_TELEGRAM_TOKEN</code>, etc.) or from the values configured below.
        </div>
      </SettingsSection>

      <div className="grid gap-[var(--space-4)] xl:grid-cols-2">
        <SettingsSection title="Slack" description="Slack bot credentials and channel filtering.">
          <SettingField label="Bot Token">
            <Input type="password" value={settings.slack_token} onChange={(event) => updateSetting("slack_token", event.target.value)} placeholder="xoxb-..." />
          </SettingField>
          <SettingField label="Channel Filter">
            <Input value={settings.slack_channel_filter} onChange={(event) => updateSetting("slack_channel_filter", event.target.value)} placeholder="general, ops (comma-separated)" />
          </SettingField>
        </SettingsSection>

        <SettingsSection title="Telegram" description="Telegram bot token and allowlist controls.">
          <SettingField label="Bot Token">
            <Input type="password" value={settings.telegram_token} onChange={(event) => updateSetting("telegram_token", event.target.value)} placeholder="123456:ABC-DEF..." />
          </SettingField>
          <SettingField label="Allowed Chats">
            <Input value={settings.telegram_allowed_chats} onChange={(event) => updateSetting("telegram_allowed_chats", event.target.value)} placeholder="chat_id1, chat_id2 (comma-separated)" />
          </SettingField>
        </SettingsSection>
      </div>

      <SettingsSection title="Discord" description="Discord bot wiring and user/channel filtering.">
        <SettingField label="Bot Token">
          <Input type="password" value={settings.discord_token} onChange={(event) => updateSetting("discord_token", event.target.value)} placeholder="Discord bot token" />
        </SettingField>
        <SettingField label="Channel Filter">
          <Input value={settings.discord_channel_filter} onChange={(event) => updateSetting("discord_channel_filter", event.target.value)} placeholder="channel_id1, channel_id2 (comma-separated)" />
        </SettingField>
        <SettingField label="Allowed Users">
          <Input value={settings.discord_allowed_users} onChange={(event) => updateSetting("discord_allowed_users", event.target.value)} placeholder="user_id1, user_id2 (comma-separated)" />
        </SettingField>
      </SettingsSection>

      <SettingsSection title="WhatsApp" description="QR-based device linking plus optional Business API fallback.">
        <WhatsAppConnector />
        <SettingField label="Allowed Contacts">
          <Input value={settings.whatsapp_allowed_contacts} onChange={(event) => updateSetting("whatsapp_allowed_contacts", event.target.value)} placeholder="+1234567890, +0987654321 (comma-separated)" />
        </SettingField>
        <div className="grid gap-[var(--space-2)] rounded-[var(--radius-lg)] border border-[var(--border)] bg-[var(--card)] p-[var(--space-3)]">
          <div className="flex items-center gap-[var(--space-2)]">
            <Badge variant="timeline">Alternative</Badge>
            <span className="text-[var(--text-sm)] font-medium text-[var(--text-primary)]">Business API</span>
          </div>
          <div className="text-[var(--text-sm)] leading-6 text-[var(--text-secondary)]">If you have a Meta Business account, you can use the Cloud API instead of QR linking.</div>
          <SettingField label="API Token">
            <Input type="password" value={settings.whatsapp_token} onChange={(event) => updateSetting("whatsapp_token", event.target.value)} placeholder="WhatsApp Business API token" />
          </SettingField>
          <SettingField label="Phone Number ID">
            <Input value={settings.whatsapp_phone_id} onChange={(event) => updateSetting("whatsapp_phone_id", event.target.value)} placeholder="Phone number ID from Meta dashboard" />
          </SettingField>
        </div>
      </SettingsSection>
    </div>
  );
}
