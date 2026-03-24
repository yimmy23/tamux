---
phase: 02
slug: core-heartbeat
status: draft
nyquist_compliant: true
wave_0_complete: true
created: 2026-03-23
---

# Phase 02 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | Rust built-in `#[test]` + `#[tokio::test]` |
| **Config file** | `crates/amux-daemon/Cargo.toml` |
| **Quick run command** | `cargo test -p tamux-daemon -- heartbeat --test-threads=1 -q` |
| **Full suite command** | `cargo test -p tamux-daemon --lib -- --test-threads=4 -q` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cargo test -p tamux-daemon -- heartbeat --test-threads=1 -q`
- **After every plan wave:** Run `cargo test -p tamux-daemon --lib -- --test-threads=4 -q`
- **Before `/gsd:verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | TDD | Status |
|---------|------|------|-------------|-----------|-------------------|-----|--------|
| 02-01-T1 | 01 | 1 | BEAT-01, BEAT-02, BEAT-04, BEAT-05 | unit | `cargo test -p tamux-daemon -- heartbeat --test-threads=1 -q` | yes | pending |
| 02-01-T2 | 01 | 1 | BEAT-01, BEAT-02 | unit | `cargo test -p tamux-daemon -- heartbeat_checks --test-threads=1 -q` | yes | pending |
| 02-02-T1 | 02 | 2 | BEAT-01, BEAT-03, BEAT-05 | unit | `cargo test -p tamux-daemon -- heartbeat::tests --test-threads=1 -q` | yes | pending |
| 02-02-T2 | 02 | 2 | BEAT-05 | unit | `cargo test -p tamux-daemon -- history --test-threads=1 -q` | no | pending |
| 02-03-T1 | 03 | 3 | BEAT-02, BEAT-03, BEAT-04, BEAT-08 | unit | `cargo test -p tamux-daemon -- heartbeat --test-threads=1 -q` | yes | pending |
| 02-03-T2 | 03 | 3 | BEAT-02, BEAT-03 | integration | `cargo test -p tamux-daemon --lib -- --test-threads=4 -q` | no | pending |

*Status: pending / green / red / flaky*

---

## Behavioral Test Coverage (Plan 03 Task 1, tdd=true)

These behavioral tests are defined in Plan 03 Task 1's `<behavior>` section and cover BEAT-03, BEAT-04, BEAT-08:

| Behavior | Requirement | Test Spec |
|----------|-------------|-----------|
| Silent default: no broadcast on quiet tick | BEAT-03, D-14 | When all checks return items_found=0 and LLM responds ACTIONABLE: false, no HeartbeatDigest event is broadcast |
| Digest event broadcast when actionable | BEAT-04, D-11 | When checks find items and LLM responds ACTIONABLE: true with ITEMS, HeartbeatDigest IS broadcast with parsed items |
| Persist on LLM failure | D-12, Pitfall 4 | When send_message returns Err, history is still persisted with status="synthesis_failed" |
| Check gating by enabled flag | BEAT-02 | Checks are skipped when their enabled flag is false in HeartbeatChecksConfig |
| Batched single LLM call | BEAT-08, D-09 | Exactly one send_message call per heartbeat cycle regardless of check count |
| Custom item due logic | D-03 | Custom HeartbeatItems only included when enabled and interval elapsed |

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Quiet hours suppress all notifications during configured window | BEAT-05 | Requires time-dependent system state | Set quiet_hours_start=0, quiet_hours_end=23, trigger heartbeat, verify no events emitted |
| DND toggle immediately suppresses notifications | BEAT-05 | Requires runtime config change | Enable DND via config, trigger heartbeat, verify no events emitted |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify commands
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Behavioral tests homed in Plan 03 Task 1 `<behavior>` section
- [x] No watch-mode flags
- [x] Feedback latency < 30s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
