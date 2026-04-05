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
    if (args[1] === "collaboration-sessions") {
      return [{ id: "session-1", parent_task_id: "task-1", disagreements: [{ topic: "deploy" }] }];
    }
    if (args[1] === "collaboration-vote-result") {
      return { session_id: "session-1", resolution: "resolved" };
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

test("preload exposes collaboration session bridge method", () => {
  assert.match(
    preloadSrc,
    /agentGetCollaborationSessions:\s*\(parentTaskId\)\s*=>\s*ipcRenderer\.invoke\('agent-get-collaboration-sessions', parentTaskId\)/,
  );
  assert.match(
    preloadSrc,
    /agentVoteOnCollaborationDisagreement:\s*\(parentTaskId, disagreementId, taskId, position, confidence\)\s*=>\s*ipcRenderer\.invoke\('agent-vote-on-collaboration-disagreement', parentTaskId, disagreementId, taskId, position, confidence\)/,
  );
});

test("runtime allowlists collaboration session query response type", () => {
  assert.ok(runtime.AGENT_QUERY_RESPONSE_TYPES.includes("collaboration-sessions"));
  assert.ok(runtime.AGENT_QUERY_RESPONSE_TYPES.includes("collaboration-vote-result"));
});

test("agent IPC handlers query collaboration sessions through the daemon bridge", async () => {
  const { handlers, queries } = createHandlerHarness();

  assert.ok(handlers.has("agent-get-collaboration-sessions"));
  assert.ok(handlers.has("agent-vote-on-collaboration-disagreement"));

  const sessions = await handlers.get("agent-get-collaboration-sessions")(null, "task-1");
  const vote = await handlers.get("agent-vote-on-collaboration-disagreement")(null, "task-1", "disagree-1", "operator", "recommend", 1.0);

  assert.deepEqual(sessions, [{ id: "session-1", parent_task_id: "task-1", disagreements: [{ topic: "deploy" }] }]);
  assert.deepEqual(vote, { session_id: "session-1", resolution: "resolved" });
  assert.deepEqual(queries, [
    [{ type: "get-collaboration-sessions", parent_task_id: "task-1" }, "collaboration-sessions"],
    [{ type: "vote-on-collaboration-disagreement", parent_task_id: "task-1", disagreement_id: "disagree-1", task_id: "operator", position: "recommend", confidence: 1.0 }, "collaboration-vote-result"],
  ]);
});