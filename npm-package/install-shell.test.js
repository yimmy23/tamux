"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const childProcess = require("node:child_process");
const os = require("node:os");
const path = require("node:path");

const install = require("./install");

test("shell installer dry-run targets GitHub release zip assets", { skip: process.platform === "win32" }, function () {
  const releaseInfo = install.getReleaseAssetInfo(os.platform(), os.arch(), "0.4.2");

  assert.ok(releaseInfo, "expected host platform to be supported by release asset mapping");

  const scriptPath = path.join(__dirname, "..", "scripts", "install.sh");
  const output = childProcess.execFileSync("sh", [scriptPath, "--dry-run"], {
    cwd: path.join(__dirname, ".."),
    env: {
      ...process.env,
      TAMUX_VERSION: "0.4.2",
    },
    encoding: "utf8",
  });

  const expectedUrl = `https://github.com/${install.GITHUB_OWNER}/${install.GITHUB_REPO}/releases/download/v0.4.2/${releaseInfo.archiveName}`;

  assert.match(output, new RegExp(expectedUrl.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")));
  assert.doesNotMatch(output, /gitlab\.com\/api\/v4\/projects/);
  assert.doesNotMatch(output, /tamux-binaries-/);
});

test("shell installer provisions bundled skills into canonical tamux root", function () {
  const scriptPath = path.join(__dirname, "..", "scripts", "install.sh");
  const script = childProcess.execFileSync("sed", ["-n", "1,260p", scriptPath], {
    cwd: path.join(__dirname, ".."),
    encoding: "utf8",
  });

  assert.match(script, /SKILLS_DIR="\$\{TAMUX_SKILLS_DIR:-\$HOME\/\.tamux\/skills\}"/);
  assert.match(script, /Extracting binaries and skills/);
  assert.match(script, /cp -R "\$EXTRACT_DIR\/skills\/\." "\$SKILLS_DIR\/"/);
});
