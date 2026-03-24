---
phase: 10
slug: progressive-ux
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-24
---

# Phase 10 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust `#[test]` (daemon/protocol crates) + manual UI verification |
| **Config file** | Cargo.toml workspace (existing) |
| **Quick run command** | `cargo test -p amux-daemon -- tier` |
| **Full suite command** | `cargo test --workspace` |
| **Estimated runtime** | ~45 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p amux-daemon -- tier`
- **After every plan wave:** Run `cargo test --workspace`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 60 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 10-01-01 | 01 | 1 | PRUX-01 | unit | `cargo test -p amux-daemon -- tier` | W0 | pending |
| 10-01-02 | 01 | 1 | PRUX-01 | unit | `cargo test -p amux-protocol` | existing | pending |
| 10-02-01 | 02 | 1 | PRUX-05 | grep | `grep -r "(window as any)" frontend/src/ --include="*.tsx" --include="*.ts" \| grep -v ".d.ts"` | existing | pending |
| 10-02-02 | 02 | 1 | PRUX-05 | tsc | `cd frontend && npx tsc --noEmit` | existing | pending |
| 10-03-01 | 03 | 2 | PRUX-02, PRUX-04 | build | `cargo build --lib -p amux-daemon && cargo build --lib -p amux-tui` | existing | pending |
| 10-04-01 | 04 | 2 | PRUX-03 | tsc+unit | `cd frontend && npx tsc --noEmit && cd .. && cargo test -p amux-tui -- tier` | W0/existing | pending |
| 10-04-02 | 04 | 2 | PRUX-03 | manual | Visual tier gating verification | N/A | pending |
| 10-05-01 | 05 | 2 | PRUX-06 | build+grep | `cargo build --workspace && grep -c agentGetStatus frontend/electron/preload.cjs` | existing | pending |
| 10-05-02 | 05 | 2 | PRUX-06 | manual | Status consistency across clients | N/A | pending |

*Status: pending / green / red / flaky*

---

## Wave 0 Requirements

- [ ] Tier resolution tests in daemon — stubs for PRUX-01
- [ ] TUI tier state tests — stubs for PRUX-03

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Collapsed sections hide/show by tier | PRUX-03 | Requires UI rendering | Set tier to Newcomer, verify all 5 advanced sections are collapsed in TUI sidebar and Electron SettingsPanel |
| Concierge onboarding guided goal run | PRUX-04 | Requires fresh state + LLM | Remove config, run setup wizard, verify concierge offers tier-adapted walkthrough |
| Concierge action buttons work | PRUX-04 | Requires UI interaction | Trigger onboarding, click each action button (focus_chat, start_goal_run, open_settings, dismiss_welcome) |
| Status consistency across clients | PRUX-06 | Requires multiple clients | Open TUI + Electron simultaneously, verify same status info shown |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 60s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
