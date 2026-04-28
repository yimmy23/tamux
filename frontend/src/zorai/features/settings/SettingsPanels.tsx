import { useEffect, useMemo, useState, type ReactNode } from "react";
import { SubAgentsTab } from "@/components/settings-panel/SubAgentsTab";
import { ModelSelector } from "@/components/settings-panel/shared";
import { useAgentStore, type AgentSettings } from "@/lib/agentStore";
import {
  getDefaultApiTransport,
  getSupportedApiTransports,
  normalizeApiTransport,
} from "@/lib/agentStore/providers";
import type { AgentProviderId, ApiTransportMode, AuthSource } from "@/lib/agentStore/types";
import { getBridge } from "@/lib/bridge";
import { usePluginStore } from "@/lib/pluginStore";
import { useSettingsStore } from "@/lib/settingsStore";
import { BUILTIN_THEMES } from "@/lib/themes";
import { ZORAI_APP_NAME } from "@/zorai/branding";
import type { ZoraiSettingsTabId } from "./settingsTabs";

type ToggleSetting = {
  key: keyof AgentSettings;
  label: string;
  description: string;
};

const authSources: AuthSource[] = ["api_key", "chatgpt_subscription", "github_copilot"];
const reasoningEfforts: AgentSettings["reasoning_effort"][] = ["none", "minimal", "low", "medium", "high", "xhigh"];
const APP_VERSION = "0.7.1";
const APP_AUTHOR = "Mariusz Kurman";
const APP_GITHUB = "mkurman/tamux";
const APP_HOMEPAGE = "tamux.app";

const toolToggles: ToggleSetting[] = [
  { key: "enable_bash_tool", label: "Terminal tool", description: "Allow agents to execute managed shell commands." },
  { key: "enable_vision_tool", label: "Vision tool", description: "Allow screenshot and image inspection workflows." },
  { key: "enable_web_browsing_tool", label: "Browser tool", description: "Allow browser-backed research actions." },
  { key: "enable_web_search_tool", label: "Web search", description: "Allow configured search provider access." },
  { key: "enable_streaming", label: "Streaming", description: "Stream assistant output as it arrives." },
  { key: "enable_conversation_memory", label: "Conversation memory", description: "Keep durable context across agent sessions." },
  { key: "auto_retry", label: "Auto retry", description: "Retry recoverable provider and tool failures." },
];

export function SettingsTabPanel({ activeTab }: { activeTab: ZoraiSettingsTabId }) {
  if (activeTab === "model") return <ModelPanel />;
  if (activeTab === "auth") return <AuthPanel />;
  if (activeTab === "interface") return <InterfacePanel />;
  if (activeTab === "tools") return <ToolsPanel />;
  if (activeTab === "search") return <SearchPanel />;
  if (activeTab === "concierge") return <ConciergePanel />;
  if (activeTab === "subagents") return <SubAgentsPanel />;
  if (activeTab === "gateway") return <GatewayPanel />;
  if (activeTab === "features") return <FeaturesPanel />;
  if (activeTab === "advanced") return <AdvancedPanel />;
  if (activeTab === "plugins") return <PluginsPanel />;
  if (activeTab === "about") return <AboutPanel />;
  return <ChatPanel />;
}

function ChatPanel() {
  const agentSettings = useAgentStore((state) => state.agentSettings);
  const updateAgentSetting = useAgentStore((state) => state.updateAgentSetting);

  return (
    <SettingsGrid>
      <Panel section="Chat" title="Streaming and memory">
        <SettingRow label="Streaming" description="Stream assistant output as it arrives."><Switch checked={agentSettings.enable_streaming} onChange={(checked) => updateAgentSetting("enable_streaming", checked)} /></SettingRow>
        <SettingRow label="Conversation Memory" description="Keep durable context across agent sessions."><Switch checked={agentSettings.enable_conversation_memory} onChange={(checked) => updateAgentSetting("enable_conversation_memory", checked)} /></SettingRow>
        <SettingRow label="Honcho Memory" description="Enable Honcho-backed memory when configured."><Switch checked={agentSettings.enable_honcho_memory} onChange={(checked) => updateAgentSetting("enable_honcho_memory", checked)} /></SettingRow>
        <SettingRow label="Visible Msgs" description="Messages visible in the terminal chat history page.">
          <input className="zorai-input" type="number" min={25} max={1000} value={agentSettings.tui_chat_history_page_size} onChange={(event) => updateAgentSetting("tui_chat_history_page_size", Number(event.target.value))} />
        </SettingRow>
      </Panel>
      <Panel section="Chat" title="Operational intelligence">
        <SettingRow label="Anticipatory Support" description="Enable proactive operator support."><Switch checked={agentSettings.anticipatory_enabled} onChange={(checked) => updateAgentSetting("anticipatory_enabled", checked)} /></SettingRow>
        <SettingRow label="Morning Brief" description="Include morning brief generation."><Switch checked={agentSettings.anticipatory_morning_brief} onChange={(checked) => updateAgentSetting("anticipatory_morning_brief", checked)} /></SettingRow>
        <SettingRow label="Predictive Hydration" description="Hydrate likely-needed context ahead of time."><Switch checked={agentSettings.anticipatory_predictive_hydration} onChange={(checked) => updateAgentSetting("anticipatory_predictive_hydration", checked)} /></SettingRow>
        <SettingRow label="Stuck Detection" description="Detect stalled work and stale execution."><Switch checked={agentSettings.anticipatory_stuck_detection} onChange={(checked) => updateAgentSetting("anticipatory_stuck_detection", checked)} /></SettingRow>
        <SettingRow label="Operator Model" description="Learn operator preferences from local activity."><Switch checked={agentSettings.operator_model_enabled} onChange={(checked) => updateAgentSetting("operator_model_enabled", checked)} /></SettingRow>
        <SettingRow label="Collaboration" description="Enable multi-agent collaboration features."><Switch checked={agentSettings.collaboration_enabled} onChange={(checked) => updateAgentSetting("collaboration_enabled", checked)} /></SettingRow>
      </Panel>
      <Panel section="Chat" title="Compliance and synthesis">
        <SettingRow label="Compliance" description="Audit mode for retained events.">
          <select className="zorai-input" value={agentSettings.compliance_mode} onChange={(event) => updateAgentSetting("compliance_mode", event.target.value as AgentSettings["compliance_mode"])}>
            {["standard", "soc2", "hipaa", "fedramp"].map((value) => <option key={value} value={value}>{value}</option>)}
          </select>
        </SettingRow>
        <NumberRow label="Retention Days" description="Compliance event retention window." value={agentSettings.compliance_retention_days} onChange={(value) => updateAgentSetting("compliance_retention_days", value)} min={1} max={3650} />
        <SettingRow label="Sign All Events" description="Sign compliance audit events."><Switch checked={agentSettings.compliance_sign_all_events} onChange={(checked) => updateAgentSetting("compliance_sign_all_events", checked)} /></SettingRow>
        <SettingRow label="Tool Synthesis" description="Allow generated tool workflows."><Switch checked={agentSettings.tool_synthesis_enabled} onChange={(checked) => updateAgentSetting("tool_synthesis_enabled", checked)} /></SettingRow>
        <SettingRow label="Require Activation" description="Require generated tools to be activated before use."><Switch checked={agentSettings.tool_synthesis_require_activation} onChange={(checked) => updateAgentSetting("tool_synthesis_require_activation", checked)} /></SettingRow>
        <NumberRow label="Tool Limit" description="Maximum generated tools." value={agentSettings.tool_synthesis_max_generated_tools} onChange={(value) => updateAgentSetting("tool_synthesis_max_generated_tools", value)} min={0} max={100} />
      </Panel>
    </SettingsGrid>
  );
}

function ModelPanel() {
  const agentSettings = useAgentStore((state) => state.agentSettings);
  const updateAgentSetting = useAgentStore((state) => state.updateAgentSetting);
  const providerAuthStates = useAgentStore((state) => state.providerAuthStates);
  const providerIds = useProviderIds(agentSettings);
  const activeProvider = agentSettings.active_provider;
  const activeProviderConfig = agentSettings[activeProvider] ?? {};
  const updateProviderConfig = (patch: Record<string, unknown>) => updateAgentSetting(activeProvider, { ...activeProviderConfig, ...patch });
  const supportedTransports = getSupportedApiTransports(activeProvider);
  const transportValue = normalizeApiTransport(activeProvider, activeProviderConfig.api_transport);
  const providerAuthenticated = Boolean(providerAuthStates.find((entry) => entry.provider_id === activeProvider)?.authenticated);

  return (
    <SettingsGrid>
      <Panel section="Svarog" title="Svarog Provider">
        <SettingRow label="Provider" description="Provider used by the primary agent.">
          <select className="zorai-input" value={activeProvider} onChange={(event) => updateAgentSetting("active_provider", event.target.value as AgentSettings["active_provider"])}>
            {providerIds.map((providerId) => <option key={providerId} value={providerId}>{providerId}</option>)}
          </select>
        </SettingRow>
        <SettingRow label="Model" description="Default model for this provider.">
          <ModelSelector
            providerId={activeProvider}
            value={String(activeProviderConfig.model ?? "")}
            customName={String(activeProviderConfig.custom_model_name ?? "")}
            onChange={(value, customModelName, details) => updateProviderConfig({
              model: value,
              custom_model_name: customModelName && customModelName !== value ? customModelName : "",
              context_window_tokens: details?.predefinedModel?.contextWindow ?? details?.fetchedModel?.contextWindow ?? activeProviderConfig.context_window_tokens,
            })}
            base_url={String(activeProviderConfig.base_url ?? "")}
            api_key={String(activeProviderConfig.api_key ?? "")}
            auth_source={activeProviderConfig.auth_source}
            allowProviderAuthFetch={providerAuthenticated}
          />
        </SettingRow>
        <SettingRow label="Transport" description="Provider API transport mode.">
          <select className="zorai-input" value={transportValue} onChange={(event) => updateProviderConfig({ api_transport: normalizeApiTransport(activeProvider, event.target.value) })}>
            {supportedTransports.map((transport) => <option key={transport} value={transport}>{formatTransportLabel(transport)}</option>)}
          </select>
        </SettingRow>
        <SettingRow label="Base URL" description="Optional OpenAI-compatible endpoint override.">
          <input className="zorai-input" value={String(activeProviderConfig.base_url ?? "")} onChange={(event) => updateProviderConfig({ base_url: event.target.value })} />
        </SettingRow>
        <SettingRow label="Assistant ID" description="Optional native assistant identifier.">
          <input className="zorai-input" value={String(activeProviderConfig.assistant_id ?? "")} onChange={(event) => updateProviderConfig({ assistant_id: event.target.value })} />
        </SettingRow>
        <SettingRow label="Effort" description="Svarog reasoning effort.">
          <select className="zorai-input" value={agentSettings.reasoning_effort} onChange={(event) => updateAgentSetting("reasoning_effort", event.target.value as AgentSettings["reasoning_effort"])}>
            {reasoningEfforts.map((value) => <option key={value} value={value}>{value}</option>)}
          </select>
        </SettingRow>
        <NumberRow label="Ctx Length" description="Context length override in tokens." value={Number(activeProviderConfig.context_window_tokens ?? 0)} onChange={(value) => updateProviderConfig({ context_window_tokens: value || null })} min={0} max={2000000} />
      </Panel>
      <Panel section="Svarog" title="Main agent identity and behavior">
        <Metric label="Fixed Name" value="Svarog" />
        <SettingRow label="System Prompt" description="Primary agent identity prompt.">
          <textarea className="zorai-input" value={agentSettings.system_prompt} onChange={(event) => updateAgentSetting("system_prompt", event.target.value)} />
        </SettingRow>
        <Metric label="Backend" value="daemon" />
      </Panel>
    </SettingsGrid>
  );
}

function AuthPanel() {
  const authStates = useAgentStore((state) => state.providerAuthStates);
  const refreshAuth = useAgentStore((state) => state.refreshProviderAuthStates);
  const validateProvider = useAgentStore((state) => state.validateProvider);
  const loginProvider = useAgentStore((state) => state.loginProvider);
  const logoutProvider = useAgentStore((state) => state.logoutProvider);
  const [loginTarget, setLoginTarget] = useState<string | null>(null);
  const [loginKey, setLoginKey] = useState("");
  const [validationResult, setValidationResult] = useState<Record<string, string>>({});

  useEffect(() => {
    void refreshAuth();
  }, [refreshAuth]);

  const runTest = async (providerId: string, baseUrl: string, authSource: string) => {
    const result = await validateProvider(providerId, baseUrl, "", authSource);
    setValidationResult((items) => ({ ...items, [providerId]: result.valid ? "ok" : result.error ?? "failed" }));
  };

  return (
    <SettingsGrid extraClassName="zorai-settings-grid--full">
      <Panel section="Auth" title="Authentication" extraClassName="zorai-settings-auth">
        <button type="button" className="zorai-ghost-button" onClick={() => void refreshAuth()}>Refresh auth status</button>
        {authStates.length === 0 ? <p className="zorai-empty-state">No provider auth status has been reported by the daemon yet.</p> : authStates.map((state) => (
          <div key={`${state.provider_id}-${state.auth_source}`} className="zorai-setting-row">
            <div><strong>{state.authenticated ? "●" : "○"} {state.provider_name}</strong><span>{state.model ? `${state.model} / ` : ""}{state.auth_source}</span>{validationResult[state.provider_id] ? <span>{validationResult[state.provider_id]}</span> : null}</div>
            <div className="zorai-card-actions">
              <button type="button" className="zorai-ghost-button" onClick={() => {
                setLoginTarget(loginTarget === state.provider_id ? null : state.provider_id);
                setLoginKey("");
              }}>API Key</button>
              {state.authenticated ? <button type="button" className="zorai-ghost-button" onClick={() => void logoutProvider(state.provider_id)}>Logout</button> : null}
              <button type="button" className="zorai-ghost-button" onClick={() => void runTest(state.provider_id, state.base_url, state.auth_source)}>Test</button>
            </div>
            {loginTarget === state.provider_id ? (
              <div className="zorai-setting-row">
                <div><strong>API Key</strong><span>Stored by the daemon provider auth store.</span></div>
                <input className="zorai-input" type="password" value={loginKey} onChange={(event) => setLoginKey(event.target.value)} />
                <button type="button" className="zorai-primary-button" disabled={!loginKey.trim()} onClick={() => void loginProvider(state.provider_id, loginKey, state.base_url).then(() => { setLoginKey(""); setLoginTarget(null); })}>Save</button>
              </div>
            ) : null}
          </div>
        ))}
      </Panel>
    </SettingsGrid>
  );
}

function InterfacePanel() {
  const settings = useSettingsStore((state) => state.settings);
  const updateSetting = useSettingsStore((state) => state.updateSetting);

  return (
    <SettingsGrid>
      <Panel section="Terminal interface" title="Shell presentation">
        <SettingRow label="Theme" description="Terminal palette used by embedded runtime tools.">
          <select className="zorai-input" value={settings.themeName} onChange={(event) => updateSetting("themeName", event.target.value)}>
            {BUILTIN_THEMES.map((theme) => <option key={theme.name} value={theme.name}>{theme.name}</option>)}
          </select>
        </SettingRow>
        <Metric label="Terminal focus" value="tab:focus" />
        <Metric label="Threads" value="ctrl+t" />
        <Metric label="Goals" value="ctrl+g" />
      </Panel>
    </SettingsGrid>
  );
}

function SearchPanel() {
  const agentSettings = useAgentStore((state) => state.agentSettings);
  const updateAgentSetting = useAgentStore((state) => state.updateAgentSetting);

  return (
    <SettingsGrid>
      <Panel section="Search" title="Web Search">
        <SettingRow label="Enable Web Search" description="Mirrors the web search tool toggle."><Switch checked={agentSettings.enable_web_search_tool} onChange={(checked) => updateAgentSetting("enable_web_search_tool", checked)} /></SettingRow>
        <SettingRow label="Provider" description="Search provider used by agent web search.">
          <select className="zorai-input" value={agentSettings.search_provider} onChange={(event) => updateAgentSetting("search_provider", event.target.value as AgentSettings["search_provider"])}>
            {["none", "firecrawl", "exa", "tavily"].map((value) => <option key={value} value={value}>{value}</option>)}
          </select>
        </SettingRow>
        <SecretRow label="Firecrawl Key" value={agentSettings.firecrawl_api_key} onChange={(value) => updateAgentSetting("firecrawl_api_key", value)} />
        <SecretRow label="Exa Key" value={agentSettings.exa_api_key} onChange={(value) => updateAgentSetting("exa_api_key", value)} />
        <SecretRow label="Tavily Key" value={agentSettings.tavily_api_key} onChange={(value) => updateAgentSetting("tavily_api_key", value)} />
        <NumberRow label="Max Results" description="Maximum search results." value={agentSettings.search_max_results} onChange={(value) => updateAgentSetting("search_max_results", value)} min={1} max={50} />
        <NumberRow label="Timeout" description="Search timeout in seconds." value={agentSettings.search_timeout_secs} onChange={(value) => updateAgentSetting("search_timeout_secs", value)} min={1} max={120} />
        <SettingRow label="Browser" description="Browsing provider.">
          <select className="zorai-input" value={agentSettings.browse_provider} onChange={(event) => updateAgentSetting("browse_provider", event.target.value as AgentSettings["browse_provider"])}>
            {["auto", "lightpanda", "chrome", "none"].map((value) => <option key={value} value={value}>{value}</option>)}
          </select>
        </SettingRow>
      </Panel>
    </SettingsGrid>
  );
}

function ToolsPanel() {
  const agentSettings = useAgentStore((state) => state.agentSettings);
  const updateAgentSetting = useAgentStore((state) => state.updateAgentSetting);

  return (
    <SettingsGrid>
      <Panel section="Tools" title="Agent capabilities" extraClassName="zorai-settings-tools">
        {toolToggles.map((toggle) => (
          <SettingRow key={toggle.key} label={toggle.label} description={toggle.description}>
            <Switch checked={Boolean(agentSettings[toggle.key])} onChange={(checked) => updateAgentSetting(toggle.key, checked as never)} />
          </SettingRow>
        ))}
      </Panel>
    </SettingsGrid>
  );
}

function ConciergePanel() {
  const agentSettings = useAgentStore((state) => state.agentSettings);
  const config = useAgentStore((state) => state.conciergeConfig);
  const updateConfig = useAgentStore((state) => state.updateConciergeConfig);
  const providerAuthStates = useAgentStore((state) => state.providerAuthStates);
  const refreshProviderAuthStates = useAgentStore((state) => state.refreshProviderAuthStates);
  const providerIds = useProviderIds(agentSettings);
  const selectedConciergeProvider = config.provider ?? "";
  const providerNames = new Map(providerAuthStates.map((state) => [state.provider_id, state.provider_name]));
  const selectableProviders = Array.from(new Set([
    ...providerIds,
    ...providerAuthStates.map((state) => state.provider_id),
    ...(selectedConciergeProvider ? [selectedConciergeProvider] : []),
  ])).filter(Boolean).sort().map((providerId) => ({
    provider_id: providerId,
    provider_name: providerNames.get(providerId) ?? providerId,
  }));
  const detailLevels = Array.from(new Set([config.detail_level, "proactive_triage", "brief", "standard", "detailed"].filter(Boolean)));
  const patchConfig = (patch: Record<string, unknown>) => void updateConfig({ ...config, ...patch });

  useEffect(() => {
    void refreshProviderAuthStates();
  }, [refreshProviderAuthStates]);

  return (
    <SettingsGrid>
      <Panel section="Rarog" title="Welcome agent and operational assistant">
        <SettingRow label="Enabled" description="Allow the concierge to brief and guide operator sessions.">
          <Switch checked={config.enabled} onChange={(checked) => patchConfig({ enabled: checked })} />
        </SettingRow>
        <SettingRow label="Detail level" description="Default depth for concierge guidance.">
          <select className="zorai-input" value={config.detail_level} onChange={(event) => patchConfig({ detail_level: event.target.value })}>
            {detailLevels.map((value) => <option key={value} value={value}>{value}</option>)}
          </select>
        </SettingRow>
        <SettingRow label="Provider" description="Rarog provider, or inherit from Svarog.">
          <select className="zorai-input" value={selectedConciergeProvider} onChange={(event) => patchConfig({ provider: event.target.value || undefined, model: event.target.value ? config.model : undefined })}>
            <option value="">(use Svarog)</option>
            {selectableProviders.map((provider) => <option key={provider.provider_id} value={provider.provider_id}>{provider.provider_name}</option>)}
          </select>
        </SettingRow>
        <SettingRow label="Model" description="Rarog model, or inherit from Svarog.">
          {selectedConciergeProvider ? (
            <ModelSelector providerId={selectedConciergeProvider as AgentProviderId} value={config.model ?? ""} customName={config.model ?? ""} onChange={(model) => patchConfig({ model: model || undefined })} allowProviderAuthFetch={Boolean(providerAuthStates.find((entry) => entry.provider_id === selectedConciergeProvider)?.authenticated)} />
          ) : <span className="zorai-empty-state">(use Svarog)</span>}
        </SettingRow>
        <SettingRow label="Reasoning" description="Rarog reasoning effort.">
          <select className="zorai-input" value={config.reasoning_effort ?? ""} onChange={(event) => patchConfig({ reasoning_effort: event.target.value || undefined })}>
            {["", "none", "minimal", "low", "medium", "high", "xhigh"].map((value) => <option key={value || "inherit"} value={value}>{value || "(use Svarog)"}</option>)}
          </select>
        </SettingRow>
      </Panel>
    </SettingsGrid>
  );
}

function SubAgentsPanel() {
  return <SubAgentsTab />;
}

function GatewayPanel() {
  const agentSettings = useAgentStore((state) => state.agentSettings);
  const updateAgentSetting = useAgentStore((state) => state.updateAgentSetting);
  const [whatsappStatus, setWhatsappStatus] = useState("");
  const connectWhatsApp = async () => {
    const result = await getBridge()?.whatsappConnect?.();
    setWhatsappStatus(result?.ok === false ? result.error ?? "WhatsApp link failed." : "WhatsApp link requested.");
  };

  return (
    <SettingsGrid>
      <Panel section="Gateway" title="Messaging platform connections">
        <SettingRow label="Gateway enabled" description="Bridge external chat platforms into Zorai.">
          <Switch checked={agentSettings.gateway_enabled} onChange={(checked) => updateAgentSetting("gateway_enabled", checked)} />
        </SettingRow>
        <SettingRow label="Command Prefix" description="Prefix used for external platform commands.">
          <input className="zorai-input" value={agentSettings.gateway_command_prefix} onChange={(event) => updateAgentSetting("gateway_command_prefix", event.target.value)} />
        </SettingRow>
      </Panel>
      <Panel section="Gateway" title="Slack">
        <SecretRow label="Bot Token" value={agentSettings.slack_token} onChange={(value) => updateAgentSetting("slack_token", value)} />
        <SettingRow label="Channel Filter" description="Allowed Slack channels."><input className="zorai-input" value={agentSettings.slack_channel_filter} onChange={(event) => updateAgentSetting("slack_channel_filter", event.target.value)} /></SettingRow>
      </Panel>
      <Panel section="Gateway" title="Telegram">
        <SecretRow label="Bot Token" value={agentSettings.telegram_token} onChange={(value) => updateAgentSetting("telegram_token", value)} />
        <SettingRow label="Allowed Chats" description="Comma-separated Telegram chat ids."><input className="zorai-input" value={agentSettings.telegram_allowed_chats} onChange={(event) => updateAgentSetting("telegram_allowed_chats", event.target.value)} /></SettingRow>
      </Panel>
      <Panel section="Gateway" title="Discord">
        <SecretRow label="Bot Token" value={agentSettings.discord_token} onChange={(value) => updateAgentSetting("discord_token", value)} />
        <SettingRow label="Channel Filter" description="Allowed Discord channels."><input className="zorai-input" value={agentSettings.discord_channel_filter} onChange={(event) => updateAgentSetting("discord_channel_filter", event.target.value)} /></SettingRow>
        <SettingRow label="Allowed Users" description="Allowed Discord users."><input className="zorai-input" value={agentSettings.discord_allowed_users} onChange={(event) => updateAgentSetting("discord_allowed_users", event.target.value)} /></SettingRow>
      </Panel>
      <Panel section="Gateway" title="WhatsApp">
        <SettingRow label="Allowed Contacts" description="Comma or newline separated phone numbers."><textarea className="zorai-input" value={agentSettings.whatsapp_allowed_contacts} onChange={(event) => updateAgentSetting("whatsapp_allowed_contacts", event.target.value)} /></SettingRow>
        <SecretRow label="API Token" value={agentSettings.whatsapp_token} onChange={(value) => updateAgentSetting("whatsapp_token", value)} />
        <SettingRow label="Phone Number ID" description="WhatsApp Cloud API phone number id."><input className="zorai-input" value={agentSettings.whatsapp_phone_id} onChange={(event) => updateAgentSetting("whatsapp_phone_id", event.target.value)} /></SettingRow>
        <button type="button" className="zorai-ghost-button" onClick={() => void connectWhatsApp()}>Link Device</button>
        <button type="button" className="zorai-ghost-button" onClick={() => void connectWhatsApp()}>Re-link Device</button>
        {whatsappStatus ? <p className="zorai-empty-state">{whatsappStatus}</p> : null}
      </Panel>
    </SettingsGrid>
  );
}

function FeaturesPanel() {
  const agentSettings = useAgentStore((state) => state.agentSettings);
  const updateAgentSetting = useAgentStore((state) => state.updateAgentSetting);

  return (
    <SettingsGrid>
      <Panel section="Features" title="Audio and media">
        <SettingRow label="STT Enabled" description="Speech-to-text feature toggle."><Switch checked={agentSettings.audio_stt_enabled} onChange={(checked) => updateAgentSetting("audio_stt_enabled", checked)} /></SettingRow>
        <SettingRow label="STT Provider" description="Speech-to-text provider."><input className="zorai-input" value={agentSettings.audio_stt_provider} onChange={(event) => updateAgentSetting("audio_stt_provider", event.target.value as never)} /></SettingRow>
        <SettingRow label="STT Model" description="Speech-to-text model."><input className="zorai-input" value={agentSettings.audio_stt_model} onChange={(event) => updateAgentSetting("audio_stt_model", event.target.value)} /></SettingRow>
        <SettingRow label="TTS Enabled" description="Text-to-speech feature toggle."><Switch checked={agentSettings.audio_tts_enabled} onChange={(checked) => updateAgentSetting("audio_tts_enabled", checked)} /></SettingRow>
        <SettingRow label="TTS Provider" description="Text-to-speech provider."><input className="zorai-input" value={agentSettings.audio_tts_provider} onChange={(event) => updateAgentSetting("audio_tts_provider", event.target.value as never)} /></SettingRow>
        <SettingRow label="TTS Model" description="Text-to-speech model."><input className="zorai-input" value={agentSettings.audio_tts_model} onChange={(event) => updateAgentSetting("audio_tts_model", event.target.value)} /></SettingRow>
        <SettingRow label="TTS Voice" description="Text-to-speech voice."><input className="zorai-input" value={agentSettings.audio_tts_voice} onChange={(event) => updateAgentSetting("audio_tts_voice", event.target.value)} /></SettingRow>
        <SettingRow label="Image Provider" description="Image generation provider."><input className="zorai-input" value={agentSettings.image_generation_provider} onChange={(event) => updateAgentSetting("image_generation_provider", event.target.value as never)} /></SettingRow>
        <SettingRow label="Image Model" description="Image generation model."><input className="zorai-input" value={agentSettings.image_generation_model} onChange={(event) => updateAgentSetting("image_generation_model", event.target.value)} /></SettingRow>
      </Panel>
    </SettingsGrid>
  );
}

function AdvancedPanel() {
  const agentSettings = useAgentStore((state) => state.agentSettings);
  const updateAgentSetting = useAgentStore((state) => state.updateAgentSetting);
  const compaction = agentSettings.compaction;
  const customCompactionProvider = compaction.custom_model.provider;
  const customCompactionTransports = getSupportedApiTransports(customCompactionProvider);
  const customCompactionTransport = normalizeApiTransport(customCompactionProvider, compaction.custom_model.api_transport);
  const updateLooseAgentSetting = (key: keyof AgentSettings, value: unknown) => updateAgentSetting(key, value as never);
  const updateCompaction = (patch: Partial<AgentSettings["compaction"]>) => updateAgentSetting("compaction", { ...compaction, ...patch });
  const updateCompactionWeles = (patch: Partial<AgentSettings["compaction"]["weles"]>) => updateCompaction({ weles: { ...compaction.weles, ...patch } });
  const updateCompactionCustom = (patch: Partial<AgentSettings["compaction"]["custom_model"]>) => updateCompaction({ custom_model: { ...compaction.custom_model, ...patch } });

  return (
    <SettingsGrid>
      <Panel section="Advanced" title="Context compaction, safety, and retry settings">
        <SettingRow label="Sandbox Managed Cmds" description="Use daemon-managed command execution defaults."><Switch checked={Boolean(agentSettings.managed_sandbox_enabled)} onChange={(checked) => updateLooseAgentSetting("managed_sandbox_enabled", checked)} /></SettingRow>
        <SettingRow label="Managed Security" description="Default approval strictness for managed shell commands.">
          <select className="zorai-input" value={String(agentSettings.managed_security_level ?? "lowest")} onChange={(event) => updateLooseAgentSetting("managed_security_level", event.target.value)}>
            {["highest", "moderate", "lowest", "yolo"].map((value) => <option key={value} value={value}>{value}</option>)}
          </select>
        </SettingRow>
        <SettingRow label="Auto Compact Context" description="Compress older conversation context automatically."><Switch checked={agentSettings.auto_compact_context} onChange={(checked) => updateAgentSetting("auto_compact_context", checked)} /></SettingRow>
        <SettingRow label="Compaction Mode" description="Strategy used when active context needs compaction.">
          <select className="zorai-input" value={compaction.strategy} onChange={(event) => updateCompaction({ strategy: event.target.value as AgentSettings["compaction"]["strategy"] })}>
            <option value="heuristic">heuristic</option>
            <option value="weles">weles</option>
            <option value="custom_model">custom model</option>
          </select>
        </SettingRow>
        <NumberRow label="Heuristic Max Msgs" description="Conversation messages kept before compaction." value={agentSettings.max_context_messages} onChange={(value) => updateAgentSetting("max_context_messages", value)} min={10} max={500} />
        <NumberRow label="Max Tool Loops" description="Upper bound for tool-call cycles in one response." value={agentSettings.max_tool_loops} onChange={(value) => updateAgentSetting("max_tool_loops", value)} min={0} max={50} />
        <NumberRow label="Max Retries" description="Provider and tool retry attempts." value={agentSettings.max_retries} onChange={(value) => updateAgentSetting("max_retries", value)} min={0} max={10} />
        <NumberRow label="Retry Delay (ms)" description="Delay between retries." value={agentSettings.retry_delay_ms} onChange={(value) => updateAgentSetting("retry_delay_ms", value)} min={0} max={60000} />
        <NumberRow label="Message Loop (ms)" description="Delay between message loop iterations." value={agentSettings.message_loop_delay_ms} onChange={(value) => updateAgentSetting("message_loop_delay_ms", value)} min={0} max={60000} />
        <NumberRow label="Tool Call Gap (ms)" description="Delay between tool calls." value={agentSettings.tool_call_delay_ms} onChange={(value) => updateAgentSetting("tool_call_delay_ms", value)} min={0} max={60000} />
        <NumberRow label="LLM Stream Timeout (s)" description="Timeout while waiting for streamed model chunks." value={agentSettings.llm_stream_chunk_timeout_secs} onChange={(value) => updateAgentSetting("llm_stream_chunk_timeout_secs", value)} min={1} max={3600} />
        <SettingRow label="Auto Retry" description="Retry recoverable provider and tool failures."><Switch checked={agentSettings.auto_retry} onChange={(checked) => updateAgentSetting("auto_retry", checked)} /></SettingRow>
        <NumberRow label="Context Len Tok" description="Fallback context length when the provider model has no known value." value={agentSettings.context_window_tokens} onChange={(value) => updateAgentSetting("context_window_tokens", value)} min={1000} max={2000000} />
        <NumberRow label="Compact Thres %" description="Token-budget threshold that triggers compaction." value={agentSettings.compact_threshold_pct} onChange={(value) => updateAgentSetting("compact_threshold_pct", value)} min={1} max={100} />
        <NumberRow label="Keep Recent" description="Recent messages preserved across compaction." value={agentSettings.keep_recent_on_compact} onChange={(value) => updateAgentSetting("keep_recent_on_compact", value)} min={0} max={100} />
        <NumberRow label="Bash Timeout (s)" description="Managed shell command timeout." value={agentSettings.bash_timeout_seconds} onChange={(value) => updateAgentSetting("bash_timeout_seconds", value)} min={5} max={300} />
        <NumberRow label="WELES Reviews" description="Concurrent WELES review limit." value={agentSettings.weles_max_concurrent_reviews} onChange={(value) => updateAgentSetting("weles_max_concurrent_reviews", value)} min={0} max={16} />
      </Panel>
      <Panel section="Advanced" title="Compaction Strategy Settings">
        {compaction.strategy === "weles" ? (
          <>
            <SettingRow label="WELES Provider" description="Provider used by WELES compaction."><input className="zorai-input" value={compaction.weles.provider} onChange={(event) => updateCompactionWeles({ provider: event.target.value as AgentProviderId })} /></SettingRow>
            <SettingRow label="WELES Model" description="Model used by WELES compaction."><input className="zorai-input" value={compaction.weles.model} onChange={(event) => updateCompactionWeles({ model: event.target.value })} /></SettingRow>
            <SettingRow label="WELES Reasoning" description="Reasoning effort for WELES compaction.">
              <select className="zorai-input" value={compaction.weles.reasoning_effort} onChange={(event) => updateCompactionWeles({ reasoning_effort: event.target.value as AgentSettings["reasoning_effort"] })}>
                {reasoningEfforts.map((value) => <option key={value} value={value}>{value}</option>)}
              </select>
            </SettingRow>
          </>
        ) : null}
        {compaction.strategy === "custom_model" ? (
          <>
            <SettingRow label="Custom Provider" description="Provider used by custom-model compaction."><input className="zorai-input" value={customCompactionProvider} onChange={(event) => updateCompactionCustom({ provider: event.target.value as AgentProviderId, api_transport: getDefaultApiTransport(event.target.value as AgentProviderId) })} /></SettingRow>
            <SettingRow label="Custom Base URL" description="Endpoint used by custom-model compaction."><input className="zorai-input" value={compaction.custom_model.base_url} onChange={(event) => updateCompactionCustom({ base_url: event.target.value })} /></SettingRow>
            <SettingRow label="Custom Auth" description="Credential source for the custom compaction model.">
              <select className="zorai-input" value={compaction.custom_model.auth_source} onChange={(event) => updateCompactionCustom({ auth_source: event.target.value as AuthSource })}>
                {authSources.map((source) => <option key={source} value={source}>{source}</option>)}
              </select>
            </SettingRow>
            <SettingRow label="Custom Model" description="Model used by custom-model compaction."><input className="zorai-input" value={compaction.custom_model.model} onChange={(event) => updateCompactionCustom({ model: event.target.value })} /></SettingRow>
            <SecretRow label="Custom API Key" value={compaction.custom_model.api_key} onChange={(value) => updateCompactionCustom({ api_key: value })} />
            <SettingRow label="Assistant ID" description="Optional native assistant identifier."><input className="zorai-input" value={compaction.custom_model.assistant_id} onChange={(event) => updateCompactionCustom({ assistant_id: event.target.value })} /></SettingRow>
            <SettingRow label="Custom Transport" description="Transport for custom-model compaction.">
              <select className="zorai-input" value={customCompactionTransport} onChange={(event) => updateCompactionCustom({ api_transport: normalizeApiTransport(customCompactionProvider, event.target.value) })}>
                {customCompactionTransports.map((transport) => <option key={transport} value={transport}>{formatTransportLabel(transport)}</option>)}
              </select>
            </SettingRow>
            <SettingRow label="Custom Reasoning" description="Reasoning effort for custom-model compaction.">
              <select className="zorai-input" value={compaction.custom_model.reasoning_effort} onChange={(event) => updateCompactionCustom({ reasoning_effort: event.target.value as AgentSettings["reasoning_effort"] })}>
                {reasoningEfforts.map((value) => <option key={value} value={value}>{value}</option>)}
              </select>
            </SettingRow>
            <NumberRow label="Custom Ctx Tok" description="Context window for custom-model compaction." value={compaction.custom_model.context_window_tokens} onChange={(value) => updateCompactionCustom({ context_window_tokens: value })} min={1000} max={2000000} />
          </>
        ) : null}
        {compaction.strategy === "heuristic" ? <p className="zorai-empty-state">Heuristic compaction uses the message limit and token threshold above.</p> : null}
      </Panel>
    </SettingsGrid>
  );
}

function PluginsPanel() {
  const plugins = usePluginStore((state) => state.plugins);
  const loading = usePluginStore((state) => state.loading);
  const error = usePluginStore((state) => state.error);
  const fetchPlugins = usePluginStore((state) => state.fetchPlugins);
  const toggleEnabled = usePluginStore((state) => state.toggleEnabled);
  const selectedPlugin = usePluginStore((state) => state.selectedPlugin);
  const settingsSchema = usePluginStore((state) => state.settingsSchema);
  const settingsValues = usePluginStore((state) => state.settingsValues);
  const selectPlugin = usePluginStore((state) => state.selectPlugin);
  const updateSetting = usePluginStore((state) => state.updateSetting);
  const installPlugin = usePluginStore((state) => state.installPlugin);
  const uninstallPlugin = usePluginStore((state) => state.uninstallPlugin);
  const testConnection = usePluginStore((state) => state.testConnection);
  const startOAuth = usePluginStore((state) => state.startOAuth);
  const testResult = usePluginStore((state) => state.testResult);
  const actionMessage = usePluginStore((state) => state.actionMessage);
  const [newPluginDir, setNewPluginDir] = useState("");
  const [newPluginSource, setNewPluginSource] = useState("");
  const pluginUpdateSettings = updateSetting;

  useEffect(() => {
    if (plugins.length === 0 && !loading) void fetchPlugins();
  }, [fetchPlugins, loading, plugins.length]);

  if (selectedPlugin) {
    const plugin = plugins.find((entry) => entry.name === selectedPlugin);
    const valueFor = (key: string) => settingsValues.find((entry) => entry.key === key)?.value ?? "";
    return (
      <SettingsGrid>
        <Panel section="Plugins" title={selectedPlugin}>
          <button type="button" className="zorai-ghost-button" onClick={() => void selectPlugin(null)}>Back</button>
          {settingsSchema.length === 0 ? <p className="zorai-empty-state">No editable plugin settings.</p> : settingsSchema.map((field) => (
            <SettingRow key={field.key} label={`${field.label}${field.required ? " *" : ""}`} description={field.description ?? field.key}>
              <input className="zorai-input" type={field.secret ? "password" : "text"} defaultValue={valueFor(field.key)} onBlur={(event) => void pluginUpdateSettings(selectedPlugin, field.key, event.target.value, field.secret)} />
            </SettingRow>
          ))}
          {plugin?.has_api ? <button type="button" className="zorai-ghost-button" onClick={() => void testConnection(selectedPlugin)}>Test Connection</button> : null}
          {plugin?.has_auth ? <button type="button" className="zorai-ghost-button" onClick={() => void startOAuth(selectedPlugin)}>{plugin.auth_status === "not_configured" ? "Connect" : "Reconnect"}</button> : null}
          <button type="button" className="zorai-ghost-button" onClick={() => void uninstallPlugin(selectedPlugin)}>Remove plugin</button>
          {testResult ? <p className="zorai-empty-state">{testResult.success ? "OK" : "Failed"}: {testResult.message}</p> : null}
          {actionMessage ? <p className="zorai-empty-state">{actionMessage}</p> : null}
        </Panel>
      </SettingsGrid>
    );
  }

  return (
    <SettingsGrid>
      <Panel section="Plugins" title="Installed extensions">
        <button type="button" className="zorai-ghost-button" onClick={() => void fetchPlugins()}>{loading ? "Refreshing..." : "Refresh plugins"}</button>
        <SettingRow label="Add plugin" description="Register an installed plugin directory with the daemon.">
          <div className="zorai-inline-fields">
            <input className="zorai-input" value={newPluginDir} onChange={(event) => setNewPluginDir(event.target.value)} placeholder="dir-name" />
            <input className="zorai-input" value={newPluginSource} onChange={(event) => setNewPluginSource(event.target.value)} placeholder="install source" />
            <button type="button" className="zorai-ghost-button" onClick={() => {
              void installPlugin(newPluginDir, newPluginSource).then(() => {
                setNewPluginDir("");
                setNewPluginSource("");
              });
            }}>Add</button>
          </div>
        </SettingRow>
        {error ? <p className="zorai-empty-state">{error}</p> : null}
        {actionMessage ? <p className="zorai-empty-state">{actionMessage}</p> : null}
        {plugins.length === 0 && !loading ? <p className="zorai-empty-state">No plugins are currently reported by the plugin daemon.</p> : plugins.map((plugin) => (
          <SettingRow key={plugin.name} label={plugin.name} description={`${plugin.version} / ${plugin.endpoint_count} endpoints / auth ${plugin.auth_status}`}>
            <Switch checked={plugin.enabled} onChange={(checked) => void toggleEnabled(plugin.name, checked)} />
            <button type="button" className="zorai-ghost-button" onClick={() => void selectPlugin(plugin.name)}>Edit</button>
            <button type="button" className="zorai-ghost-button" onClick={() => void uninstallPlugin(plugin.name)}>Remove</button>
          </SettingRow>
        ))}
      </Panel>
    </SettingsGrid>
  );
}

function AboutPanel() {
  const agentSettings = useAgentStore((state) => state.agentSettings);
  const settings = useSettingsStore((state) => state.settings);

  return (
    <SettingsGrid>
      <Panel section="About" title={`${ZORAI_APP_NAME} shell`}>
        <Metric label="Version" value={APP_VERSION} />
        <Metric label="Author" value={APP_AUTHOR} />
        <Metric label="GitHub" value={APP_GITHUB} />
        <Metric label="Homepage" value={APP_HOMEPAGE} />
        <Metric label="Active provider" value={agentSettings.active_provider} />
        <Metric label="Backend" value="daemon" />
        <Metric label="Theme" value={settings.themeName} />
        <Metric label="Chat page size" value={String(agentSettings.react_chat_history_page_size)} />
      </Panel>
    </SettingsGrid>
  );
}

function useProviderIds(agentSettings: AgentSettings) {
  return useMemo(() => Object.keys(agentSettings).filter((key) => {
    const value = agentSettings[key];
    return value && typeof value === "object" && "model" in value && "base_url" in value;
  }).sort(), [agentSettings]);
}

function SettingsGrid({ children, extraClassName }: { children: ReactNode; extraClassName?: string }) {
  return <div className={["zorai-settings-grid", extraClassName ?? ""].filter(Boolean).join(" ")}>{children}</div>;
}

function Panel({ section, title, children, extraClassName }: { section: string; title: string; children: ReactNode; extraClassName?: string }) {
  return (
    <div className={["zorai-panel", extraClassName ?? ""].filter(Boolean).join(" ")}>
      <div><div className="zorai-section-label">{section}</div><h2>{title}</h2></div>
      {children}
    </div>
  );
}

function SettingRow({ label, description, children }: { label: string; description: string; children: ReactNode }) {
  return (
    <div className="zorai-setting-row">
      <div><strong>{label}</strong><span>{description}</span></div>
      {children}
    </div>
  );
}

function SecretRow({ label, value, onChange }: { label: string; value: string; onChange: (value: string) => void }) {
  return (
    <SettingRow label={label} description="Stored as a local gateway credential.">
      <input className="zorai-input" type="password" value={value} onChange={(event) => onChange(event.target.value)} />
    </SettingRow>
  );
}

function NumberRow({ label, description, value, onChange, min, max }: { label: string; description: string; value: number; onChange: (value: number) => void; min: number; max: number }) {
  return (
    <SettingRow label={label} description={description}>
      <input className="zorai-input" type="number" min={min} max={max} value={value} onChange={(event) => onChange(Number(event.target.value))} />
    </SettingRow>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return <div className="zorai-setting-row"><div><strong>{label}</strong><span>{value}</span></div></div>;
}

function Switch({ checked, onChange }: { checked: boolean; onChange: (checked: boolean) => void }) {
  return (
    <button type="button" className={["zorai-switch", checked ? "zorai-switch--on" : ""].filter(Boolean).join(" ")} aria-pressed={checked} onClick={() => onChange(!checked)}>
      <span />
    </button>
  );
}

function formatTransportLabel(transport: ApiTransportMode) {
  return transport.replace(/_/g, " ");
}
