const test = require("node:test");
const assert = require("node:assert/strict");

const { registerAgentIpcHandlers } = require("./main/agent-ipc-handlers.cjs");

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

test("operator profile session start uses a startup-tolerant bridge timeout", async () => {
  const { handlers, queries } = createHandlerHarness();

  await handlers.get("agent-start-operator-profile-session")(null, "first_run_onboarding");

  assert.deepEqual(queries[0], [
    { type: "start-operator-profile-session", kind: "first_run_onboarding" },
    "operator-profile-session-started",
    30000,
  ]);
});

test("operator profile answer actions send commands without waiting on query timeouts", async () => {
  const handlers = new Map();
  const commands = [];
  const queries = [];
  const ipcMain = {
    handle(name, handler) {
      handlers.set(name, handler);
    },
  };

  registerAgentIpcHandlers(
    ipcMain,
    {
      sendAgentCommand: (command) => commands.push(command),
      sendAgentQuery: async (...args) => {
        queries.push(args);
        return { ok: true };
      },
    },
    {
      logToFile: () => {},
      openAICodexAuthHandlers: {
        status: async () => ({ available: false }),
        login: async () => ({ available: false }),
        logout: async () => ({ ok: true }),
      },
    },
  );

  await assert.doesNotReject(
    handlers.get("agent-submit-operator-profile-answer")(null, "s1", "q1", "true"),
  );
  await assert.doesNotReject(
    handlers.get("agent-skip-operator-profile-question")(null, "s1", "q1", "skip"),
  );
  await assert.doesNotReject(
    handlers.get("agent-defer-operator-profile-question")(null, "s1", "q1", 123),
  );

  assert.deepEqual(commands, [
    { type: "submit-operator-profile-answer", session_id: "s1", question_id: "q1", answer_json: "true" },
    { type: "skip-operator-profile-question", session_id: "s1", question_id: "q1", reason: "skip" },
    { type: "defer-operator-profile-question", session_id: "s1", question_id: "q1", defer_until_unix_ms: 123 },
  ]);
  assert.deepEqual(queries, []);
});
