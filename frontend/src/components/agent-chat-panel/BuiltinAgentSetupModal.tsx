import { useEffect, useState } from "react";
import { getProviderDefinition, PROVIDER_DEFINITIONS } from "@/lib/agentStore/providers";
import type { AgentProviderConfig, AgentProviderId } from "@/lib/agentStore";
import { ModelSelector, SettingRow, inputStyle, smallBtnStyle } from "@/components/settings-panel/shared";
import { useAgentChatPanelRuntime } from "./runtime/context";

export function BuiltinAgentSetupModal() {
  const runtime = useAgentChatPanelRuntime();
  const setup = runtime.builtinAgentSetup;
  const [providerId, setProviderId] = useState<AgentProviderId>("openai");
  const [model, setModel] = useState("");

  useEffect(() => {
    if (!setup) {
      return;
    }
    setProviderId(setup.providerId);
    setModel(setup.model);
  }, [setup]);

  if (!setup) {
    return null;
  }

  const providerOptions = PROVIDER_DEFINITIONS.filter((provider) => provider.id !== "custom");
  const providerConfig = runtime.agentSettings[providerId] as AgentProviderConfig | undefined;
  const providerLabel = getProviderDefinition(providerId)?.name ?? providerId;

  return (
    <div
      style={{
        position: "absolute",
        inset: 0,
        zIndex: 20,
        background: "rgba(6, 8, 14, 0.72)",
        display: "grid",
        placeItems: "center",
        padding: "var(--space-4)",
      }}
    >
      <div
        style={{
          width: "min(520px, 100%)",
          display: "grid",
          gap: "var(--space-4)",
          padding: "var(--space-5)",
          borderRadius: "var(--radius-xl)",
          border: "1px solid var(--border)",
          background: "linear-gradient(180deg, rgba(18,24,36,0.98), rgba(11,15,24,0.98))",
          boxShadow: "0 24px 80px rgba(0,0,0,0.38)",
        }}
      >
        <div style={{ display: "grid", gap: "var(--space-1)" }}>
          <div style={{ fontSize: "var(--text-xs)", letterSpacing: "0.08em", textTransform: "uppercase", color: "var(--accent)" }}>
            Builtin Agent Setup
          </div>
          <div style={{ fontSize: "var(--text-lg)", fontWeight: 700 }}>
            Configure {setup.targetAgentName}
          </div>
          <div style={{ color: "var(--text-secondary)", lineHeight: 1.5, fontSize: "var(--text-sm)" }}>
            This builtin persona needs its own runtime before it can join thread workflow. Choose a provider, then a model.
          </div>
        </div>

        <div style={{ display: "grid", gap: "var(--space-3)" }}>
          <SettingRow label="Provider">
            <select
              value={providerId}
              onChange={(event) => {
                const nextProviderId = event.target.value as AgentProviderId;
                setProviderId(nextProviderId);
                setModel(
                  ((runtime.agentSettings[nextProviderId] as AgentProviderConfig | undefined)?.model || getProviderDefinition(nextProviderId)?.defaultModel || ""),
                );
              }}
              style={{ ...inputStyle, width: 240 }}
            >
              {providerOptions.map((provider) => (
                <option key={provider.id} value={provider.id}>{provider.name}</option>
              ))}
            </select>
          </SettingRow>

          <SettingRow label="Model">
            <div style={{ width: 280 }}>
              <ModelSelector
                providerId={providerId}
                value={model}
                onChange={(value) => setModel(value)}
                base_url={providerConfig?.base_url}
                api_key={providerConfig?.api_key}
                auth_source={providerConfig?.auth_source}
              />
            </div>
          </SettingRow>

          <div style={{ fontSize: "var(--text-xs)", color: "var(--text-muted)" }}>
            Selected runtime: {providerLabel} / {model || "choose a model"}
          </div>

          {setup.error && (
            <div
              style={{
                border: "1px solid rgba(255, 120, 120, 0.35)",
                background: "rgba(120, 18, 18, 0.22)",
                color: "rgb(255, 210, 210)",
                borderRadius: "var(--radius-md)",
                padding: "var(--space-3)",
                fontSize: "var(--text-sm)",
                lineHeight: 1.5,
              }}
            >
              {setup.error}
            </div>
          )}
        </div>

        <div style={{ display: "flex", justifyContent: "flex-end", gap: "var(--space-2)" }}>
          <button type="button" onClick={runtime.cancelBuiltinAgentSetup} style={smallBtnStyle}>
            Cancel
          </button>
          <button
            type="button"
            onClick={() => void runtime.submitBuiltinAgentSetup(providerId, model)}
            disabled={!model.trim()}
            style={{
              ...smallBtnStyle,
              opacity: model.trim() ? 1 : 0.5,
              cursor: model.trim() ? "pointer" : "not-allowed",
            }}
          >
            Save And Retry
          </button>
        </div>
      </div>
    </div>
  );
}
