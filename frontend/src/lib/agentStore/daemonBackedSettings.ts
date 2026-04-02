export function normalizeDaemonBackedAgentMode(
  agentBackend: string,
  activeProvider: string,
  authSource: string,
): string {
  if (
    agentBackend === "legacy"
    && activeProvider === "openai"
    && authSource === "chatgpt_subscription"
  ) {
    return "daemon";
  }

  return agentBackend;
}
