import { useEffect, useMemo, useState } from "react";
import { getBridge } from "@/lib/bridge";
import type { AgentSettings } from "../../lib/agentStore";
import { PasswordInput, Section, SettingRow, TextAreaInput, TextInput, Toggle, smallBtnStyle } from "./shared";
import { GatewayHealth } from "./GatewaySettings";
import { getWhatsAppAllowlistState, type WhatsAppAllowlistState } from "./whatsappAllowlist";

type WhatsAppStatus = "disconnected" | "connecting" | "qr_ready" | "connected" | "error";

function normalizeWhatsAppStatus(raw: unknown): WhatsAppStatus {
    if (raw === "starting" || raw === "connecting") return "connecting";
    if (raw === "qr_ready" || raw === "connected" || raw === "error" || raw === "disconnected") {
        return raw;
    }
    return "disconnected";
}

function WhatsAppConnector({ allowlistState }: { allowlistState: WhatsAppAllowlistState }) {
    const [status, setStatus] = useState<WhatsAppStatus>("disconnected");
    const [qrDataUrl, setQrDataUrl] = useState<string | null>(null);
    const [phoneInfo, setPhoneInfo] = useState<string | null>(null);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        checkStatus();

        const amux = getBridge();
        const unsubWhatsAppQr = amux?.onWhatsAppQR?.((dataUrl: string | null) => {
            if (dataUrl && dataUrl.trim()) {
                setQrDataUrl(dataUrl);
                setStatus("qr_ready");
                setError(null);
            }
        });
        const unsubWhatsAppConnected = amux?.onWhatsAppConnected?.((info: { phone?: string | null }) => {
            setPhoneInfo(info?.phone ?? null);
            setStatus("connected");
            setQrDataUrl(null);
            setError(null);
        });
        const unsubWhatsAppDisconnected = amux?.onWhatsAppDisconnected?.((payload: { reason?: string | null } | null | undefined) => {
            setStatus("disconnected");
            setQrDataUrl(null);
            setPhoneInfo(null);
            if (typeof payload?.reason === "string" && payload.reason.trim()) {
                setError(payload.reason);
            } else {
                setError(null);
            }
        });
        const unsubWhatsAppError = amux?.onWhatsAppError?.((message: string) => {
            setError(typeof message === "string" ? message : "WhatsApp link error");
            setStatus("error");
        });

        return () => {
            unsubWhatsAppQr?.();
            unsubWhatsAppConnected?.();
            unsubWhatsAppDisconnected?.();
            unsubWhatsAppError?.();
        };
    }, []);

    async function checkStatus() {
        try {
            const amux = getBridge();
            if (!amux?.whatsappStatus) return;
            const result = await amux.whatsappStatus();
            const nextStatus = normalizeWhatsAppStatus(result.status);
            setStatus(nextStatus);
            if (result.phone) setPhoneInfo(result.phone);
            if (nextStatus !== "qr_ready") setQrDataUrl(null);
            if (typeof result.lastError === "string" && result.lastError.trim()) {
                setError(result.lastError);
            }
        } catch {
            // IPC not available yet
        }
    }

    async function connect() {
        if (!allowlistState.hasValidContacts) {
            setError(allowlistState.errorText || "Set at least one allowed WhatsApp contact before linking.");
            setStatus("error");
            return;
        }

        setStatus("connecting");
        setError(null);
        try {
            const amux = getBridge();
            if (!amux?.whatsappConnect) {
                setError("WhatsApp link bridge is not available.");
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
            await getBridge()?.whatsappDisconnect?.();
            setStatus("disconnected");
            setQrDataUrl(null);
            setPhoneInfo(null);
            setError(null);
        } catch (disconnectError: any) {
            setError(disconnectError.message || "Failed to disconnect");
        }
    }

    const statusColors: Record<WhatsAppStatus, string> = {
        disconnected: "var(--text-muted)",
        connecting: "var(--warning)",
        qr_ready: "var(--accent)",
        connected: "var(--success)",
        error: "var(--danger)",
    };

    const statusLabels: Record<WhatsAppStatus, string> = {
        disconnected: "Not connected",
        connecting: "Initializing...",
        qr_ready: "Scan QR code with WhatsApp",
        connected: "Connected",
        error: "Connection error",
    };

    return (
        <div style={{ marginBottom: 12 }}>
            <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between", marginBottom: 10 }}>
                <div style={{ display: "flex", alignItems: "center", gap: 8 }}>
                    <span style={{
                        width: 8, height: 8, borderRadius: "50%",
                        background: statusColors[status],
                        boxShadow: status === "connected" ? `0 0 8px var(--success)` : "none",
                    }} />
                    <span style={{ fontSize: 12, color: statusColors[status], fontWeight: 600 }}>
                        {statusLabels[status]}
                    </span>
                    {phoneInfo && status === "connected" ? (
                        <span style={{ fontSize: 11, color: "var(--text-muted)" }}>({phoneInfo})</span>
                    ) : null}
                </div>
                <div style={{ display: "flex", gap: 6 }}>
                    {(status === "disconnected" || status === "error") ? (
                        <button onClick={connect} disabled={!allowlistState.hasValidContacts} style={{
                            ...smallBtnStyle,
                            color: allowlistState.hasValidContacts ? "var(--success)" : "var(--text-muted)",
                            borderColor: allowlistState.hasValidContacts ? "rgba(166, 227, 161, 0.2)" : "var(--border)",
                            opacity: allowlistState.hasValidContacts ? 1 : 0.6,
                            cursor: allowlistState.hasValidContacts ? "pointer" : "not-allowed",
                        }}>Link Device</button>
                    ) : null}
                    {status === "connected" ? (
                        <button onClick={disconnect} style={{
                            ...smallBtnStyle, color: "var(--danger)", borderColor: "rgba(243, 139, 168, 0.2)",
                        }}>Disconnect</button>
                    ) : null}
                    {status === "qr_ready" ? (
                        <button onClick={disconnect} style={smallBtnStyle}>Cancel</button>
                    ) : null}
                </div>
            </div>

            <div style={{
                marginBottom: 10,
                padding: "6px 10px",
                borderRadius: 0,
                background: allowlistState.hasValidContacts ? "rgba(255,255,255,0.03)" : "rgba(243, 139, 168, 0.08)",
                border: allowlistState.hasValidContacts
                    ? "1px solid rgba(255,255,255,0.06)"
                    : "1px solid rgba(243, 139, 168, 0.2)",
                fontSize: 11,
                color: allowlistState.hasValidContacts ? "var(--text-muted)" : "var(--danger)",
                lineHeight: 1.4,
            }}>
                {allowlistState.hasValidContacts
                    ? `Only messages from allowed contacts will be forwarded after linking. ${allowlistState.helperText}`
                    : `${allowlistState.errorText} ${allowlistState.helperText}`}
            </div>

            {allowlistState.warningText ? (
                <div style={{
                    marginBottom: 10,
                    padding: "6px 10px",
                    borderRadius: 0,
                    background: "rgba(250, 179, 135, 0.08)",
                    border: "1px solid rgba(250, 179, 135, 0.2)",
                    fontSize: 11,
                    color: "var(--warning)",
                    lineHeight: 1.4,
                }}>
                    {allowlistState.warningText}
                </div>
            ) : null}

            {(status === "qr_ready" || status === "connecting") ? (
                <div style={{
                    display: "flex", flexDirection: "column", alignItems: "center",
                    padding: 16, borderRadius: 0,
                    background: "rgba(255,255,255,0.03)", border: "1px solid rgba(255,255,255,0.06)",
                }}>
                    {qrDataUrl ? (
                        <>
                            <img
                                src={qrDataUrl}
                                alt="WhatsApp linking QR code"
                                style={{
                                    display: "block",
                                    width: "100%",
                                    maxWidth: 320,
                                    height: "auto",
                                    padding: 12,
                                    background: "rgba(255,255,255,0.98)",
                                    border: "1px solid rgba(255,255,255,0.08)",
                                    imageRendering: "pixelated",
                                }}
                            />
                            <div style={{ marginTop: 12, fontSize: 11, color: "var(--text-muted)", textAlign: "center", lineHeight: 1.5 }}>
                                Open WhatsApp on your phone → Settings → Linked Devices → Link a Device
                            </div>
                        </>
                    ) : (
                        <div style={{ padding: 24, fontSize: 12, color: "var(--text-muted)" }}>
                            Generating QR code...
                        </div>
                    )}
                </div>
            ) : null}

            {status === "connected" ? (
                <div style={{
                    padding: "8px 10px", borderRadius: 0,
                    background: "rgba(166, 227, 161, 0.05)", border: "1px solid rgba(166, 227, 161, 0.15)",
                    fontSize: 11, color: "var(--text-secondary)", lineHeight: 1.5,
                }}>
                    Session is active. Messages from allowed contacts will be forwarded to amux.
                    The session persists across restarts — no need to re-scan.
                </div>
            ) : null}

            {error ? (
                <div style={{
                    marginTop: 8, padding: "6px 10px", borderRadius: 0,
                    background: "rgba(243, 139, 168, 0.08)", border: "1px solid rgba(243, 139, 168, 0.2)",
                    fontSize: 11, color: "var(--danger)", lineHeight: 1.4,
                }}>
                    {error}
                </div>
            ) : null}
        </div>
    );
}

export function GatewayTab({
    settings, updateSetting,
}: {
    settings: AgentSettings;
    updateSetting: <K extends keyof AgentSettings>(key: K, value: AgentSettings[K]) => void;
}) {
    const whatsappAllowlistState = useMemo(
        () => getWhatsAppAllowlistState(settings.whatsapp_allowed_contacts),
        [settings.whatsapp_allowed_contacts],
    );

    return (
        <>
            <Section title="Connection Status">
                <GatewayHealth />
            </Section>

            <Section title="Gateway">
                <SettingRow label="Enable Gateway">
                    <Toggle value={settings.gateway_enabled}
                        onChange={(value) => updateSetting("gateway_enabled", value)} />
                </SettingRow>
                <SettingRow label="Command Prefix">
                    <TextInput value={settings.gateway_command_prefix}
                        onChange={(value) => updateSetting("gateway_command_prefix", value)}
                        placeholder="!tamux" />
                </SettingRow>
                <div style={{ fontSize: 11, color: "var(--text-muted)", marginTop: 4, marginBottom: 12, lineHeight: 1.5 }}>
                    The gateway bridges chat platforms to tamux. The <code style={{ color: "var(--accent)" }}>tamux-gateway</code> binary
                    reads tokens from environment variables (<code>TAMUX_SLACK_TOKEN</code>, <code>TAMUX_TELEGRAM_TOKEN</code>, etc.)
                    or from the values configured below.
                </div>
            </Section>

            <Section title="Slack">
                <SettingRow label="Bot Token">
                    <PasswordInput value={settings.slack_token}
                        onChange={(value) => updateSetting("slack_token", value)}
                        placeholder="xoxb-..." />
                </SettingRow>
                <SettingRow label="Channel Filter">
                    <TextInput value={settings.slack_channel_filter}
                        onChange={(value) => updateSetting("slack_channel_filter", value)}
                        placeholder="general, ops (comma-separated)" />
                </SettingRow>
            </Section>

            <Section title="Telegram">
                <SettingRow label="Bot Token">
                    <PasswordInput value={settings.telegram_token}
                        onChange={(value) => updateSetting("telegram_token", value)}
                        placeholder="123456:ABC-DEF..." />
                </SettingRow>
                <SettingRow label="Allowed Chats">
                    <TextInput value={settings.telegram_allowed_chats}
                        onChange={(value) => updateSetting("telegram_allowed_chats", value)}
                        placeholder="chat_id1, chat_id2 (comma-separated)" />
                </SettingRow>
            </Section>

            <Section title="Discord">
                <SettingRow label="Bot Token">
                    <PasswordInput value={settings.discord_token}
                        onChange={(value) => updateSetting("discord_token", value)}
                        placeholder="Discord bot token" />
                </SettingRow>
                <SettingRow label="Channel Filter">
                    <TextInput value={settings.discord_channel_filter}
                        onChange={(value) => updateSetting("discord_channel_filter", value)}
                        placeholder="channel_id1, channel_id2 (comma-separated)" />
                </SettingRow>
                <SettingRow label="Allowed Users">
                    <TextInput value={settings.discord_allowed_users}
                        onChange={(value) => updateSetting("discord_allowed_users", value)}
                        placeholder="user_id1, user_id2 (comma-separated)" />
                </SettingRow>
            </Section>

            <Section title="WhatsApp">
                <WhatsAppConnector allowlistState={whatsappAllowlistState} />
                <SettingRow label="Allowed Contacts">
                    <TextAreaInput value={settings.whatsapp_allowed_contacts}
                        onChange={(value) => updateSetting("whatsapp_allowed_contacts", value)}
                        placeholder={"+1234567890, +19876543210\n+447700900123"}
                        rows={4} />
                </SettingRow>
                <div style={{ fontSize: 11, color: "var(--text-muted)", marginTop: 4, marginBottom: 12, lineHeight: 1.5 }}>
                    Enter allowed WhatsApp numbers separated by commas or new lines. {whatsappAllowlistState.hasValidContacts
                        ? `${whatsappAllowlistState.contacts.length} valid contact${whatsappAllowlistState.contacts.length === 1 ? " is" : "s are"} ready for linking.`
                        : "Linking stays disabled until at least one valid contact is configured."}
                </div>

                <div style={{
                    marginTop: 12, padding: "8px 10px", borderRadius: 0,
                    background: "rgba(255,255,255,0.02)", border: "1px solid rgba(255,255,255,0.05)",
                }}>
                    <div style={{ fontSize: 11, color: "var(--text-muted)", marginBottom: 6, fontWeight: 600 }}>
                        Alternative: Business API
                    </div>
                    <div style={{ fontSize: 10, color: "var(--text-muted)", marginBottom: 8, lineHeight: 1.5 }}>
                        If you have a Meta Business account, you can use the Cloud API instead of QR linking.
                    </div>
                    <SettingRow label="API Token">
                        <PasswordInput value={settings.whatsapp_token}
                            onChange={(value) => updateSetting("whatsapp_token", value)}
                            placeholder="WhatsApp Business API token" />
                    </SettingRow>
                    <SettingRow label="Phone Number ID">
                        <TextInput value={settings.whatsapp_phone_id}
                            onChange={(value) => updateSetting("whatsapp_phone_id", value)}
                            placeholder="Phone number ID from Meta dashboard" />
                    </SettingRow>
                </div>
            </Section>
        </>
    );
}
