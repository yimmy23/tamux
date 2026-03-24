---
status: partial
phase: 09-distribution
source: [09-VERIFICATION.md]
started: 2026-03-24
updated: 2026-03-24
---

## Current Test

[awaiting human testing]

## Tests

### 1. npx tamux end-to-end install
expected: Binary downloads for the correct platform, wizard collects provider/key/client pref, daemon starts in background, TUI opens
result: [pending]

### 2. Unix install script (curl | sh)
expected: Platform detected, binaries downloaded with SHA256 verification, installed to ~/.local/bin, shell profiles updated for PATH
result: [pending]

### 3. Windows PowerShell install script
expected: Binaries downloaded with SHA256 verification, installed to C:\Program Files\tamux, system PATH updated
result: [pending]

### 4. GitLab CI pipeline on tag push
expected: build:linux-x64 and build:windows-x64 run automatically; build:linux-arm64 runs; build:darwin-arm64 is manual; release:create uploads SHA256SUMS and creates GitLab Release; release:npm-publish is manual
result: [pending]

## Summary

total: 4
passed: 0
issues: 0
pending: 4
skipped: 0
blocked: 0

## Gaps
