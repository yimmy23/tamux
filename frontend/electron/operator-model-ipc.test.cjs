const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

const preloadPath = path.join(__dirname, "preload.cjs");
const runtimePath = path.join(__dirname, "agent-query-runtime.cjs");
const handlerPath = path.join(__dirname, "main", "agent-ipc-handlers.cjs");

const preloadSrc = fs.readFileSync(preloadPath, "utf8");
const runtime = require(runtimePath);
const { registerAgentIpcHandlers } = require(handlerPath);

function createHandlerHarness() {
  const handlers = new Map();
  const queries = [];
  const ipcMain = {
    handle(name, handler) {
      handlers.set(name, handler);
    },
  };
  const sendAgentQuery = async (...args) => {
    queries.push(args);
    if (args[1] === "operator-model") {
      return { version: "1.0", session_count: 2 };
    }
    if (args[1] === "operator-model-reset") {
      return { ok: true };
    }
    return { ok: true };
  };

  registerAgentIpcHandlers(
    ipcMain,
    { sendAgentCommand: () => {}, sendAgentQuery },
    {
      logToFile: () => {},
      openAICodexAuthHandlers: {
        status: async () => ({ available: false }),
        login: async () => ({ available: false }),
        logout: async () => ({ ok: true }),
      },
    },
  );

  return { handlers, queries };
}

test("preload exposes operator model bridge methods", () => {
  assert.match(
    preloadSrc,
    /agentGetOperatorModel:\s*\(\)\s*=>\s*ipcRenderer\.invoke\('agent-get-operator-model'\)/,
  );
  assert.match(
    preloadSrc,
    /agentResetOperatorModel:\s*\(\)\s*=>\s*ipcRenderer\.invoke\('agent-reset-operator-model'\)/,
  );
});

test("runtime allowlists operator model query response types", () => {
  assert.ok(runtime.AGENT_QUERY_RESPONSE_TYPES.includes("operator-model"));
  assert.ok(runtime.AGENT_QUERY_RESPONSE_TYPES.includes("operator-model-reset"));
});

test("agent IPC handlers query and reset operator model through the daemon bridge", async () => {
  const { handlers, queries } = createHandlerHarness();

  assert.ok(handlers.has("agent-get-operator-model"));
  assert.ok(handlers.has("agent-reset-operator-model"));

  const model = await handlers.get("agent-get-operator-model")();
  const reset = await handlers.get("agent-reset-operator-model")();

  assert.deepEqual(model, { version: "1.0", session_count: 2 });
  assert.deepEqual(reset, { ok: true });
  assert.deepEqual(queries, [
    [{ type: "get-operator-model" }, "operator-model"],
    [{ type: "reset-operator-model" }, "operator-model-reset"],
  ]);
});