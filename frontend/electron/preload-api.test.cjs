const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

const preloadSrc = fs.readFileSync(path.join(__dirname, "preload.cjs"), "utf8");

test("preload exposes the zorai bridge exactly once", () => {
  const matches = preloadSrc.match(/contextBridge\.exposeInMainWorld\('zorai',\s*bridgeApi\)/g) ?? [];

  assert.equal(matches.length, 1);
});
