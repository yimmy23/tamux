"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

test("GitHub release workflow bundles guidelines in every native zip", function () {
  const workflow = fs.readFileSync(
    path.join(__dirname, "..", ".github", "workflows", "release.yml"),
    "utf8"
  );

  assert.match(workflow, /cp -R guidelines dist-release\/linux\//);
  assert.match(workflow, /zip -r "\$ZIP" tamux-daemon tamux tamux-tui tamux-mcp tamux-gateway skills guidelines/);
  assert.match(workflow, /cp -R guidelines dist-release\/linux-arm64\//);
  assert.match(workflow, /Copy-Item -Recurse guidelines dist-release\/windows\//);
  assert.match(workflow, /Compress-Archive -Path tamux-daemon\.exe, tamux\.exe, tamux-tui\.exe, tamux-mcp\.exe, tamux-gateway\.exe, skills, guidelines/);
  assert.match(workflow, /cp -R guidelines dist-release\/macos\//);
  assert.match(workflow, /zip -r "\$ZIP" tamux-daemon tamux tamux-tui tamux-mcp tamux-gateway skills guidelines/);
});
