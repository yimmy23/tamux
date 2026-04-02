import test from "node:test";
import assert from "node:assert/strict";

import { normalizeDaemonBackedAgentMode } from "./daemonBackedSettings.ts";

test("legacy chatgpt subscription settings migrate to daemon mode", () => {
  assert.equal(
    normalizeDaemonBackedAgentMode("legacy", "openai", "chatgpt_subscription"),
    "daemon",
  );
});

test("non-chatgpt legacy settings keep their existing mode", () => {
  assert.equal(
    normalizeDaemonBackedAgentMode("legacy", "openai", "api_key"),
    "legacy",
  );
  assert.equal(
    normalizeDaemonBackedAgentMode("legacy", "github-copilot", "github_copilot"),
    "legacy",
  );
});
