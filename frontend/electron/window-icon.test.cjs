const test = require("node:test");
const assert = require("node:assert/strict");
const path = require("node:path");

const { resolveWindowIcon } = require("./main/window-runtime.cjs");

test("Linux windows use the PNG app icon", () => {
    assert.equal(
        resolveWindowIcon({ electronDir: "/repo/frontend/electron", path, platform: "linux" }),
        "/repo/frontend/assets/icon.png",
    );
});

test("Windows windows use the ICO app icon", () => {
    assert.equal(
        resolveWindowIcon({ electronDir: "/repo/frontend/electron", path, platform: "win32" }),
        "/repo/frontend/assets/icon.ico",
    );
});
