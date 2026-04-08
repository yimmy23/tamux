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
    if (args[1] === "prompt-inspection") {
      return {
        agent_id: args[0]?.agent_id || "swarog",
        agent_name: args[0]?.agent_id === "weles" ? "Weles" : "Svarog",
        provider_id: "openai",
        model: "gpt-5.4-mini",
        sections: [{ id: "base_prompt", title: "Base Prompt", content: "Prompt body" }],
        final_prompt: "Prompt body\n\n## Runtime Identity",
      };
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

test("preload exposes agent prompt inspection bridge method", () => {
  assert.match(
    preloadSrc,
    /agentInspectPrompt:\s*\(agentId\)\s*=>\s*ipcRenderer\.invoke\('agent-inspect-prompt', agentId\)/,
  );
});

test("runtime allowlists prompt inspection response type", () => {
  assert.ok(runtime.AGENT_QUERY_RESPONSE_TYPES.includes("prompt-inspection"));
});

test("agent IPC handlers query prompt inspection through the daemon bridge", async () => {
  const { handlers, queries } = createHandlerHarness();

  assert.ok(handlers.has("agent-inspect-prompt"));

  const mainPrompt = await handlers.get("agent-inspect-prompt")(null, null);
  const welesPrompt = await handlers.get("agent-inspect-prompt")(null, "weles");

  assert.equal(mainPrompt.agent_id, "swarog");
  assert.equal(welesPrompt.agent_id, "weles");
  assert.deepEqual(queries, [
    [{ type: "inspect-prompt", agent_id: null }, "prompt-inspection"],
    [{ type: "inspect-prompt", agent_id: "weles" }, "prompt-inspection"],
  ]);
});