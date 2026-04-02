export type OpenAICodexAuthSnapshot = {
  available?: boolean | null;
  status?: string | null;
  authUrl?: string | null;
};

function normalizeStatus(status: string | null | undefined): string | null {
  if (typeof status !== "string") {
    return null;
  }

  const trimmed = status.trim().toLowerCase();
  return trimmed || null;
}

function normalizeAuthUrl(authUrl: string | null | undefined): string | null {
  if (typeof authUrl !== "string") {
    return null;
  }

  const trimmed = authUrl.trim();
  return trimmed || null;
}

export function deriveOpenAICodexAuthUi(snapshot: OpenAICodexAuthSnapshot | null | undefined): {
  authUrl: string | null;
  isTerminal: boolean;
  shouldPoll: boolean;
} {
  const status = normalizeStatus(snapshot?.status);
  const authUrl = normalizeAuthUrl(snapshot?.authUrl);
  const isPending = status === "pending";
  const isAvailable = snapshot?.available === true;
  const isTerminal = isAvailable || status === "completed" || status === "error" || (!isPending && !authUrl);

  return {
    authUrl: isPending ? authUrl : null,
    isTerminal,
    shouldPoll: isPending && Boolean(authUrl),
  };
}
