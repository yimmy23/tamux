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
  assert.doesNotMatch(workflow, /run: npm ci && npm run build/);
  assert.match(workflow, /max_attempts=3/);
  assert.match(workflow, /npm ci --prefer-offline --no-audit/);
  assert.match(workflow, /cp "\$LINUX_APPIMAGE" dist-release\/linux\/zorai-desktop/);
  assert.match(workflow, /zip -r "\$ZIP" zorai-daemon zorai zoi zorai-tui zorai-mcp zorai-gateway zorai-desktop skills guidelines/);
  assert.match(workflow, /cp -R guidelines dist-release\/linux-arm64\//);
  assert.match(workflow, /Electron package \(Linux arm64 AppImage\)/);
  assert.match(workflow, /dist-release\/linux-arm64\/zorai-desktop/);
  assert.match(workflow, /zip -r "\$ZIP" zorai-daemon zorai zoi zorai-tui zorai-mcp zorai-gateway zorai-desktop skills guidelines/);
  assert.match(workflow, /npm config set fetch-retries 5/);
  assert.match(workflow, /\$maxAttempts = 3/);
  assert.match(workflow, /npm ci --prefer-offline --no-audit/);
  assert.match(workflow, /Copy-Item -Recurse guidelines dist-release\/windows\//);
  assert.match(workflow, /Copy-Item frontend\/release\/zorai-portable\.exe dist-release\/windows\/zorai-desktop\.exe/);
  assert.match(workflow, /Compress-Archive -Path zorai-daemon\.exe, zorai\.exe, zoi\.exe, zorai-tui\.exe, zorai-mcp\.exe, zorai-gateway\.exe, zorai-desktop\.exe, skills, guidelines/);
  assert.match(workflow, /build-windows-arm64:/);
  assert.match(workflow, /zorai-windows-arm64\.zip/);
  assert.match(workflow, /SHA256SUMS-windows-arm64\.txt/);
  assert.match(workflow, /cp -R guidelines dist-release\/macos\//);
  assert.match(workflow, /zorai-desktop\.app\.zip/);
  assert.match(workflow, /find frontend\/release -maxdepth 2 -type d -name "zorai\.app"/);
  assert.doesNotMatch(workflow, /cp -R frontend\/release\/mac\/zorai\.app/);
  assert.match(workflow, /zip -r "\$ZIP" zorai-daemon zorai zoi zorai-tui zorai-mcp zorai-gateway zorai-desktop zorai-desktop\.app\.zip skills guidelines/);
});
