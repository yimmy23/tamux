---
phase: 9
slug: distribution
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-03-24
---

# Phase 9 -- Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust `#[test]` (daemon crates) + shell script integration tests |
| **Config file** | Cargo.toml workspace (existing) |
| **Quick run command** | `cargo test -p amux-cli --lib` |
| **Full suite command** | `cargo test --workspace` |
| **Estimated runtime** | ~45 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p amux-cli --lib`
- **After every plan wave:** Run `cargo test --workspace`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 60 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|-----------|-------------------|-------------|--------|
| 09-01-01 | 01 | 1 | DIST-01 | integration | `node npm-package/install.js --dry-run` | W0 | pending |
| 09-01-02 | 01 | 1 | DIST-01 | unit | `node -c npm-package/bin/tamux.js` | W0 | pending |
| 09-02-01 | 02 | 1 | DIST-02 | integration | `bash scripts/build-release.sh --skip-electron` | existing | pending |
| 09-03-01 | 03 | 1 | DIST-03 | integration | `bash scripts/install.sh --dry-run` | W0 | pending |
| 09-03-02 | 03 | 1 | DIST-04 | integration | `pwsh scripts/install.ps1 -DryRun` | W0 | pending |
| 09-04-01 | 04 | 1 | DIST-05 | unit | `cargo test -p tamux-cli -- setup_wizard` | W0 | pending |

*Status: pending / green / red / flaky*

---

## Wave 0 Requirements

- [ ] npm package scaffold with `postinstall.js` and `install.js` -- stubs for DIST-01
- [ ] `scripts/install.sh` with `--dry-run` flag -- stubs for DIST-03
- [ ] `scripts/install.ps1` with `-DryRun` flag -- stubs for DIST-04
- [ ] Setup wizard test stubs in amux-cli -- stubs for DIST-05

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| `npx tamux` downloads correct platform binary | DIST-01 | Requires npm registry publish + real download | Publish to npm, run `npx tamux` on fresh machine, verify binary matches OS/arch |
| Electron app launches after install | DIST-01 | Requires GUI environment | Run install, launch Electron app, verify window opens |
| GitLab CI pipeline succeeds on all runners | DIST-02 | Requires CI infrastructure | Push tag, monitor pipeline, verify all 4 platform jobs complete |
| First-run wizard completes end-to-end | DIST-05 | Requires fresh ~/.tamux/ state | Remove ~/.tamux/, run `tamux`, complete wizard, verify config.json written |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 60s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
