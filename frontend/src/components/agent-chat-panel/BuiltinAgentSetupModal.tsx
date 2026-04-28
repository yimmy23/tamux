import type { AgentProviderId } from "@/lib/agentStore";
import { BuiltinAgentRuntimeSetupDialog } from "./BuiltinAgentRuntimeSetupDialog";
import { useAgentChatPanelRuntime } from "./runtime/context";

export function BuiltinAgentSetupModal() {
  const runtime = useAgentChatPanelRuntime();
  const setup = runtime.builtinAgentSetup;

  if (!setup) {
    return null;
  }

  return (
    <BuiltinAgentRuntimeSetupDialog
      value={setup}
      agentSettings={runtime.agentSettings}
      onCancel={runtime.cancelBuiltinAgentSetup}
      onSubmit={(providerId: AgentProviderId, model: string) => runtime.submitBuiltinAgentSetup(providerId, model)}
    />
  );
}
