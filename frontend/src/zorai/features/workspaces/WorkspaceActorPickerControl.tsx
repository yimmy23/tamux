import { useEffect, useMemo, useState } from "react";
import { BuiltinAgentRuntimeSetupDialog } from "@/components/agent-chat-panel/BuiltinAgentRuntimeSetupDialog";
import { normalizeBridgePayload } from "@/components/agent-chat-panel/runtime/daemonHelpers";
import { useAgentStore } from "@/lib/agentStore";
import { getProviderDefinition } from "@/lib/agentStore/providers";
import type { AgentProviderId } from "@/lib/agentStore/types";
import { getAgentBridge } from "@/lib/agentDaemonConfig";
import {
  normalizeActorValue,
  workspaceActorPickerOptions,
  type WorkspaceActorPickerMode,
  type WorkspaceActorPickerOption,
} from "./workspaceActorPicker";

type PendingSetup = {
  option: WorkspaceActorPickerOption;
  error: string | null;
};

export function WorkspaceActorPickerControl({
  mode,
  value,
  onChange,
}: {
  mode: WorkspaceActorPickerMode;
  value: string;
  onChange: (value: string) => void;
}) {
  const subAgents = useAgentStore((state) => state.subAgents);
  const refreshSubAgents = useAgentStore((state) => state.refreshSubAgents);
  const agentSettings = useAgentStore((state) => state.agentSettings);
  const [pendingSetup, setPendingSetup] = useState<PendingSetup | null>(null);
  const normalizedValue = normalizeActorValue(value);
  const options = useMemo(() => {
    const pickerOptions = workspaceActorPickerOptions(mode, subAgents);
    if (!normalizedValue || pickerOptions.some((option) => option.value === normalizedValue)) {
      return pickerOptions;
    }
    return [
      ...pickerOptions,
      {
        label: normalizedValue,
        value: normalizedValue,
        targetAgentId: normalizedValue.includes(":") ? normalizedValue.split(":").slice(1).join(":") : normalizedValue,
        requiresRuntimeSetup: false,
      },
    ];
  }, [mode, normalizedValue, subAgents]);
  const defaultProviderId = agentSettings.active_provider as AgentProviderId;
  const defaultProviderConfig = agentSettings[defaultProviderId] as { model?: string } | undefined;
  const defaultModel = defaultProviderConfig?.model?.trim()
    || getProviderDefinition(defaultProviderId)?.defaultModel
    || "";

  useEffect(() => {
    void refreshSubAgents();
  }, [refreshSubAgents]);

  const handleSelect = (nextValue: string) => {
    const option = options.find((entry) => entry.value === nextValue) ?? null;
    if (option?.requiresRuntimeSetup && option.targetAgentId) {
      setPendingSetup({ option, error: null });
      return;
    }
    onChange(nextValue);
  };

  const saveRuntimeSetup = async (providerId: AgentProviderId, model: string) => {
    if (!pendingSetup?.option.targetAgentId) return;
    const amux = getAgentBridge();
    if (!amux?.agentSetTargetAgentProviderModel) {
      setPendingSetup({ ...pendingSetup, error: "Builtin agent runtime setup is not available in this runtime." });
      return;
    }
    const response = await amux.agentSetTargetAgentProviderModel(
      pendingSetup.option.targetAgentId,
      providerId,
      model,
    );
    const payload = normalizeBridgePayload(response);
    if (payload?.ok === false && typeof payload?.error === "string") {
      setPendingSetup({ ...pendingSetup, error: payload.error });
      return;
    }
    await refreshSubAgents();
    onChange(pendingSetup.option.value);
    setPendingSetup(null);
  };

  return (
    <>
      <select
        aria-label={mode}
        value={normalizedValue}
        onChange={(event) => handleSelect(event.target.value)}
      >
        {options.map((option) => (
          <option key={option.value || "none"} value={option.value}>
            {option.label}{option.requiresRuntimeSetup ? " / setup" : ""}
          </option>
        ))}
      </select>
      {pendingSetup ? (
        <BuiltinAgentRuntimeSetupDialog
          value={{
            targetAgentName: pendingSetup.option.label,
            providerId: defaultProviderId,
            model: defaultModel,
            error: pendingSetup.error,
          }}
          agentSettings={agentSettings}
          submitLabel="Save And Select"
          onCancel={() => setPendingSetup(null)}
          onSubmit={saveRuntimeSetup}
        />
      ) : null}
    </>
  );
}
