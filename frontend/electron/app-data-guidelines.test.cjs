"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");

const { installBundledGuidelines } = require("./main/app-data.cjs");

test("installBundledGuidelines copies missing files without overwriting user guidelines", function () {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), "tamux-electron-guidelines-"));
    const source = path.join(root, "resources", "guidelines");
    const targetRoot = path.join(root, "tamux");
    const target = path.join(targetRoot, "guidelines");

    fs.mkdirSync(source, { recursive: true });
    fs.mkdirSync(target, { recursive: true });
    fs.writeFileSync(path.join(source, "coding-task.md"), "# bundled coding\n");
    fs.writeFileSync(path.join(source, "research-task.md"), "# bundled research\n");
    fs.writeFileSync(path.join(target, "coding-task.md"), "# user coding\n");

    const result = installBundledGuidelines({
        sourceCandidates: [path.join(root, "missing"), source],
        targetRoot,
    });

    assert.deepEqual(result, { copied: 1, skipped: 1, source });
    assert.equal(fs.readFileSync(path.join(target, "coding-task.md"), "utf8"), "# user coding\n");
    assert.equal(fs.readFileSync(path.join(target, "research-task.md"), "utf8"), "# bundled research\n");
});
