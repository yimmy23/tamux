const test = require("node:test");
const assert = require("node:assert/strict");

const {
  resolveRendererLoadTarget,
} = require("./main/load-target.cjs");

test("development electron can explicitly load the built dist output", () => {
  const target = resolveRendererLoadTarget({
    app: { isPackaged: false },
    electronDir: "/repo/frontend/electron",
    env: { TAMUX_ELECTRON_USE_DIST_IN_DEV: "1" },
    path: require("node:path"),
  });

  assert.deepEqual(target, {
    kind: "file",
    value: "/repo/frontend/dist/index.html",
  });
});

test("development electron defaults to the Vite dev server", () => {
  const target = resolveRendererLoadTarget({
    app: { isPackaged: false },
    electronDir: "/repo/frontend/electron",
    env: {},
    path: require("node:path"),
  });

  assert.deepEqual(target, {
    kind: "url",
    value: "http://localhost:5173",
  });
});
