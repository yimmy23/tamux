export const zoraiSettingsTabs = [
  { id: "auth", title: "Auth", description: "Provider authentication status and login actions." },
  { id: "model", title: "Svarog", description: "Main agent provider, model, transport, and identity." },
  { id: "concierge", title: "Rarog", description: "Welcome agent and operational assistant behavior." },
  { id: "tools", title: "Tools", description: "Agent capability toggles and tool loop limits." },
  { id: "search", title: "Search", description: "Web search and browsing provider settings." },
  { id: "runtime", title: "Chat", description: "Streaming, memory, collaboration, and chat behavior." },
  { id: "gateway", title: "Gateway", description: "Slack, Discord, Telegram, and WhatsApp bridge." },
  { id: "subagents", title: "Sub-agents", description: "Delegated roles attached to the orchestration runtime." },
  { id: "features", title: "Features", description: "Feature tier, heartbeat, skills, audio, and media settings." },
  { id: "advanced", title: "Advanced", description: "Context compaction, retry, and safety settings." },
  { id: "plugins", title: "Plugins", description: "Installed plugin runtime and enabled extensions." },
  { id: "interface", title: "Terminal interface", description: "Terminal shell presentation preferences." },
  { id: "about", title: "About", description: "Local runtime identity and shell status." },
] as const;

export type ZoraiSettingsTabId = (typeof zoraiSettingsTabs)[number]["id"];

export function getDefaultZoraiSettingsTab(): ZoraiSettingsTabId {
  return "auth";
}

export function getZoraiSettingsTab(tabId: ZoraiSettingsTabId) {
  return zoraiSettingsTabs.find((tab) => tab.id === tabId) ?? zoraiSettingsTabs[0];
}
