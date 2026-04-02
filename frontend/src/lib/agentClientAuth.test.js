import test from "node:test";
import assert from "node:assert/strict";

import { resolveProviderAuthDecision } from "./agentClientAuth.js";

test("metadata-only codex auth status no longer resolves direct provider auth", () => {
  const result = resolveProviderAuthDecision({
    provider: "openai",
    authSource: "chatgpt_subscription",
    configuredApiKey: "",
    hasCodexStatusBridge: true,
    usesDaemonExecution: false,
    status: {
      available: true,
      authMode: "chatgpt_subscription",
      accountId: " account-123 ",
    },
  });

  assert.deepEqual(result, {
    mode: "error",
    error: "ChatGPT subscription auth requires daemon-backed execution. Switch Agent Backend to daemon.",
  });
});

test("daemon-backed chatgpt subscription resolution reuses daemon auth state", () => {
  const result = resolveProviderAuthDecision({
    provider: "openai",
    authSource: "chatgpt_subscription",
    configuredApiKey: "",
    hasCodexStatusBridge: true,
    usesDaemonExecution: true,
    status: {
      available: true,
      authMode: "chatgpt_subscription",
      accountId: " account-123 ",
    },
  });

  assert.deepEqual(result, {
    mode: "daemon",
    accountId: "account-123",
  });
});

test("non-daemon chatgpt subscription resolution fails clearly", () => {
  const result = resolveProviderAuthDecision({
    provider: "openai",
    authSource: "chatgpt_subscription",
    configuredApiKey: "",
    hasCodexStatusBridge: true,
    usesDaemonExecution: false,
    status: {
      available: true,
      authMode: "chatgpt_subscription",
    },
  });

  assert.deepEqual(result, {
    mode: "error",
    error: "ChatGPT subscription auth requires daemon-backed execution. Switch Agent Backend to daemon.",
  });
});
