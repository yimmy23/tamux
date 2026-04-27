import { useMemo } from "react";
import { useAgentStore, type AgentSettings } from "@/lib/agentStore";
import { useSettingsStore } from "@/lib/settingsStore";
import { BUILTIN_THEMES } from "@/lib/themes";
import { ZORAI_APP_NAME } from "@/zorai/branding";

type ToggleSetting = {
  key: keyof AgentSettings;
  label: string;
  description: string;
};

const toolToggles: ToggleSetting[] = [
  { key: "enable_bash_tool", label: "Terminal tool", description: "Allow agents to execute managed shell commands." },
  { key: "enable_vision_tool", label: "Vision tool", description: "Allow screenshot and image inspection workflows." },
  { key: "enable_web_browsing_tool", label: "Browser tool", description: "Allow browser-backed research actions." },
  { key: "enable_web_search_tool", label: "Web search", description: "Allow configured search provider access." },
  { key: "enable_streaming", label: "Streaming", description: "Stream assistant output as it arrives." },
  { key: "enable_conversation_memory", label: "Conversation memory", description: "Keep durable context across agent sessions." },
  { key: "gateway_enabled", label: "Gateway", description: "Bridge Slack, Discord, Telegram, and WhatsApp." },
];

export function SettingsRail() {
  const settings = useSettingsStore((state) => state.settings);
  const agentSettings = useAgentStore((state) => state.agentSettings);

  return (
    <div className="zorai-rail-stack">
      <div className="zorai-rail-card">
        <strong>Provider</strong>
        <span>{agentSettings.active_provider || "not configured"}</span>
      </div>
      <div className="zorai-rail-card">
        <strong>Theme</strong>
        <span>{settings.themeName}</span>
      </div>
      <div className="zorai-rail-card">
        <strong>Runtime</strong>
        <span>{agentSettings.agent_backend}</span>
      </div>
    </div>
  );
}

export function SettingsView() {
  const settings = useSettingsStore((state) => state.settings);
  const updateSetting = useSettingsStore((state) => state.updateSetting);
  const agentSettings = useAgentStore((state) => state.agentSettings);
  const updateAgentSetting = useAgentStore((state) => state.updateAgentSetting);
  const resetAgentSettings = useAgentStore((state) => state.resetAgentSettings);

  const providerIds = useMemo(() => {
    return Object.keys(agentSettings)
      .filter((key) => {
        const value = agentSettings[key];
        return value && typeof value === "object" && "model" in value && "base_url" in value;
      })
      .sort();
  }, [agentSettings]);

  const activeProvider = agentSettings.active_provider;
  const activeProviderConfig = agentSettings[activeProvider] ?? {};

  const updateProviderConfig = (patch: Record<string, unknown>) => {
    updateAgentSetting(activeProvider, { ...activeProviderConfig, ...patch });
  };

  return (
    <section className="zorai-feature-surface zorai-settings-surface">
      <div className="zorai-view-header">
        <div>
          <div className="zorai-kicker">Settings</div>
          <h1>Configure {ZORAI_APP_NAME} without leaving orchestration.</h1>
          <p>Provider, runtime, interface, tools, and gateway controls are grouped for repeated operator use.</p>
        </div>
      </div>

      <div className="zorai-settings-grid">
        <div className="zorai-panel">
          <div>
            <div className="zorai-section-label">Runtime</div>
            <h2>Agent engine</h2>
          </div>
          <SettingRow label="Enable agent runtime" description="Turns the primary Zorai agent runtime on or off.">
            <Switch checked={agentSettings.enabled} onChange={(checked) => updateAgentSetting("enabled", checked)} />
          </SettingRow>
          <SettingRow label="Backend" description="Choose the agent execution backend.">
            <select
              className="zorai-input"
              value={agentSettings.agent_backend}
              onChange={(event) => updateAgentSetting("agent_backend", event.target.value as AgentSettings["agent_backend"])}
            >
              <option value="daemon">{ZORAI_APP_NAME}</option>
              <option value="openclaw">OpenClaw</option>
              <option value="hermes">Hermes</option>
              <option value="legacy">Legacy fallback</option>
            </select>
          </SettingRow>
          <button type="button" className="zorai-ghost-button" onClick={resetAgentSettings}>Reset agent defaults</button>
        </div>

        <div className="zorai-panel">
          <div>
            <div className="zorai-section-label">Model</div>
            <h2>Provider selection</h2>
          </div>
          <SettingRow label="Active provider" description="Provider used by the primary Zorai agent.">
            <select
              className="zorai-input"
              value={activeProvider}
              onChange={(event) => updateAgentSetting("active_provider", event.target.value as AgentSettings["active_provider"])}
            >
              {providerIds.map((providerId) => <option key={providerId} value={providerId}>{providerId}</option>)}
            </select>
          </SettingRow>
          <SettingRow label="Model" description="Default model for this provider.">
            <input
              className="zorai-input"
              value={String(activeProviderConfig.model ?? "")}
              onChange={(event) => updateProviderConfig({ model: event.target.value })}
            />
          </SettingRow>
          <SettingRow label="Base URL" description="Optional OpenAI-compatible endpoint override.">
            <input
              className="zorai-input"
              value={String(activeProviderConfig.base_url ?? "")}
              onChange={(event) => updateProviderConfig({ base_url: event.target.value })}
            />
          </SettingRow>
        </div>

        <div className="zorai-panel">
          <div>
            <div className="zorai-section-label">Interface</div>
            <h2>Shell presentation</h2>
          </div>
          <SettingRow label="Theme" description="Terminal palette used by embedded runtime tools.">
            <select className="zorai-input" value={settings.themeName} onChange={(event) => updateSetting("themeName", event.target.value)}>
              {BUILTIN_THEMES.map((theme) => <option key={theme.name} value={theme.name}>{theme.name}</option>)}
            </select>
          </SettingRow>
          <SettingRow label="Chat font size" description="Message text size for agent conversations.">
            <input
              className="zorai-input"
              type="number"
              min={11}
              max={22}
              value={agentSettings.chatFontSize}
              onChange={(event) => updateAgentSetting("chatFontSize", Number(event.target.value))}
            />
          </SettingRow>
        </div>

        <div className="zorai-panel zorai-settings-tools">
          <div>
            <div className="zorai-section-label">Tools</div>
            <h2>Agent capabilities</h2>
          </div>
          {toolToggles.map((toggle) => (
            <SettingRow key={toggle.key} label={toggle.label} description={toggle.description}>
              <Switch
                checked={Boolean(agentSettings[toggle.key])}
                onChange={(checked) => updateAgentSetting(toggle.key, checked as never)}
              />
            </SettingRow>
          ))}
        </div>
      </div>
    </section>
  );
}

function SettingRow({
  label,
  description,
  children,
}: {
  label: string;
  description: string;
  children: React.ReactNode;
}) {
  return (
    <div className="zorai-setting-row">
      <div>
        <strong>{label}</strong>
        <span>{description}</span>
      </div>
      {children}
    </div>
  );
}

function Switch({ checked, onChange }: { checked: boolean; onChange: (checked: boolean) => void }) {
  return (
    <button
      type="button"
      className={["zorai-switch", checked ? "zorai-switch--on" : ""].filter(Boolean).join(" ")}
      aria-pressed={checked}
      onClick={() => onChange(!checked)}
    >
      <span />
    </button>
  );
}

