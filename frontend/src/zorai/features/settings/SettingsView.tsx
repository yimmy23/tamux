import { useEffect } from "react";
import { SettingsPanel } from "@/components/SettingsPanel";
import { useAgentStore } from "@/lib/agentStore";
import { useSettingsStore } from "@/lib/settingsStore";
import { useWorkspaceStore } from "@/lib/workspaceStore";

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
  useEffect(() => {
    useWorkspaceStore.setState({ settingsOpen: true });
    return () => useWorkspaceStore.setState({ settingsOpen: false });
  }, []);

  return (
    <section className="zorai-feature-surface zorai-settings-surface">
      <SettingsPanel
        style={{
          position: "relative",
          inset: "auto",
          width: "100%",
          height: "100%",
          zIndex: "auto",
          background: "transparent",
          border: "0",
          padding: 0,
        }}
      />
    </section>
  );
}
