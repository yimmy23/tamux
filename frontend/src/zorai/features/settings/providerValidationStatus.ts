export type ProviderValidationStatus = {
  state: "testing" | "success" | "error";
  message: string;
};

export type ProviderValidationStatusMap = Record<string, ProviderValidationStatus>;

export type ProviderValidationResult = {
  valid: boolean;
  error?: string;
};

export function markProviderValidationTesting(
  statuses: ProviderValidationStatusMap,
  providerId: string,
): ProviderValidationStatusMap {
  return {
    ...statuses,
    [providerId]: { state: "testing", message: "Testing..." },
  };
}

export function providerValidationStatusFromResult(
  result: ProviderValidationResult,
): ProviderValidationStatus {
  if (result.valid) {
    return { state: "success", message: "ok" };
  }
  const error = result.error?.trim();
  return { state: "error", message: error || "failed" };
}

export function formatProviderValidationError(error: unknown): string {
  if (error instanceof Error && error.message.trim()) {
    return error.message;
  }
  if (typeof error === "string" && error.trim()) {
    return error;
  }
  return "failed";
}

export function getProviderValidationButtonLabel(
  statuses: ProviderValidationStatusMap,
  providerId: string,
): string {
  return statuses[providerId]?.state === "testing" ? "Testing..." : "Test";
}
