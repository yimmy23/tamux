import { formatSkillWorkflowNotice } from "./skillWorkflowNotice";

function assert(condition: unknown, message: string): void {
  if (!condition) {
    throw new Error(message);
  }
}

const weakGate = formatSkillWorkflowNotice(
  "skill-gate",
  "Weak skill discovery recommends read_skill systematic-debugging before read_file; allowing the tool call to proceed.",
  JSON.stringify({
    recommended_skill: "systematic-debugging",
    normalized_intent: "debug panic root cause",
    confidence_tier: "weak",
    recommended_action: "read_skill systematic-debugging",
    mesh_state: "fresh",
    rationale: ["matched debug intent"],
    capability_family: ["development", "debugging"],
  }),
);

assert(
  weakGate.kind === "skill-discovery-recommended",
  "Weak skill gates should remain recommendations in the mission log",
);
assert(
  weakGate.message?.includes("allowed progress") ?? false,
  "Weak skill gates should describe that progress was allowed",
);
assert(
  weakGate.message?.includes("debug panic root cause") ?? false,
  "Weak skill notices should surface normalized intent",
);
assert(
  weakGate.message?.includes("development / debugging") ?? false,
  "Weak skill notices should surface capability family",
);

const strongGate = formatSkillWorkflowNotice(
  "skill-gate",
  "Skill discovery gate blocked read_file.",
  JSON.stringify({
    recommended_skill: "systematic-debugging",
    normalized_intent: "debug panic root cause",
    confidence_tier: "strong",
    recommended_action: "read_skill systematic-debugging",
    mesh_state: "degraded",
    requires_approval: true,
    rationale: ["matched debug intent"],
  }),
);

assert(
  strongGate.kind === "skill-discovery-required",
  "Strong skill gates should remain blocking requirements in the mission log",
);
assert(
  strongGate.message?.includes("approval required") ?? false,
  "Strong skill notices should mention approval when required",
);
assert(
  strongGate.message?.includes("mesh=degraded") ?? false,
  "Strong skill notices should surface mesh state",
);
