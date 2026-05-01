import { useEffect, useRef } from "react";
import {
  buildDaemonAgentConfig,
  diffDaemonConfigEntries,
  getAgentBridge,
  shouldUseDaemonRuntime,
} from "@/lib/agentDaemonConfig";
import { useAgentStore } from "@/lib/agentStore";
import { useSettingsStore } from "@/lib/settingsStore";
import { ZORAI_APP_NAME } from "@/zorai/branding";
import { SettingsTabPanel } from "./SettingsPanels";
import { getZoraiSettingsTab, zoraiSettingsTabs, type ZoraiSettingsTabId } from "./settingsTabs";

type SettingsProps = {
  activeTab: ZoraiSettingsTabId;
  onSelectTab: (tabId: ZoraiSettingsTabId) => void;
};

export function SettingsRail({ activeTab, onSelectTab }: SettingsProps) {
  return (
    <div className="zorai-rail-stack">
      <div className="zorai-section-label">Settings</div>
      {zoraiSettingsTabs.map((tab) => (
        <button
          type="button"
          key={tab.id}
          className={[
            "zorai-rail-card",
            "zorai-rail-card--button",
            tab.id === activeTab ? "zorai-rail-card--active" : "",
          ].filter(Boolean).join(" ")}
          onClick={() => onSelectTab(tab.id)}
        >
          <strong>{tab.title}</strong>
          <span>{tab.description}</span>
        </button>
      ))}
    </div>
  );
}

export function SettingsView({ activeTab, onSelectTab }: SettingsProps) {
  const selectedTab = getZoraiSettingsTab(activeTab);
  const settings = useSettingsStore((state) => state.settings);
  const agentSettings = useAgentStore((state) => state.agentSettings);
  const agentSettingsHydrated = useAgentStore((state) => state.agentSettingsHydrated);
  const refreshAgentSettingsFromDaemon = useAgentStore((state) => state.refreshAgentSettingsFromDaemon);
  const refreshConciergeConfig = useAgentStore((state) => state.refreshConciergeConfig);
  const markAgentSettingsSynced = useAgentStore((state) => state.markAgentSettingsSynced);
  const lastDaemonConfigJsonRef = useRef<string | null>(null);

  useEffect(() => {
    void refreshConciergeConfig();
    void refreshAgentSettingsFromDaemon().then((ok) => {
      if (!ok) return;
      const latestAgentSettings = useAgentStore.getState().agentSettings;
      lastDaemonConfigJsonRef.current = JSON.stringify(
        buildDaemonAgentConfig(latestAgentSettings, settings),
      );
    });
  }, [refreshAgentSettingsFromDaemon, refreshConciergeConfig, settings]);

  useEffect(() => {
    if (!agentSettingsHydrated) return;
    if (!shouldUseDaemonRuntime(agentSettings.agent_backend)) return;
    const bridge = getAgentBridge();
    if (!bridge?.agentSetConfigItem) return;
    const nextConfig = buildDaemonAgentConfig(agentSettings, settings);
    const nextConfigJson = JSON.stringify(nextConfig);
    if (lastDaemonConfigJsonRef.current === null) return;
    if (lastDaemonConfigJsonRef.current === nextConfigJson) return;
    const previousConfig = JSON.parse(lastDaemonConfigJsonRef.current);
    const providerChanged = previousConfig.provider !== nextConfig.provider;
    const modelChanged = previousConfig.model !== nextConfig.model;
    const canSwitchProviderModel = (providerChanged || modelChanged)
      && typeof bridge.agentSetProviderModel === "function";
    const changes = diffDaemonConfigEntries(previousConfig, nextConfig);
    const remainingChanges = canSwitchProviderModel
      ? changes.filter(({ keyPath }) => keyPath !== "/provider" && keyPath !== "/model" && keyPath !== "/base_url")
      : changes;
    const syncConfig = async () => {
      if (canSwitchProviderModel) {
        await bridge.agentSetProviderModel?.(nextConfig.provider, nextConfig.model);
      }
      if (remainingChanges.length > 0) {
        await Promise.all(
          remainingChanges.map(({ keyPath, value }) => bridge.agentSetConfigItem?.(keyPath, value)),
        );
      }
    };
    void syncConfig().then(() => {
      lastDaemonConfigJsonRef.current = nextConfigJson;
      markAgentSettingsSynced();
    }).catch(() => { });
  }, [
    agentSettings,
    agentSettingsHydrated,
    markAgentSettingsSynced,
    settings,
  ]);

  return (
    <section className="zorai-feature-surface zorai-settings-surface">
      <div className="zorai-view-header">
        <div>
          <div className="zorai-kicker">Settings</div>
          <h1>{selectedTab.title}</h1>
          <p>{selectedTab.description} Configure {ZORAI_APP_NAME} without leaving orchestration.</p>
        </div>
      </div>

      <div className="zorai-settings-tab-strip" aria-label="Settings sections">
        {zoraiSettingsTabs.map((tab) => (
          <button
            type="button"
            key={tab.id}
            className={["zorai-ghost-button", tab.id === activeTab ? "zorai-button--active" : ""].filter(Boolean).join(" ")}
            onClick={() => onSelectTab(tab.id)}
          >
            {tab.title}
          </button>
        ))}
      </div>

      <SettingsTabPanel activeTab={activeTab} />
    </section>
  );
}
