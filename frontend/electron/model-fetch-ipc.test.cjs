const assert = require("node:assert/strict");
const { test } = require("node:test");
const { registerAgentIpcHandlers } = require("./main/agent-ipc-handlers.cjs");

test("agent-fetch-models forwards output modality filters", async () => {
  const handlers = new Map();
  const ipcMain = {
    handle: (name, fn) => handlers.set(name, fn),
  };
  const calls = [];
  const sendAgentQuery = async (...args) => {
    calls.push(args);
    return { models: [] };
  };

  registerAgentIpcHandlers(
    ipcMain,
    { sendAgentCommand: () => {}, sendAgentQuery },
    {
      logToFile: () => {},
      openAICodexAuthHandlers: {},
      saveTempAudioCapture: async () => "",
    },
  );

  const handler = handlers.get("agent-fetch-models");
  assert.equal(typeof handler, "function");

  await handler({}, "openrouter", "https://openrouter.ai/api/v1", "router-key", "embedding");

  assert.deepEqual(calls[0], [
    {
      type: "fetch-models",
      provider_id: "openrouter",
      base_url: "https://openrouter.ai/api/v1",
      api_key: "router-key",
      output_modalities: "embedding",
    },
    "provider-models",
  ]);
});
