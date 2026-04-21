import { getBridge } from "./bridge";

let pendingAgentConfigRequest: Promise<unknown | null> | null = null;

export async function getDaemonAgentConfig(): Promise<unknown | null> {
  const bridge = getBridge();
  if (!bridge?.agentGetConfig) {
    return null;
  }
  if (!pendingAgentConfigRequest) {
    pendingAgentConfigRequest = Promise.resolve()
      .then(() => bridge.agentGetConfig?.() ?? null)
      .catch(() => null)
      .finally(() => {
        pendingAgentConfigRequest = null;
      });
  }
  return pendingAgentConfigRequest;
}
