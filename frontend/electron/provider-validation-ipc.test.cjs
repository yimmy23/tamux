const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

const handlersSrc = fs.readFileSync(path.join(__dirname, "main/agent-ipc-handlers.cjs"), "utf8");

test("provider validation IPC allows slow remote provider checks", () => {
  assert.match(
    handlersSrc,
    /sendAgentQuery\(\{\s*type:\s*'validate-provider'[\s\S]*?\},\s*'provider-validation',\s*30000\)/
  );
});
