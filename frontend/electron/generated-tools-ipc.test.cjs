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
    if (args[1] === "generated-tools") {
      return [{ id: "tool-1", name: "tool-1", status: "new" }];
    }
    if (args[1] === "generated-tool-result") {
      return {
        tool_name: "tool-1",
        result: { status: args[0]?.type === "retire-generated-tool" ? "archived" : "active" },
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

test("preload exposes generated tool bridge methods", () => {
  assert.match(
    preloadSrc,
    /agentListGeneratedTools:\s*\(\)\s*=>\s*ipcRenderer\.invoke\('agent-list-generated-tools'\)/,
  );
  assert.match(
    preloadSrc,
    /agentRunGeneratedTool:\s*\(toolName, argsJson\)\s*=>\s*ipcRenderer\.invoke\('agent-run-generated-tool', toolName, argsJson\)/,
  );
  assert.match(
    preloadSrc,
    /agentActivateGeneratedTool:\s*\(toolName\)\s*=>\s*ipcRenderer\.invoke\('agent-activate-generated-tool', toolName\)/,
  );
  assert.match(
    preloadSrc,
    /agentPromoteGeneratedTool:\s*\(toolName\)\s*=>\s*ipcRenderer\.invoke\('agent-promote-generated-tool', toolName\)/,
  );
  assert.match(
    preloadSrc,
    /agentRetireGeneratedTool:\s*\(toolName\)\s*=>\s*ipcRenderer\.invoke\('agent-retire-generated-tool', toolName\)/,
  );
});

test("runtime allowlists generated tool query response types", () => {
  assert.ok(runtime.AGENT_QUERY_RESPONSE_TYPES.includes("generated-tools"));
  assert.ok(runtime.AGENT_QUERY_RESPONSE_TYPES.includes("generated-tool-result"));
});

test("agent IPC handlers query generated tools through the daemon bridge", async () => {
  const { handlers, queries } = createHandlerHarness();

  assert.ok(handlers.has("agent-list-generated-tools"));
  assert.ok(handlers.has("agent-run-generated-tool"));
  assert.ok(handlers.has("agent-activate-generated-tool"));
  assert.ok(handlers.has("agent-promote-generated-tool"));
  assert.ok(handlers.has("agent-retire-generated-tool"));

  const tools = await handlers.get("agent-list-generated-tools")();
  const runResult = await handlers.get("agent-run-generated-tool")(null, "tool-1", "{}");
  const activateResult = await handlers.get("agent-activate-generated-tool")(null, "tool-1");
  const promoteResult = await handlers.get("agent-promote-generated-tool")(null, "tool-1");
  const retireResult = await handlers.get("agent-retire-generated-tool")(null, "tool-1");

  assert.deepEqual(tools, [{ id: "tool-1", name: "tool-1", status: "new" }]);
  assert.deepEqual(runResult, { tool_name: "tool-1", result: { status: "active" } });
  assert.deepEqual(activateResult, { tool_name: "tool-1", result: { status: "active" } });
  assert.deepEqual(promoteResult, { tool_name: "tool-1", result: { status: "active" } });
  assert.deepEqual(retireResult, { tool_name: "tool-1", result: { status: "archived" } });
  assert.deepEqual(queries, [
    [{ type: "list-generated-tools" }, "generated-tools"],
    [{ type: "run-generated-tool", tool_name: "tool-1", args_json: "{}" }, "generated-tool-result"],
    [{ type: "activate-generated-tool", tool_name: "tool-1" }, "generated-tool-result"],
    [{ type: "promote-generated-tool", tool_name: "tool-1" }, "generated-tool-result"],
    [{ type: "retire-generated-tool", tool_name: "tool-1" }, "generated-tool-result"],
  ]);
});
