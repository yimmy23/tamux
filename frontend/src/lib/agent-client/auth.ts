import { getBridge } from "../bridge";
import { resolveProviderAuthDecision } from "../agentClientAuth.js";
import type { ChatRequest, OpenAICodexAuthStatus, ResolvedProviderAuth } from "./types";

export async function resolveProviderAuth(
  req: ChatRequest,
): Promise<ResolvedProviderAuth> {
  const zorai = getBridge();
  const isChatGptSubscription =
    req.provider === "openai" && req.config.auth_source === "chatgpt_subscription";
  const status =
    isChatGptSubscription && zorai?.openAICodexAuthStatus
      ? (await zorai.openAICodexAuthStatus({
          refresh: true,
        })) as OpenAICodexAuthStatus
      : undefined;
  const resolution = resolveProviderAuthDecision({
    provider: req.provider,
    authSource: req.config.auth_source,
    configuredApiKey: req.config.api_key,
    hasCodexStatusBridge: Boolean(zorai?.openAICodexAuthStatus),
    usesDaemonExecution: Boolean(zorai?.agentSendMessage),
    status,
  });

  if (resolution.mode === "error") {
    throw new Error(resolution.error);
  }

  if (resolution.mode === "daemon") {
    throw new Error(
      "ChatGPT subscription auth requires daemon-backed execution. This renderer path is API-key only.",
    );
  }

  return {
    api_key: resolution.apiKey,
    accountId: resolution.accountId,
  };
}
