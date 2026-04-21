const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

const agentIpcHandlersSrc = fs.readFileSync(
  path.join(__dirname, "main/agent-ipc-handlers.cjs"),
  "utf8",
);
const whatsappRuntimeSrc = fs.readFileSync(
  path.join(__dirname, "main/whatsapp-runtime.cjs"),
  "utf8",
);

test("gateway config IPC uses a dedicated daemon query", () => {
  assert.match(
    agentIpcHandlersSrc,
    /ipcMain\.handle\('gateway:get-config'[\s\S]*?sendAgentQuery\(\{\s*type:\s*'get-gateway-config'/,
  );
});

test("whatsapp runtime reuses dedicated gateway config queries", () => {
  assert.match(
    whatsappRuntimeSrc,
    /sendAgentQuery\(\{\s*type:\s*'get-gateway-config'\s*\},\s*'gateway-config'\)/,
  );
  assert.doesNotMatch(
    whatsappRuntimeSrc,
    /sendAgentQuery\(\{\s*type:\s*'get-config'\s*\},\s*'config'\)/,
  );
});
