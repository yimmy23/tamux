export type AgentDirective =
  | { kind: "internal_delegate"; agentAlias: string; body: string }
  | { kind: "participant_upsert"; agentAlias: string; body: string }
  | { kind: "participant_deactivate"; agentAlias: string };

function isKnownAgentAlias(agentAlias: string, knownAgentAliases: string[]): boolean {
  const normalized = agentAlias.trim().toLowerCase();
  if (!normalized) {
    return false;
  }

  return knownAgentAliases.some((candidate) => candidate.trim().toLowerCase() === normalized);
}

export function parseLeadingAgentDirective(
  text: string,
  knownAgentAliases: string[],
): AgentDirective | null {
  const trimmed = text.trimStart();
  const prefix = trimmed[0];
  if (prefix !== "!" && prefix !== "@") {
    return null;
  }

  const tokenMatch = /^([!@])([^\s]+)(?:\s+([\s\S]+))?$/.exec(trimmed);
  if (!tokenMatch) {
    return null;
  }

  const [, directivePrefix, agentAliasRaw, bodyRaw = ""] = tokenMatch;
  const agentAlias = agentAliasRaw.trim();
  const body = bodyRaw.trim();
  if (!isKnownAgentAlias(agentAlias, knownAgentAliases) || !body) {
    return null;
  }

  if (directivePrefix === "!") {
    return { kind: "internal_delegate", agentAlias, body };
  }

  switch (body.toLowerCase()) {
    case "leave":
    case "stop":
    case "done":
    case "return":
      return { kind: "participant_deactivate", agentAlias };
    default:
      return { kind: "participant_upsert", agentAlias, body };
  }
}