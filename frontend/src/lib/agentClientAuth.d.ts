export type ProviderAuthDecision =
  | {
    mode: "direct";
    apiKey: string;
    accountId?: string;
  }
  | {
    mode: "daemon";
    accountId?: string;
  }
  | {
    mode: "error";
    error: string;
  };

export function resolveProviderAuthDecision(input: {
  provider: string;
  authSource: string | undefined;
  configuredApiKey: string | undefined;
  hasCodexStatusBridge: boolean;
  usesDaemonExecution: boolean;
  status?: {
    available?: boolean;
    accountId?: string;
    error?: string;
  };
}): ProviderAuthDecision;
