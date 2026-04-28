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
  assert.match(workflow, /zip -r "\$ZIP" zorai-daemon zorai zoi zorai-tui zorai-mcp zorai-gateway skills guidelines/);
  assert.match(workflow, /cp -R guidelines dist-release\/linux-arm64\//);
  assert.match(workflow, /Copy-Item -Recurse guidelines dist-release\/windows\//);
  assert.match(workflow, /Compress-Archive -Path zorai-daemon\.exe, zorai\.exe, zoi\.exe, zorai-tui\.exe, zorai-mcp\.exe, zorai-gateway\.exe, skills, guidelines/);
  assert.match(workflow, /cp -R guidelines dist-release\/macos\//);
  assert.match(workflow, /zip -r "\$ZIP" zorai-daemon zorai zoi zorai-tui zorai-mcp zorai-gateway skills guidelines/);
});
