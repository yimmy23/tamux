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

function createRuntimeHarness() {
  const spawned = [];
  const runtime = createAgentDbBridgeRuntime({
    fs: { existsSync: () => true },
    getDaemonPath: () => path.join("/tmp", "tamux-daemon"),
    getMainWindow: () => null,
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

  return { runtime, spawned };
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

test("db bridge error rejects the oldest pending request with the bridge message", async () => {
  const { runtime, spawned } = createRuntimeHarness();

  const pendingQuery = runtime.sendDbQuery({ type: "list-agent-threads" }, "agent-thread-list", 5000);
  assert.equal(spawned.length, 1);

  spawned[0].emitStdout(`${JSON.stringify({ type: "error", message: "db exploded" })}\n`);

  await assert.rejects(pendingQuery, /db exploded/);
});