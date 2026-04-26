"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

test("powershell installer uses GitHub release zip assets and full binary set", function () {
  const scriptPath = path.join(__dirname, "..", "scripts", "install.ps1");
  const script = fs.readFileSync(scriptPath, "utf8");

  assert.match(script, /\$GitHubOwner\s*=\s*"mkurman"/);
  assert.match(script, /\$GitHubRepo\s*=\s*"tamux"/);
  assert.match(script, /api\.github\.com\/repos\/\$GitHubOwner\/\$GitHubRepo/);
  assert.match(script, /github\.com\/\$GitHubOwner\/\$GitHubRepo\/releases\/download/);
  assert.match(script, /tamux-windows-\$script:ArchName\.zip/);
  assert.match(script, /SHA256SUMS-windows-\$script:ArchName\.txt/);
  assert.match(script, /tamux-gateway\.exe/);
  assert.match(script, /tamux-mcp\.exe/);
  assert.doesNotMatch(script, /gitlab\.com\/api\/v4\/projects/);
  assert.doesNotMatch(script, /tamux-binaries-/);
});

test("powershell installer provisions bundled skills into canonical tamux root", function () {
  const scriptPath = path.join(__dirname, "..", "scripts", "install.ps1");
  const script = fs.readFileSync(scriptPath, "utf8");

  assert.match(script, /\$SkillsDir = if \(\$env:TAMUX_SKILLS_DIR\) \{ \$env:TAMUX_SKILLS_DIR \} else \{ Join-Path \$HOME "\.tamux\\skills" \}/);
  assert.match(script, /\$GuidelinesDir = if \(\$env:TAMUX_GUIDELINES_DIR\) \{ \$env:TAMUX_GUIDELINES_DIR \} else \{ Join-Path \$HOME "\.tamux\\guidelines" \}/);
  assert.match(script, /Extracting binaries, skills, and guidelines/);
  assert.match(script, /Copy-Item -Path \(Join-Path \$script:ExtractDir "skills\\\*"\) -Destination \$SkillsDir -Recurse -Force/);
  assert.match(script, /function Install-Guidelines/);
  assert.match(script, /if \(Test-Path \$targetPath\) \{/);
});
