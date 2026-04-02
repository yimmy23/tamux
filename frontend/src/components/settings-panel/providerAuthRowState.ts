import type { AuthSource } from "@/lib/agentStore";

type OpenAIProviderRowStateArgs = {
  providerId: string;
  providerAuthenticated: boolean;
  providerAuthSource: AuthSource;
  selectedAuthSource?: AuthSource | null;
  chatgptAvailable: boolean;
};

export type OpenAIProviderRowState = {
  authenticated: boolean;
  showApiKeyLogin: boolean;
  showApiKeyLogout: boolean;
  showChatgptLogin: boolean;
  showChatgptLogout: boolean;
};

export function resolveOpenAIProviderRowState({
  providerId,
  providerAuthenticated,
  providerAuthSource: _providerAuthSource,
  selectedAuthSource: _selectedAuthSource,
  chatgptAvailable,
}: OpenAIProviderRowStateArgs): OpenAIProviderRowState {
  const isOpenAI = providerId === "openai";
  const apiKeyAuthenticated = providerAuthenticated;
  const authenticated = isOpenAI ? (apiKeyAuthenticated || chatgptAvailable) : apiKeyAuthenticated;

  return {
    authenticated,
    showApiKeyLogin: !apiKeyAuthenticated,
    showApiKeyLogout: apiKeyAuthenticated,
    showChatgptLogin: isOpenAI && !chatgptAvailable,
    showChatgptLogout: isOpenAI && chatgptAvailable,
  };
}
