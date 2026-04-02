import test from "node:test";
import assert from "node:assert/strict";

import { deriveOpenAICodexAuthUi } from "./openaiSubscriptionAuth.ts";

test("pending auth keeps polling and preserves auth url", () => {
  assert.deepEqual(
    deriveOpenAICodexAuthUi({
      available: false,
      status: "pending",
      authUrl: "https://example.test/auth",
    }),
    {
      authUrl: "https://example.test/auth",
      isTerminal: false,
      shouldPoll: true,
    },
  );
});

test("completed auth clears stale auth url and stops polling", () => {
  assert.deepEqual(
    deriveOpenAICodexAuthUi({
      available: true,
      status: "completed",
      authUrl: "https://example.test/stale",
    }),
    {
      authUrl: null,
      isTerminal: true,
      shouldPoll: false,
    },
  );
});

test("error auth clears stale auth url and stops polling", () => {
  assert.deepEqual(
    deriveOpenAICodexAuthUi({
      available: false,
      status: "error",
      authUrl: "https://example.test/stale",
    }),
    {
      authUrl: null,
      isTerminal: true,
      shouldPoll: false,
    },
  );
});

test("blank pending auth url is treated as cleared", () => {
  assert.deepEqual(
    deriveOpenAICodexAuthUi({
      available: false,
      status: "pending",
      authUrl: "   ",
    }),
    {
      authUrl: null,
      isTerminal: false,
      shouldPoll: false,
    },
  );
});

test("missing status without auth url is terminal and not polled", () => {
  assert.deepEqual(
    deriveOpenAICodexAuthUi({
      available: false,
      status: null,
      authUrl: null,
    }),
    {
      authUrl: null,
      isTerminal: true,
      shouldPoll: false,
    },
  );
});
