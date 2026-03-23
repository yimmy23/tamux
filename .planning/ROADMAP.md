# Roadmap: tamux — The Agent That Lives

## Overview

This milestone bridges tamux's 10x engineering depth to user-felt experience. Starting with production hardening (SQLite WAL, circuit breaker) to handle concurrency, we build the heartbeat that makes the agent feel alive, add transparency so autonomous actions are trustworthy, then layer memory consolidation and skill evolution that make the agent grow smarter over time. Gateway completion and distribution run in parallel. Progressive UX ships last, after all features stabilize, to create a coherent experience that reveals depth as the user grows.

## Phases

**Phase Numbering:**
- Integer phases (1, 2, 3): Planned milestone work
- Decimal phases (2.1, 2.2): Urgent insertions (marked with INSERTED)

Decimal phases appear between their surrounding integers in numeric order.

- [ ] **Phase 1: Production Foundation** - SQLite WAL mode, shared connection pool, circuit breaker wiring
- [ ] **Phase 2: Core Heartbeat** - Configurable heartbeat loop with cron scheduling, check framework, and client delivery
- [ ] **Phase 3: Transparent Autonomy** - Every autonomous action explained, auditable, and escalation-visible
- [ ] **Phase 4: Adaptive Heartbeat** - Operator-aware scheduling, learned check priorities, M2 anticipatory integration
- [ ] **Phase 5: Memory Consolidation** - Idle-time learning: trace review, fact aging, heuristic promotion, cross-session continuity
- [ ] **Phase 6: Skill Discovery** - Automatic skill drafting, maturity lifecycle, and user-facing skill feed
- [ ] **Phase 7: Community Skills** - Skill registry, import with security scanning, agentskills.io compatibility
- [ ] **Phase 8: Gateway Completion** - Daemon-side Slack, Discord, Telegram with unified config and health monitoring
- [ ] **Phase 9: Distribution** - Single-command install via npm, cargo-dist binaries, platform installers
- [ ] **Phase 10: Progressive UX** - Capability tiers, concierge onboarding, typed bridge helper, consistent status

## Phase Details

### Phase 1: Production Foundation
**Goal**: Daemon handles concurrent workloads (heartbeat + agent + consolidation) without database contention or unrecoverable LLM failures
**Depends on**: Nothing (first phase)
**Requirements**: FOUN-01, FOUN-02, FOUN-03, FOUN-04, FOUN-05, FOUN-06
**Success Criteria** (what must be TRUE):
  1. Multiple concurrent daemon subsystems (agent, snapshot, session) read and write SQLite without "database is locked" errors
  2. A single HistoryStore instance is shared across all daemon components — no duplicate connection opens
  3. When an LLM provider is down or rate-limited, the circuit breaker trips and requests fail fast with a clear error instead of hanging
  4. High-throughput PTY sessions stream without dropped events under sustained output
**Plans:** 3 plans

Plans:
- [x] 01-01-PLAN.md — HistoryStore async migration to tokio-rusqlite 0.6.0 with WAL pragmas
- [ ] 01-02-PLAN.md — Shared Arc<HistoryStore> wiring + configurable broadcast channel capacity
- [ ] 01-03-PLAN.md — Circuit breaker wiring into LLM call path with per-provider instances

### Phase 2: Core Heartbeat
**Goal**: The agent wakes on a schedule, checks for actionable work, and delivers findings to connected clients — making it feel alive
**Depends on**: Phase 1
**Requirements**: BEAT-01, BEAT-02, BEAT-03, BEAT-04, BEAT-05, BEAT-08
**Success Criteria** (what must be TRUE):
  1. Agent wakes at configurable intervals (cron expression) and checks stale TODOs, stuck goals, unreplied messages, and repo changes
  2. Heartbeat is silent when nothing is actionable — user is never interrupted with noise
  3. When the heartbeat finds something, all connected clients (TUI, Electron, CLI) receive the notification
  4. User can configure schedule, toggle individual checks, and set quiet hours from agent config
  5. Multiple heartbeat checks are batched into a single LLM call to keep token cost low
**Plans**: TBD

### Phase 3: Transparent Autonomy
**Goal**: Users trust the agent's autonomous actions because every action is explained, auditable, and escalation reasoning is visible
**Depends on**: Phase 2
**Requirements**: TRNS-01, TRNS-02, TRNS-03, TRNS-04, TRNS-05
**Success Criteria** (what must be TRUE):
  1. Every autonomous heartbeat action includes a plain-language explanation ("I did X because Y") visible in all clients
  2. User can ask "why did you do that?" about any past action and get a clear causal trace summary
  3. A chronological action audit feed is accessible from TUI, Electron, and CLI
  4. Agent communicates confidence level on non-trivial decisions (e.g., "I'm 80% sure this is a stale TODO")
  5. When the agent escalates through levels (self-correction to sub-agent to user to external), the user sees each step and reasoning
**Plans**: TBD
**UI hint**: yes

### Phase 4: Adaptive Heartbeat
**Goal**: The heartbeat learns the operator's rhythm and priorities, becoming a proactive assistant that surfaces the right things at the right time
**Depends on**: Phase 3
**Requirements**: BEAT-06, BEAT-07, BEAT-09
**Success Criteria** (what must be TRUE):
  1. Agent learns when the user is active and surfaces proactive items during those windows, staying quiet during off-hours
  2. Heartbeat checks evolve based on what the user actually cares about — frequently dismissed checks get deprioritized
  3. M2 Anticipatory Runtime outputs (morning brief, stuck hints, predictive hydration) are delivered through the heartbeat cycle
**Plans**: TBD

### Phase 5: Memory Consolidation
**Goal**: The agent gets smarter during idle time — reviewing traces, aging stale facts, promoting heuristics, and maintaining continuity across restarts
**Depends on**: Phase 4
**Requirements**: MEMO-01, MEMO-02, MEMO-03, MEMO-04, MEMO-05, MEMO-06, MEMO-07, MEMO-08, MEMO-09
**Success Criteria** (what must be TRUE):
  1. During idle periods (no active tasks, goals, streams, or operator activity), agent reviews recent traces and updates MEMORY.md with consolidated learnings
  2. Memory facts decay over time with configurable half-life — stale facts lose prominence while recent facts dominate
  3. All consolidation is append-only with tombstones and a full provenance audit trail; any consolidation can be rolled back within 7 days
  4. Successful tool sequences are automatically promoted into learned heuristics that improve future task execution
  5. After daemon restart, threads resume seamlessly with full context — no "starting fresh" experience
**Plans**: TBD

### Phase 6: Skill Discovery
**Goal**: The agent recognizes when it solved something novel and automatically creates skills that improve through real-world use
**Depends on**: Phase 5
**Requirements**: SKIL-01, SKIL-02, SKIL-03, SKIL-04, SKIL-05, SKIL-06
**Success Criteria** (what must be TRUE):
  1. When the agent solves a problem through high complexity (many tools, replans, eventual success), it automatically drafts a candidate skill
  2. Agent distinguishes novel solution approaches from known patterns — only genuinely new approaches trigger skill drafting
  3. Skills follow a visible maturity lifecycle (Draft to Testing to Active to Proven to Canonical) with real-world use counts driving promotion
  4. New and promoted skills are surfaced to the user naturally in conversation via concierge messaging
**Plans**: TBD

### Phase 7: Community Skills
**Goal**: Operators can share skills with provenance and import community skills safely
**Depends on**: Phase 6
**Requirements**: SKIL-07, SKIL-08, SKIL-09, SKIL-10
**Success Criteria** (what must be TRUE):
  1. Operator can publish a proven skill to the community registry with full provenance metadata
  2. Imported community skills reset to Draft maturity and must prove themselves locally before promotion
  3. Every imported skill passes a mandatory static analysis security scan before it can execute
  4. Skill format is compatible with the agentskills.io standard (SKILL.md + metadata frontmatter)
**Plans**: TBD

### Phase 8: Gateway Completion
**Goal**: The agent communicates through Slack, Discord, and Telegram natively from the daemon, with unified config and health monitoring
**Depends on**: Phase 1
**Requirements**: GATE-01, GATE-02, GATE-03, GATE-04, GATE-05, GATE-06
**Success Criteria** (what must be TRUE):
  1. Daemon-side Slack gateway polls conversations and sends messages via real API calls (not stubs)
  2. Daemon-side Discord gateway receives and sends messages via real REST API calls (not stubs)
  3. Daemon-side Telegram gateway uses long-polling for incoming messages and sends replies via real API (not stubs)
  4. All gateway platforms share a single unified configuration surface for both daemon and Electron paths
  5. Gateway disconnections are detected, auto-reconnected, and status surfaced to the user
**Plans**: TBD
**UI hint**: yes

### Phase 9: Distribution
**Goal**: Anyone can install tamux with a single command on any platform and be running within seconds
**Depends on**: Phase 1
**Requirements**: DIST-01, DIST-02, DIST-03, DIST-04, DIST-05
**Success Criteria** (what must be TRUE):
  1. `npx tamux` downloads and runs the correct platform binary on Linux, macOS, and Windows
  2. Standalone binaries are available via cargo-dist for all three platforms
  3. Unix users can install via `curl -fsSL https://tamux.dev/install.sh | sh`
  4. Windows users can install via a PowerShell installer script
  5. First run automatically spawns the daemon, opens the default client, and starts concierge onboarding
**Plans**: TBD

### Phase 10: Progressive UX
**Goal**: The interface reveals depth as the user grows — newcomers see simplicity, power users see the full system
**Depends on**: Phases 1-9 (needs stable feature set)
**Requirements**: PRUX-01, PRUX-02, PRUX-03, PRUX-04, PRUX-05, PRUX-06
**Success Criteria** (what must be TRUE):
  1. Operator model drives capability tiers (Newcomer, Familiar, Power User, Expert) that control which features are visible
  2. Tier transitions are announced naturally via concierge ("You've been using goal runs — here's what else they can do")
  3. New users see a simplified interface; advanced features appear only as usage grows
  4. First-time users experience a guided concierge onboarding with hands-on examples
  5. Status visibility is consistent across TUI, Electron, and CLI — user always knows what the agent is doing and why
**Plans**: TBD
**UI hint**: yes

## Progress

**Execution Order:**
Phases execute in numeric order: 1 > 2 > 3 > 4 > 5 > 6 > 7 > 8 > 9 > 10
Note: Phases 8 and 9 can execute in parallel with phases 2-7 (they only depend on Phase 1).

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 1. Production Foundation | 0/3 | Planning complete | - |
| 2. Core Heartbeat | 0/? | Not started | - |
| 3. Transparent Autonomy | 0/? | Not started | - |
| 4. Adaptive Heartbeat | 0/? | Not started | - |
| 5. Memory Consolidation | 0/? | Not started | - |
| 6. Skill Discovery | 0/? | Not started | - |
| 7. Community Skills | 0/? | Not started | - |
| 8. Gateway Completion | 0/? | Not started | - |
| 9. Distribution | 0/? | Not started | - |
| 10. Progressive UX | 0/? | Not started | - |
