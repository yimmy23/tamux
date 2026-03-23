# Requirements: tamux — The Agent That Lives

**Defined:** 2026-03-23
**Core Value:** An agent that feels alive and gets smarter over time — while remaining simple enough that anyone can understand what it's doing and why.

## v1 Requirements

Requirements for the "Living Agent" milestone. Each maps to roadmap phases.

### Production Foundation

- [x] **FOUN-01**: Daemon SQLite uses WAL journal mode with `busy_timeout=5000` for concurrent read/write safety
- [x] **FOUN-02**: HistoryStore uses shared async connection via `tokio-rusqlite` 0.6.0 instead of open-per-call pattern
- [x] **FOUN-03**: Single `HistoryStore` instance shared via `Arc` across SessionManager, AgentEngine, and SnapshotManager
- [x] **FOUN-04**: Circuit breaker (`circuit_breaker.rs`) wired into `send_completion_request()` LLM call path per-provider
- [x] **FOUN-05**: Broadcast channel capacity configurable per-session with sensible defaults (1024+ for PTY, 512+ for agent events)
- [x] **FOUN-06**: WAL mode pragmas applied on first connection: `journal_mode=WAL`, `synchronous=NORMAL`, `wal_autocheckpoint=1000`

### Proactive Heartbeat

- [x] **BEAT-01**: Daemon runs a configurable heartbeat loop using `croner` cron expressions (default: every 15 minutes)
- [x] **BEAT-02**: Heartbeat checks: stale TODOs, stuck goal runs, unreplied gateway messages, repo changes
- [x] **BEAT-03**: Heartbeat is silent by default — only surfaces items with concrete actionable suggestions
- [x] **BEAT-04**: Heartbeat results delivered via existing `AgentEvent` broadcast to all connected clients
- [x] **BEAT-05**: User can configure heartbeat schedule, enable/disable checks, and set quiet hours in agent config
- [ ] **BEAT-06**: Operator-aware scheduling: agent learns when user is active and surfaces proactive items during active windows
- [ ] **BEAT-07**: Adaptive heartbeat: agent learns what to check based on operator model patterns (what user cares about, what goes wrong)
- [ ] **BEAT-08**: Heartbeat batches multiple checks into a single LLM call to manage token cost
- [ ] **BEAT-09**: Heartbeat integrates M2 Anticipatory Runtime outputs (morning brief, stuck hints, predictive hydration)

### Memory Consolidation

- [ ] **MEMO-01**: During idle heartbeat ticks, agent reviews recent execution traces and consolidates learnings into MEMORY.md
- [ ] **MEMO-02**: Memory facts have confidence scores that decay exponentially over time (configurable half-life, default ~69 hours)
- [ ] **MEMO-03**: Consolidation is append-only with tombstones — never deletes, only marks facts as superseded
- [ ] **MEMO-04**: All consolidation actions logged to provenance system with full audit trail
- [ ] **MEMO-05**: 7-day rollback window: any consolidation can be reversed within 7 days
- [ ] **MEMO-06**: Successful tool sequences automatically promoted into learned heuristics during consolidation
- [ ] **MEMO-07**: Idle detection uses composite signal: no active tasks + no active goals + no active streams + operator inactive
- [ ] **MEMO-08**: Proactive memory refinement: reorganize and compress memory blocks for higher signal density during idle time
- [ ] **MEMO-09**: Cross-session context continuity: threads resume seamlessly after daemon restart with full context

### Skill Ecosystem

- [ ] **SKIL-01**: Agent automatically drafts a candidate skill when it solves a novel problem (high tool count, multiple replans, eventual success)
- [ ] **SKIL-02**: Novelty detection: agent recognizes when a solution approach hasn't been seen before
- [ ] **SKIL-03**: Skills follow maturity lifecycle: Draft → Testing → Active → Proven → Canonical
- [ ] **SKIL-04**: Draft skills are validated against a few mental test scenarios (LLM evaluation) before promotion to Testing
- [ ] **SKIL-05**: Skill promotion requires N successful real-world uses (configurable threshold, default: 3)
- [ ] **SKIL-06**: New and promoted skills surfaced to user naturally in conversation via concierge
- [ ] **SKIL-07**: Community skill registry: operators can publish skills with provenance metadata
- [ ] **SKIL-08**: Imported community skills reset to Draft maturity and must prove themselves locally before promotion
- [ ] **SKIL-09**: Community skill security: mandatory static analysis scan before import
- [ ] **SKIL-10**: Skill format compatible with agentskills.io standard (SKILL.md + metadata frontmatter)

### Transparent Autonomy

- [ ] **TRNS-01**: Every autonomous heartbeat action includes a simple explanation: "I did X because Y"
- [ ] **TRNS-02**: Causal trace summaries available on demand: user can ask "why did you do that?" and get a clear answer
- [ ] **TRNS-03**: Action audit feed: chronological list of all autonomous actions accessible from any client
- [ ] **TRNS-04**: Confidence signals: agent communicates its certainty level on non-trivial decisions
- [ ] **TRNS-05**: Escalation visibility: when agent escalates (L0→L1→L2→L3), user sees the progression and reasoning

### Progressive UX

- [ ] **PRUX-01**: Capability tiers driven by operator model: Newcomer → Familiar → Power User → Expert
- [ ] **PRUX-02**: Tier transitions announced naturally via concierge ("You've been using goal runs — here's what else they can do")
- [ ] **PRUX-03**: New users see simplified interface with core features; advanced capabilities revealed as usage grows
- [ ] **PRUX-04**: Concierge onboarding: guided first experience that explains what the agent can do, with hands-on examples
- [ ] **PRUX-05**: Typed `getBridge()` helper replaces all 39 `(window as any).tamux ?? (window as any).amux` casts in frontend
- [ ] **PRUX-06**: Consistent status visibility across TUI, Electron, and CLI: user always knows what agent is doing and why

### Distribution

- [ ] **DIST-01**: Single-command install via npm: `npx tamux` downloads and runs appropriate platform binary
- [ ] **DIST-02**: Standalone platform binaries via cargo-dist (Linux, macOS, Windows)
- [ ] **DIST-03**: Shell installer: `curl -fsSL https://tamux.dev/install.sh | sh` for Unix systems
- [ ] **DIST-04**: PowerShell installer for Windows
- [ ] **DIST-05**: First-run automatically spawns daemon, opens default client, and starts concierge onboarding

### Gateway Completion

- [ ] **GATE-01**: Daemon-side Slack gateway: real `conversations.history` polling + `chat.postMessage` via reqwest (not stubs)
- [ ] **GATE-02**: Daemon-side Discord gateway: real REST API message receive/send via reqwest (not stubs)
- [ ] **GATE-03**: Daemon-side Telegram gateway: real `getUpdates` long-polling + `sendMessage` via reqwest (not stubs)
- [ ] **GATE-04**: Unified gateway config: single configuration surface for both daemon and Electron gateway paths
- [ ] **GATE-05**: Gateway health monitoring: detect disconnections, auto-reconnect, surface status to user
- [ ] **GATE-06**: Gateway messages routed through heartbeat: unreplied messages surfaced as proactive items

## v2 Requirements

Deferred to future milestone. Tracked but not in current roadmap.

### Advanced Security

- **SECR-01**: WASM skill sandboxing via wasmtime for untrusted community skill execution
- **SECR-02**: Skill code signing with provenance chain linked to WORM audit trail
- **SECR-03**: Per-skill capability grants (filesystem, network, shell access controls)
- **SECR-04**: API key storage migration from plaintext config.json to OS keyring

### Remote Execution

- **REMX-01**: Serverless daemon execution on Daytona/Modal (agent runs in cloud, operator connects remotely)
- **REMX-02**: Multi-device sync: daemon state replicated across operator's machines

### Advanced Learning

- **LERN-01**: Cross-installation skill telemetry: anonymized success rates shared across community
- **LERN-02**: Skill A/B testing with statistical significance thresholds
- **LERN-03**: Automated skill merge: converge similar skill variants into optimized canonical versions

### Expanded Gateway

- **GTWX-01**: WhatsApp daemon-side gateway (currently Electron-only via Baileys)
- **GTWX-02**: Slack Events API with Socket Mode (replace polling)
- **GTWX-03**: Gateway webhook support for incoming messages (not just polling)

### Electron Modernization

- **ELEC-01**: Modularize `electron/main.cjs` (4,500 lines) into per-integration modules
- **ELEC-02**: Automated tests for Electron main process logic
- **ELEC-03**: Agent-side workspace tool gating (disable workspace tools when no Electron client connected)

## Out of Scope

| Feature | Reason |
|---------|--------|
| Cloud-hosted SaaS | Local-first is the architectural moat; cloud hosting contradicts core value proposition |
| Training/fine-tuning LLMs | tamux orchestrates models, doesn't train them; not our domain |
| Mobile app | Desktop-first; mobile access via gateway messaging platforms (WhatsApp, Telegram) |
| Multi-user/multi-tenant | Single-operator desktop tool; multi-user auth complexity not justified |
| Browser extension / IDE plugin | Standalone runtime, not a plugin; MCP server provides IDE integration |
| Real-time collaborative editing | Agent collaborates with one operator, not multiple humans |
| Vector database for memory | Research confirms SQLite FTS5 is sufficient; vector DB adds operational complexity for marginal gain |
| Automatic rusqlite upgrade (0.32→0.38) | tokio-rusqlite 0.6.0 buys compatibility; full upgrade is a separate future effort |
| WASM skill execution this milestone | Significant scope; local-only skills don't need sandboxing; defer until community registry proves demand |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| FOUN-01 | Phase 1 | Complete |
| FOUN-02 | Phase 1 | Complete |
| FOUN-03 | Phase 1 | Complete |
| FOUN-04 | Phase 1 | Complete |
| FOUN-05 | Phase 1 | Complete |
| FOUN-06 | Phase 1 | Complete |
| BEAT-01 | Phase 2 | Complete |
| BEAT-02 | Phase 2 | Complete |
| BEAT-03 | Phase 2 | Complete |
| BEAT-04 | Phase 2 | Complete |
| BEAT-05 | Phase 2 | Complete |
| BEAT-06 | Phase 4 | Pending |
| BEAT-07 | Phase 4 | Pending |
| BEAT-08 | Phase 2 | Pending |
| BEAT-09 | Phase 4 | Pending |
| MEMO-01 | Phase 5 | Pending |
| MEMO-02 | Phase 5 | Pending |
| MEMO-03 | Phase 5 | Pending |
| MEMO-04 | Phase 5 | Pending |
| MEMO-05 | Phase 5 | Pending |
| MEMO-06 | Phase 5 | Pending |
| MEMO-07 | Phase 5 | Pending |
| MEMO-08 | Phase 5 | Pending |
| MEMO-09 | Phase 5 | Pending |
| SKIL-01 | Phase 6 | Pending |
| SKIL-02 | Phase 6 | Pending |
| SKIL-03 | Phase 6 | Pending |
| SKIL-04 | Phase 6 | Pending |
| SKIL-05 | Phase 6 | Pending |
| SKIL-06 | Phase 6 | Pending |
| SKIL-07 | Phase 7 | Pending |
| SKIL-08 | Phase 7 | Pending |
| SKIL-09 | Phase 7 | Pending |
| SKIL-10 | Phase 7 | Pending |
| TRNS-01 | Phase 3 | Pending |
| TRNS-02 | Phase 3 | Pending |
| TRNS-03 | Phase 3 | Pending |
| TRNS-04 | Phase 3 | Pending |
| TRNS-05 | Phase 3 | Pending |
| PRUX-01 | Phase 10 | Pending |
| PRUX-02 | Phase 10 | Pending |
| PRUX-03 | Phase 10 | Pending |
| PRUX-04 | Phase 10 | Pending |
| PRUX-05 | Phase 10 | Pending |
| PRUX-06 | Phase 10 | Pending |
| DIST-01 | Phase 9 | Pending |
| DIST-02 | Phase 9 | Pending |
| DIST-03 | Phase 9 | Pending |
| DIST-04 | Phase 9 | Pending |
| DIST-05 | Phase 9 | Pending |
| GATE-01 | Phase 8 | Pending |
| GATE-02 | Phase 8 | Pending |
| GATE-03 | Phase 8 | Pending |
| GATE-04 | Phase 8 | Pending |
| GATE-05 | Phase 8 | Pending |
| GATE-06 | Phase 8 | Pending |

**Coverage:**
- v1 requirements: 56 total
- Mapped to phases: 56
- Unmapped: 0

---
*Requirements defined: 2026-03-23*
*Last updated: 2026-03-23 after roadmap creation*
