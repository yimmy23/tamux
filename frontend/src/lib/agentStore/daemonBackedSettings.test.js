import test from "node:test";
import assert from "node:assert/strict";

import { normalizeDaemonBackedAgentMode } from "./daemonBackedSettings.ts";

test("legacy chatgpt subscription settings normalize to daemon mode", () => {
  assert.equal(
    normalizeDaemonBackedAgentMode("legacy", "openai", "chatgpt_subscription"),
    "daemon",
  );
});

test("all former external and legacy backend values normalize to daemon mode", () => {
  assert.equal(
    normalizeDaemonBackedAgentMode("legacy", "openai", "api_key"),
    "daemon",
  );
  assert.equal(
    normalizeDaemonBackedAgentMode("legacy", "github-copilot", "github_copilot"),
    "daemon",
  );
  assert.equal(
    normalizeDaemonBackedAgentMode("hermes", "openai", "api_key"),
    "daemon",
  );
  assert.equal(
    normalizeDaemonBackedAgentMode("openclaw", "openai", "api_key"),
    "daemon",
  );
});
