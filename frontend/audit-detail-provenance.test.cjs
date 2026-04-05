const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

const auditStorePath = path.join(__dirname, "src", "lib", "auditStore.ts");
const detailViewPath = path.join(__dirname, "src", "components", "audit-panel", "AuditDetailView.tsx");

const auditStoreSrc = fs.readFileSync(auditStorePath, "utf8");
const detailViewSrc = fs.readFileSync(detailViewPath, "utf8");

test("audit store preserves detailed provenance entries for UI matching", () => {
  assert.match(auditStoreSrc, /provenanceReport:\s*\{[\s\S]*entries:/);
  assert.match(auditStoreSrc, /function findMatchingProvenanceEntry\(/);
});

test("audit detail view renders provenance verification details", () => {
  assert.match(detailViewSrc, /useAuditStore/);
  assert.match(detailViewSrc, /ProvenanceIndicator/);
  assert.match(detailViewSrc, /Integrity verification/i);
  assert.match(detailViewSrc, /Compliance mode/i);
});