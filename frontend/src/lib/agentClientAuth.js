export function resolveProviderAuthDecision({
  provider,
  authSource,
  configuredApiKey,
  hasCodexStatusBridge,
  usesDaemonExecution,
  status,
}) {
  if (provider !== "openai" || authSource !== "chatgpt_subscription") {
    return {
      mode: "direct",
      apiKey: typeof configuredApiKey === "string" ? configuredApiKey : "",
    };
  }

  if (!hasCodexStatusBridge) {
    return {
      mode: "error",
      error: "ChatGPT subscription auth is unavailable in this build.",
    };
  }

  if (status?.available) {
    if (usesDaemonExecution) {
      return {
        mode: "daemon",
        accountId: typeof status.accountId === "string" ? status.accountId.trim() : undefined,
      };
    }

    return {
      mode: "error",
      error: "ChatGPT subscription auth requires daemon-backed execution. Switch Agent Backend to daemon.",
    };
  }

  return {
    mode: "error",
    error: typeof status?.error === "string" && status.error.trim()
      ? status.error.trim()
      : "ChatGPT subscription auth not found. Authenticate in Settings > Agent.",
  };
}
