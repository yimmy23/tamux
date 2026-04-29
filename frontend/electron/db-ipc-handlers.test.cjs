const assert = require("node:assert/strict");
const test = require("node:test");

const { registerDbIpcHandlers } = require("./main/db-ipc-handlers.cjs");

function createHarness() {
  const handlers = new Map();
  const ackCommands = [];
  const queryCommands = [];
  let queryResult = null;
  const ipcMain = {
    handle(channel, handler) {
      handlers.set(channel, handler);
    },
  };

  registerDbIpcHandlers(ipcMain, {
    async sendDbAckCommand(command) {
      ackCommands.push(command);
    },
    async sendDbQuery(command) {
      queryCommands.push(command);
      return queryResult;
    },
  });

  return {
    ackCommands,
    handlers,
    queryCommands,
    setQueryResult(value) {
      queryResult = value;
    },
  };
}

test("db-upsert-transcript-index serializes renderer transcript entries for the daemon protocol", async () => {
  const { ackCommands, handlers } = createHarness();
  const handler = handlers.get("db-upsert-transcript-index");

  const ok = await handler({}, {
    id: "tx_1",
    filename: "2026-04-29/test.log",
    filePath: "transcripts/2026-04-29/test.log",
    reason: "manual",
    workspaceId: "workspace-1",
    surfaceId: "surface-1",
    paneId: "pane-1",
    capturedAt: 1_777_474_400_123,
    sizeBytes: 42,
    preview: "hello",
    content: "hello world",
  });

  assert.equal(ok, true);
  assert.equal(ackCommands.length, 1);
  assert.equal(ackCommands[0].type, "upsert-transcript-index");
  assert.deepEqual(JSON.parse(ackCommands[0].entry_json), {
    id: "tx_1",
    pane_id: "pane-1",
    workspace_id: "workspace-1",
    surface_id: "surface-1",
    filename: "2026-04-29/test.log",
    reason: "manual",
    captured_at: 1_777_474_400_123,
    size_bytes: 42,
    preview: "hello",
  });
});

test("db-list-transcript-index converts daemon transcript entries for renderer stores", async () => {
  const { handlers, queryCommands, setQueryResult } = createHarness();
  setQueryResult([{
    id: "tx_1",
    pane_id: "pane-1",
    workspace_id: "workspace-1",
    surface_id: "surface-1",
    filename: "2026-04-29/test.log",
    reason: "manual",
    captured_at: 1_777_474_400_123,
    size_bytes: 42,
    preview: "hello",
  }]);

  const handler = handlers.get("db-list-transcript-index");
  const entries = await handler({}, "workspace-1");

  assert.deepEqual(queryCommands, [{
    type: "list-transcript-index",
    workspace_id: "workspace-1",
  }]);
  assert.deepEqual(entries, [{
    id: "tx_1",
    filename: "2026-04-29/test.log",
    filePath: "transcripts/2026-04-29/test.log",
    reason: "manual",
    workspaceId: "workspace-1",
    surfaceId: "surface-1",
    paneId: "pane-1",
    cwd: null,
    capturedAt: 1_777_474_400_123,
    sizeBytes: 42,
    preview: "hello",
    content: "",
  }]);
});
