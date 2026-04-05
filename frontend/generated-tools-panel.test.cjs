const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

const panelPath = path.join(__dirname, "src", "components", "generated-tools", "GeneratedToolsPanel.tsx");
const agentTabPath = path.join(__dirname, "src", "components", "settings-panel", "AgentTab.tsx");

const panelSrc = fs.readFileSync(panelPath, "utf8");
const agentTabSrc = fs.readFileSync(agentTabPath, "utf8");

test("generated tools panel exposes operator actions", () => {
  assert.match(panelSrc, /Generated Tools/i);
  assert.match(panelSrc, /agentListGeneratedTools/);
  assert.match(panelSrc, /agentRunGeneratedTool/);
  assert.match(panelSrc, /agentActivateGeneratedTool/);
  assert.match(panelSrc, /agentPromoteGeneratedTool/);
  assert.match(panelSrc, /agentRetireGeneratedTool/);
  assert.match(panelSrc, /Refresh Tools/i);
  assert.match(panelSrc, /Run Tool/i);
  assert.match(panelSrc, /Activate/i);
  assert.match(panelSrc, /Promote/i);
  assert.match(panelSrc, /Retire/i);
});

test("agent settings tab renders generated tools panel when synthesis is enabled", () => {
  assert.match(agentTabSrc, /GeneratedToolsPanel/);
});
