import test from "node:test";
import assert from "node:assert/strict";

import { resolveOpenAIProviderRowState } from "./providerAuthRowState.ts";

test("openai auth row shows green state and chatgpt logout when daemon chatgpt auth is available", () => {
  assert.deepEqual(
    resolveOpenAIProviderRowState({
      providerId: "openai",
      providerAuthenticated: false,
      providerAuthSource: "api_key",
      selectedAuthSource: "api_key",
      chatgptAvailable: true,
    }),
    {
      authenticated: true,
      showApiKeyLogin: true,
      showApiKeyLogout: false,
      showChatgptLogin: false,
      showChatgptLogout: true,
    },
  );
});

test("openai auth row keeps both login affordances when neither api key nor chatgpt auth exists", () => {
  assert.deepEqual(
    resolveOpenAIProviderRowState({
      providerId: "openai",
      providerAuthenticated: false,
      providerAuthSource: "api_key",
      selectedAuthSource: "api_key",
      chatgptAvailable: false,
    }),
    {
      authenticated: false,
      showApiKeyLogin: true,
      showApiKeyLogout: false,
      showChatgptLogin: true,
      showChatgptLogout: false,
    },
  );
});

test("openai auth row keeps api key logout while allowing chatgpt login when only api key auth exists", () => {
  assert.deepEqual(
    resolveOpenAIProviderRowState({
      providerId: "openai",
      providerAuthenticated: true,
      providerAuthSource: "api_key",
      selectedAuthSource: "api_key",
      chatgptAvailable: false,
    }),
    {
      authenticated: true,
      showApiKeyLogin: false,
      showApiKeyLogout: true,
      showChatgptLogin: true,
      showChatgptLogout: false,
    },
  );
});
