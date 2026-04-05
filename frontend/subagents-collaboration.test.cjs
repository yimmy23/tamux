const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

const subagentsViewPath = path.join(__dirname, "src", "components", "agent-chat-panel", "SubagentsView.tsx");
const src = fs.readFileSync(subagentsViewPath, "utf8");

test("subagents view exposes collaboration session inspection affordance", () => {
  assert.match(src, /agentGetCollaborationSessions/);
  assert.match(src, /Inspect Collaboration/i);
  assert.match(src, /Collaboration Sessions/i);
  assert.match(src, /agentVoteOnCollaborationDisagreement/);
  assert.match(src, /Vote /i);
});