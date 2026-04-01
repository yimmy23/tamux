import type { SubAgentDefinition } from "./types";
import { getSubAgentCapabilities, sanitizeSubAgentUpdate } from "./providerActions";

const weles: SubAgentDefinition = {
  id: "weles_builtin",
  name: "WELES",
  provider: "openai",
  model: "gpt-4o-mini",
  enabled: true,
  builtin: true,
  immutable_identity: true,
  disable_allowed: false,
  delete_allowed: false,
  protected_reason: "Daemon-owned WELES registry entry",
  reasoning_effort: "medium",
  created_at: 1,
};

const capabilities = getSubAgentCapabilities(weles);
if (capabilities.canToggle) {
  throw new Error("WELES should not be toggleable in the protected UI model");
}
if (capabilities.canDelete) {
  throw new Error("WELES should not be deletable in the protected UI model");
}
if (!capabilities.isProtected) {
  throw new Error("WELES should be marked protected in the UI model");
}
if (!capabilities.protectedReason.includes("WELES")) {
  throw new Error("Protected WELES reason should be surfaced to the UI");
}

const sanitized = sanitizeSubAgentUpdate(weles, {
  ...weles,
  name: "Forged WELES",
  enabled: false,
  reasoning_effort: "high",
  system_prompt: "Operator suffix",
});

if (sanitized.name !== "WELES") {
  throw new Error("Protected WELES updates must preserve immutable name");
}
if (sanitized.enabled !== true) {
  throw new Error("Protected WELES updates must preserve enabled state");
}
if (sanitized.reasoning_effort !== "high") {
  throw new Error("Protected WELES updates should still allow reasoning effort edits");
}
