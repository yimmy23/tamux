const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

const panelPath = path.join(__dirname, "src", "components", "SessionVaultPanel.tsx");
const contentPath = path.join(__dirname, "src", "components", "session-vault-panel", "SessionVaultContent.tsx");
const memoryViewPath = path.join(__dirname, "src", "components", "session-vault-panel", "SessionVaultMemoryView.tsx");

const panelSrc = fs.readFileSync(panelPath, "utf8");
const contentSrc = fs.readFileSync(contentPath, "utf8");
const memoryViewSrc = fs.readFileSync(memoryViewPath, "utf8");

test("session vault panel fetches memory provenance when memory mode is active", () => {
  assert.match(panelSrc, /agentGetMemoryProvenanceReport|loadMemoryProvenance/);
  assert.match(panelSrc, /agentConfirmMemoryProvenanceEntry|confirmMemoryEntry/);
  assert.match(panelSrc, /agentRetractMemoryProvenanceEntry|retractMemoryEntry/);
});

test("session vault content exposes a memory provenance mode with uncertain entries", () => {
  assert.match(contentSrc, /"memory"/);
  assert.match(contentSrc, /Memory Provenance/i);
  assert.match(contentSrc, /uncertain/i);
  assert.match(contentSrc, /Confirm/i);
  assert.match(contentSrc, /confirmMemoryEntry/);
  assert.match(contentSrc, /Retract/i);
  assert.match(contentSrc, /retractMemoryEntry/);
});

test("session vault memory view renders persisted provenance relationships", () => {
  assert.match(memoryViewSrc, /relationship/i);
  assert.match(memoryViewSrc, /relationship\.relation_type/);
  assert.match(memoryViewSrc, /entry\.relationships/);
});