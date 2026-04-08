import { formatSkillWorkflowNotice } from "./skillWorkflowNotice.ts";

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
    confidence_tier: "weak",
    recommended_action: "read_skill systematic-debugging",
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

const strongGate = formatSkillWorkflowNotice(
  "skill-gate",
  "Skill discovery gate blocked read_file.",
  JSON.stringify({
    recommended_skill: "systematic-debugging",
    confidence_tier: "strong",
    recommended_action: "read_skill systematic-debugging",
  }),
);

assert(
  strongGate.kind === "skill-discovery-required",
  "Strong skill gates should remain blocking requirements in the mission log",
);
