const test = require("node:test");
const assert = require("node:assert/strict");
const path = require("node:path");

const { createAgentDbBridgeRuntime } = require("./main/agent-db-bridge-runtime.cjs");
const {
  pendingHandlerMatchesResponseType,
  resolvePendingAgentQueryEvent,
} = require("./agent-query-runtime.cjs");

function createFakeBridgeProcess() {
  const stdoutListeners = new Map();
  const stderrListeners = new Map();
  const processListeners = new Map();
  const writes = [];

  const process = {
    killed: false,
    stdin: {
      writable: true,
      write(chunk) {
        writes.push(chunk);
        return true;
      },
    },
    stdout: {
      on(event, listener) {
        stdoutListeners.set(event, listener);
      },
    },
    stderr: {
      on(event, listener) {
        stderrListeners.set(event, listener);
      },
    },
    on(event, listener) {
      processListeners.set(event, listener);
    },
  };

  return {
    process,
    writes,
    emitStdout(payload) {
      stdoutListeners.get("data")?.(Buffer.from(payload, "utf8"));
    },
    emitStderr(payload) {
      stderrListeners.get("data")?.(Buffer.from(payload, "utf8"));
    },
    emitExit(code = 0) {
      process.killed = true;
      processListeners.get("exit")?.(code);
    },
  };
}

function createRuntimeHarness(options = {}) {
  const spawned = [];
  const sentEvents = [];
  const mainWindow = options.mainWindow ?? {
    isDestroyed: () => false,
    webContents: {
      send(channel, payload) {
        sentEvents.push({ channel, payload });
      },
    },
  };
  const runtime = createAgentDbBridgeRuntime({
    fs: { existsSync: () => true },
    getDaemonPath: () => path.join("/tmp", "zorai-daemon"),
    getMainWindow: () => mainWindow,
    logToFile: () => {},
    pendingHandlerMatchesResponseType,
    resolvePendingAgentQueryEvent,
    sendRenderedWhatsAppQr: async () => {},
    setWhatsAppSubscribed: () => {},
    shouldRestoreWhatsAppSubscription: () => false,
    spawn: () => {
      const fake = createFakeBridgeProcess();
      spawned.push(fake);
      return fake.process;
    },
  });

  return { runtime, spawned, sentEvents };
}

test("agent bridge rejects a concurrent query for the same response type", async () => {
  const { runtime, spawned } = createRuntimeHarness();

  const firstPromise = runtime.sendAgentQuery({ type: "get-thread", thread_id: "thread-1" }, "thread-detail", 5000);
  assert.equal(spawned.length, 1);

  await assert.rejects(
    runtime.sendAgentQuery({ type: "get-thread", thread_id: "thread-2" }, "thread-detail", 5000),
    /already pending/i,
  );

  spawned[0].emitStdout(`${JSON.stringify({ type: "thread-detail", data: { id: "thread-1" } })}\n`);
  await assert.doesNotReject(firstPromise);
  assert.equal(spawned[0].writes.length, 1);
});

test("agent bridge coalesces duplicate concurrent queries", async () => {
  const { runtime, spawned } = createRuntimeHarness();
  const command = { type: "list-workspace-tasks", workspace_id: "main", include_deleted: false };

  const firstPromise = runtime.sendAgentQuery(command, "workspace-task-list", 5000);
  const secondPromise = runtime.sendAgentQuery(command, "workspace-task-list", 5000);

  assert.equal(spawned.length, 1);
  assert.equal(spawned[0].writes.length, 1);

  spawned[0].emitStdout(`${JSON.stringify({ type: "workspace-task-list", data: [{ id: "task-1" }] })}\n`);

  assert.deepEqual(await firstPromise, [{ id: "task-1" }]);
  assert.deepEqual(await secondPromise, [{ id: "task-1" }]);
});

test("db bridge error rejects the oldest pending request with the bridge message", async () => {
  const { runtime, spawned } = createRuntimeHarness();

  const pendingQuery = runtime.sendDbQuery({ type: "list-agent-threads" }, "agent-thread-list", 5000);
  assert.equal(spawned.length, 1);

  spawned[0].emitStdout(`${JSON.stringify({ type: "error", message: "db exploded" })}\n`);

  await assert.rejects(pendingQuery, /db exploded/);
});

test("agent bridge forwards only actionable concierge welcome events to renderer", () => {
  const { runtime, spawned, sentEvents } = createRuntimeHarness();

  runtime.sendAgentCommand({ type: "subscribe" });
  assert.equal(spawned.length, 1);

  spawned[0].emitStdout(`${JSON.stringify({ type: "concierge_welcome", content: "Draft", actions: [] })}\n`);
  spawned[0].emitStdout(`${JSON.stringify({ type: "concierge_welcome", content: "Final", actions: [{ id: "start" }] })}\n`);

  assert.deepEqual(sentEvents, [
    {
      channel: "agent-event",
      payload: { type: "concierge_welcome", content: "Final", actions: [{ id: "start" }] },
    },
  ]);
});
