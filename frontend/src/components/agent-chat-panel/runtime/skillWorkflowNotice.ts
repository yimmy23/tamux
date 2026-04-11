function parseWorkflowDetails(details: string | null): Record<string, unknown> | null {
  if (!details) {
    return null;
  }
  try {
    const parsed: unknown = JSON.parse(details);
    return parsed && typeof parsed === "object"
      ? (parsed as Record<string, unknown>)
      : null;
  } catch {
    return null;
  }
}

export function formatSkillWorkflowNotice(
  kind: string,
  message: string | null,
  details: string | null,
): { kind: string; message: string | null } {
  const parsed = parseWorkflowDetails(details);
  const recommendedSkill =
    typeof parsed?.recommended_skill === "string" ? parsed.recommended_skill : null;
  const confidenceTier =
    typeof parsed?.confidence_tier === "string" ? parsed.confidence_tier : null;
  const recommendedAction =
    typeof parsed?.recommended_action === "string" ? parsed.recommended_action : null;
  const normalizedIntent =
    typeof parsed?.normalized_intent === "string" ? parsed.normalized_intent : null;
  const meshState = typeof parsed?.mesh_state === "string" ? parsed.mesh_state : null;
  const requiresApproval = parsed?.requires_approval === true;
  const rationaleRaw = parsed?.rationale;
  const rationale = Array.isArray(rationaleRaw)
    ? rationaleRaw.filter((value): value is string => typeof value === "string" && value.trim().length > 0)
    : [];
  const capabilityFamilyRaw = parsed?.capability_family;
  const capabilityFamily = Array.isArray(capabilityFamilyRaw)
    ? capabilityFamilyRaw.filter((value): value is string => typeof value === "string" && value.trim().length > 0)
    : [];
  const skipRationale =
    typeof parsed?.skip_rationale === "string" ? parsed.skip_rationale : null;

  if (kind === "skill-preflight") {
    const nextKind = confidenceTier === "strong"
      ? "skill-discovery-required"
      : "skill-discovery-recommended";
    const summary = [
      confidenceTier === "strong" ? "Skill gate required" : "Skill guidance ready",
      recommendedSkill ? `skill=${recommendedSkill}` : null,
      normalizedIntent ? `intent=${normalizedIntent}` : null,
      confidenceTier ? `confidence=${confidenceTier}` : null,
      recommendedAction ? `next=${recommendedAction}` : null,
      capabilityFamily.length ? `family=${capabilityFamily.join(" / ")}` : null,
      meshState ? `mesh=${meshState}` : null,
      requiresApproval ? "approval required" : null,
      rationale.length ? `why=${rationale.join(", ")}` : null,
    ].filter(Boolean).join(" | ");
    return { kind: nextKind, message: summary || message };
  }

  if (kind === "skill-gate") {
    const blocked = confidenceTier === "strong";
    const summary = [
      blocked ? "Skill gate blocked progress" : "Skill guidance allowed progress",
      recommendedSkill ? `skill=${recommendedSkill}` : null,
      normalizedIntent ? `intent=${normalizedIntent}` : null,
      confidenceTier ? `confidence=${confidenceTier}` : null,
      recommendedAction ? `next=${recommendedAction}` : null,
      capabilityFamily.length ? `family=${capabilityFamily.join(" / ")}` : null,
      meshState ? `mesh=${meshState}` : null,
      requiresApproval ? "approval required" : null,
      rationale.length ? `why=${rationale.join(", ")}` : null,
    ].filter(Boolean).join(" | ");
    return {
      kind: blocked ? "skill-discovery-required" : "skill-discovery-recommended",
      message: summary || message,
    };
  }

  if (kind === "skill-discovery-skipped") {
    const summary = [
      "Skill recommendation skipped",
      recommendedSkill ? `skill=${recommendedSkill}` : null,
      skipRationale ? `why=${skipRationale}` : null,
    ].filter(Boolean).join(" | ");
    return { kind, message: summary || message };
  }

  if (kind === "skill-community-scout") {
    const candidates = Array.isArray(parsed?.candidates) ? parsed.candidates.length : null;
    const timeout =
      typeof parsed?.community_preapprove_timeout_secs === "number"
        ? parsed.community_preapprove_timeout_secs
        : null;
    const summary = [
      "Community scout update",
      typeof candidates === "number" ? `candidates=${candidates}` : null,
      typeof timeout === "number" ? `timeout=${timeout}s` : null,
    ].filter(Boolean).join(" | ");
    return { kind, message: summary || message };
  }

  return { kind, message };
}
