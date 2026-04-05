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
    if (args[1] === "audit-list") {
      return [{ id: 1, action_type: "tool" }];
    }
    if (args[1] === "provenance-report") {
      return { total_entries: 3, valid_hash_entries: 3, valid_signature_entries: 2 };
    }
    if (args[1] === "memory-provenance-report") {
      return { total_entries: 4, summary_by_status: { active: 3, uncertain: 1 } };
    }
    if (args[1] === "memory-provenance-confirmed") {
      return { entry_id: "old-confirmable", confirmed_at: 123 };
    }
    if (args[1] === "memory-provenance-retracted") {
      return { entry_id: "retractable-memory-entry", retracted_at: 456 };
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

test("preload exposes audit history and provenance bridge methods", () => {
  assert.match(
    preloadSrc,
    /agentQueryAudits:\s*\(actionTypes, since, limit\)\s*=>\s*ipcRenderer\.invoke\('agent-query-audits', actionTypes, since, limit\)/,
  );
  assert.match(
    preloadSrc,
    /agentGetProvenanceReport:\s*\(limit\)\s*=>\s*ipcRenderer\.invoke\('agent-get-provenance-report', limit\)/,
  );
  assert.match(
    preloadSrc,
    /agentGetMemoryProvenanceReport:\s*\(target, limit\)\s*=>\s*ipcRenderer\.invoke\('agent-get-memory-provenance-report', target, limit\)/,
  );
  assert.match(
    preloadSrc,
    /agentConfirmMemoryProvenanceEntry:\s*\(entryId\)\s*=>\s*ipcRenderer\.invoke\('agent-confirm-memory-provenance-entry', entryId\)/,
  );
  assert.match(
    preloadSrc,
    /agentRetractMemoryProvenanceEntry:\s*\(entryId\)\s*=>\s*ipcRenderer\.invoke\('agent-retract-memory-provenance-entry', entryId\)/,
  );
});

test("runtime allowlists audit and provenance query response types", () => {
  assert.ok(runtime.AGENT_QUERY_RESPONSE_TYPES.includes("audit-list"));
  assert.ok(runtime.AGENT_QUERY_RESPONSE_TYPES.includes("provenance-report"));
  assert.ok(runtime.AGENT_QUERY_RESPONSE_TYPES.includes("memory-provenance-report"));
  assert.ok(runtime.AGENT_QUERY_RESPONSE_TYPES.includes("memory-provenance-confirmed"));
  assert.ok(runtime.AGENT_QUERY_RESPONSE_TYPES.includes("memory-provenance-retracted"));
});

test("agent IPC handlers query audit history and provenance through the daemon bridge", async () => {
  const { handlers, queries } = createHandlerHarness();

  assert.ok(handlers.has("agent-query-audits"));
  assert.ok(handlers.has("agent-get-provenance-report"));
  assert.ok(handlers.has("agent-get-memory-provenance-report"));
  assert.ok(handlers.has("agent-confirm-memory-provenance-entry"));
  assert.ok(handlers.has("agent-retract-memory-provenance-entry"));

  const audits = await handlers.get("agent-query-audits")(null, ["tool"], 1000, 25);
  const provenance = await handlers.get("agent-get-provenance-report")(null, 10);
  const memory = await handlers.get("agent-get-memory-provenance-report")(null, "MEMORY.md", 12);
  const confirmed = await handlers.get("agent-confirm-memory-provenance-entry")(null, "old-confirmable");
  const retracted = await handlers.get("agent-retract-memory-provenance-entry")(null, "retractable-memory-entry");

  assert.deepEqual(audits, [{ id: 1, action_type: "tool" }]);
  assert.deepEqual(provenance, {
    total_entries: 3,
    valid_hash_entries: 3,
    valid_signature_entries: 2,
  });
  assert.deepEqual(memory, {
    total_entries: 4,
    summary_by_status: { active: 3, uncertain: 1 },
  });
  assert.deepEqual(confirmed, { entry_id: "old-confirmable", confirmed_at: 123 });
  assert.deepEqual(retracted, { entry_id: "retractable-memory-entry", retracted_at: 456 });
  assert.deepEqual(queries, [
    [{ type: "query-audits", action_types: ["tool"], since: 1000, limit: 25 }, "audit-list"],
    [{ type: "get-provenance-report", limit: 10 }, "provenance-report"],
    [{ type: "get-memory-provenance-report", target: "MEMORY.md", limit: 12 }, "memory-provenance-report"],
    [{ type: "confirm-memory-provenance-entry", entry_id: "old-confirmable" }, "memory-provenance-confirmed"],
    [{ type: "retract-memory-provenance-entry", entry_id: "retractable-memory-entry" }, "memory-provenance-retracted"],
  ]);
});