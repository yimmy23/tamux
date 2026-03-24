# tamux — The Agent That Lives

## What This Is

tamux is a daemon-first, self-orchestrating AI agent runtime that lives on your machine, remembers everything it learns, ships work while you sleep, and gets smarter every day. It's a local desktop application (Electron + TUI + CLI) powered by a Rust daemon that owns all state — threads, tasks, goal runs, memory, telemetry, and terminal sessions. Multiple clients (desktop GUI, terminal UI, CLI, MCP server, chat gateways) connect to the same daemon, so long-running work survives UI disconnects and can be reattached from any surface.

tamux is not a chatbot wrapper. It is the most architecturally deep open-source agent runtime in existence — 4-layer self-orchestration, genetic skill evolution, sub-agent management, crash-recoverable goal runs, WORM audit trails, and an operator model that learns how you work. The next milestone is about making that depth *felt* — turning infrastructure into experience.

## Core Value

**An agent that feels alive and gets smarter over time — while remaining simple enough that anyone can understand what it's doing and why.**

Depth without clarity is wasted engineering. Every capability must surface as a simple, understandable experience. If the user can't feel it or explain it, it doesn't count.

## Requirements

### Validated

These capabilities exist in the codebase today and are relied upon:

- ✓ Daemon-first architecture: single source of truth, survives UI disconnects — existing
- ✓ Multi-client connectivity: Electron GUI, TUI, CLI, MCP server, chat gateway — existing
- ✓ Agent runtime with threaded conversations and streaming LLM inference — existing
- ✓ Tool execution with 15+ tool families (terminal, file, search, memory, skill, task, goal) — existing
- ✓ Background task queue with priority, dependencies, retry policy, approval waiting — existing
- ✓ Goal runs: decompose objectives into step plans, dispatch as tasks, replan on failure — existing
- ✓ Sub-agent spawning with tool filtering, context budgets, termination DSL — existing
- ✓ 4-layer self-orchestration: Execution → Liveness → Meta-Cognition → Learning — existing
- ✓ Health monitoring with hysteresis, stuck detection (5 patterns), crash recovery — existing
- ✓ 4-level escalation pathway: Self-Correction → Sub-Agent → User → External — existing
- ✓ Checkpointing: pre-step/post-step snapshots, restore from any checkpoint — existing
- ✓ Layered persistent memory: SOUL.md, MEMORY.md, USER.md with size limits — existing
- ✓ Pre-compaction memory flush: last chance to preserve facts before context compression — existing
- ✓ Episodic recall: session search, OneContext, Honcho cross-session memory — existing
- ✓ Skills system: create, load, generate from trajectories, variant metadata — existing
- ✓ Genetic skill evolution: branching, A/B testing, promotion, deprecation, merge — existing
- ✓ Operator model (M1): learns output density preference, risk behavior, session rhythm — existing
- ✓ Anticipatory runtime (M2): morning brief, predictive hydration, stuck-work hints — existing
- ✓ Causal traces (M3): records why decisions were made, which options failed — existing
- ✓ Semantic environment model (M5): crate/package inspection, dependency graphs, conventions — existing
- ✓ Deep storage with provenance (M6): memory provenance, confidence aging, contradiction checks — existing
- ✓ Collaboration protocol (M7): sub-agent voting, disagreement records, shared state — existing
- ✓ Trusted provenance (M8): hash-linked WORM audit ledgers — existing
- ✓ Implicit feedback learning (M9): learns from denials, corrections, attention patterns — existing
- ✓ Runtime tool synthesis (M10): synthesize guarded tools from CLI/OpenAPI surfaces — existing
- ✓ Execution traces with pattern mining and heuristic learning — existing
- ✓ Tool composition graph: DependsOn, SynergizesWith, ConflictsWith relationships — existing
- ✓ Context self-management: audit, compress, archive (FTS5), restore — existing
- ✓ Multi-provider LLM support: 20+ providers, OpenAI + Anthropic wire formats — existing
- ✓ Terminal multiplexing with PTY session lifecycle management — existing
- ✓ Gateway integrations: Slack, Discord, Telegram, WhatsApp (Electron-side working) — existing
- ✓ Safety model: policy enforcement, approval flow, sandbox, rate limiting, command validation — existing
- ✓ Telemetry: operational, cognitive, contextual, provenance — existing
- ✓ SQLite persistence + file-based storage (intentional split) — existing
- ✓ Binary-framed IPC protocol shared across all Rust crates — existing
- ✓ 376 unit tests across self-orchestrating agent modules — existing

### Active

These are what we're building next — the "living agent" milestone:

**Alive: Proactive Heartbeat & Anticipatory Surface**
- [x] Configurable heartbeat loop that wakes the agent at intervals to check work, repos, messages — Validated in Phase 2: Core Heartbeat
- [x] Heartbeat-driven TODO monitoring: detect stale tasks, stuck goal runs, unreplied gateway messages — Validated in Phase 2: Core Heartbeat
- [x] Wire M2 Anticipatory Runtime outputs (morning brief, stuck hints, hydration) into heartbeat cycle — Validated in Phase 4: Adaptive Heartbeat
- [x] Operator-aware scheduling: learn when the user is active, surface proactive items at the right time — Validated in Phase 4: Adaptive Heartbeat
- [x] Heartbeat learns what to check based on operator model patterns (not a static checklist) — Validated in Phase 4: Adaptive Heartbeat

**Remembers: Memory Consolidation & Idle Learning**
- [x] Memory consolidation during idle heartbeat ticks: review traces, update MEMORY.md, decay stale facts — Validated in Phase 5: Memory Consolidation
- [x] Automatic promotion of successful tool sequences into learned heuristics during idle time — Validated in Phase 5: Memory Consolidation
- [x] Proactive memory refinement: reorganize and compress memory blocks for higher signal density — Validated in Phase 5: Memory Consolidation
- [x] Cross-session context continuity: seamless thread resumption after daemon restart — Validated in Phase 5: Memory Consolidation (gap closure 05-05)

**Grows: Automatic Skill Discovery & Ecosystem**
- [x] Automatic skill drafting: when agent solves a novel problem (high complexity, eventual success), draft a candidate skill — Validated in Phase 6: Skill Discovery
- [x] Skill maturity lifecycle: Draft → Testing → Active → Proven → Canonical — Validated in Phase 6: Skill Discovery
- [x] Skill discovery feed: surface new/promoted skills to the user naturally in conversation — Validated in Phase 6: Skill Discovery
- [ ] Community skill registry: publish, subscribe, discover skills with provenance metadata

**Understands: Living Project Model**
- [ ] Continuous project understanding: not a static codebase map but a living model that evolves with changes
- [ ] Intent inference: when user says "fix the auth bug," agent should know which one from context + traces + operator model
- [ ] Contextual tool selection: use learned heuristics to pick optimal tools for task type without exploring

**Ships: Production Hardening**
- [x] Wire circuit breaker into LLM call path — Validated in Phase 1: Production Foundation
- [x] Enable WAL mode for SQLite, share HistoryStore via Arc — Validated in Phase 1: Production Foundation
- [x] Complete daemon-side gateway implementations (currently stubs in amux-gateway crate) — Validated in Phase 8: Gateway Completion
- [x] Increase broadcast channel capacity for high-throughput PTY sessions — Validated in Phase 1: Production Foundation

**Feels Simple: User Experience**
- [ ] Zero-friction first run: single binary or `npx` install that handles daemon + client setup
- [ ] Concierge onboarding: guided first experience that explains what the agent can do
- [x] Clear status visibility: user always knows what the agent is doing, why, and what it learned — Validated in Phase 3: Transparent Autonomy
- [x] Transparent autonomy: every autonomous action has a simple explanation surface ("I did X because Y") — Validated in Phase 3: Transparent Autonomy
- [ ] Graceful complexity disclosure: start simple, reveal depth as the user grows with the agent
- [ ] Unified bridge helper: replace 39 `(window as any).tamux` casts with typed `getBridge()` accessor

### Out of Scope

- Cloud-hosted SaaS deployment — tamux is local-first by design, not a hosted service
- Training or fine-tuning LLMs — tamux orchestrates existing models, it doesn't train them
- Mobile app — desktop-first (Electron + TUI); mobile access via gateway messaging platforms
- Multi-user/multi-tenant — single-operator desktop tool; auth complexity not justified
- Real-time collaborative editing — the agent collaborates with the operator, not multiple humans simultaneously
- Browser extension — not an IDE plugin or browser copilot; it's a standalone runtime

## Context

### Competitive Landscape (March 2026)

**OpenClaw** (214K GitHub stars) — the most viral agent project ever. Wins on simplicity: TypeScript, messaging-first UX, 20+ channel gateway, heartbeat cron for proactivity. Architecture is shallow (single ReAct loop, no sub-agents, no learning, no goal decomposition). The lesson: perceived aliveness beats actual sophistication. Their heartbeat is trivially simple but it makes the agent *feel* alive.

**Hermes Agent** (Nous Research) — Python, "the agent that grows with you." Dual memory (local + Honcho), skill documents from successful tasks, multi-channel gateway. No sub-agents, no goal decomposition, no checkpointing, no learning from traces. The lesson: the narrative of self-improvement resonates deeply, even if the mechanism is basic.

**Devin** (Cognition) — the most autonomous coding agent. Compound AI (model swarm), multi-agent dispatch, fork/rollback, machine snapshots. Closed-source, SaaS, expensive. Deep execution but no transparency, no local ownership, no proactivity.

**Claude Code / Anthropic Agent SDK** — excellent single-session coding agent. No persistent memory, no learning, no proactivity, fresh context every session.

### tamux's Position

tamux has 10x the engineering depth of any open-source competitor. The self-orchestrating architecture (4 layers, 56 files, 376 tests) is unmatched. But depth that users can't feel is invisible. The next milestone bridges the gap between infrastructure and experience.

### Key Insight

OpenClaw got 214K stars with a simple heartbeat cron and WhatsApp integration. tamux has genetic skill evolution, WORM audit trails, and a 4-level escalation pathway. The winner won't be the deepest architecture or the simplest one — it'll be the one that makes depth feel effortless.

### Technical Foundation

- **Languages:** Rust (daemon, CLI, TUI, protocol, gateway, MCP) + TypeScript/React (Electron frontend)
- **Runtime:** Tokio async, Electron 33.x, SQLite (rusqlite bundled)
- **IPC:** Binary-framed protocol over Unix socket / TCP / named pipe
- **State:** Daemon-owned, persisted to SQLite + markdown/JSON files in `~/.tamux/`
- **Codebase:** 6 Rust crates in a Cargo workspace + Electron frontend in `frontend/`

## Constraints

- **Tech stack**: Rust daemon + TypeScript/React frontend — no language changes, Rust performance and safety are core to the daemon-first architecture
- **Local-first**: All data stays on the operator's machine — no phone-home, no cloud dependency, no account required
- **Provider-agnostic**: Must work with any OpenAI-compatible or Anthropic-compatible LLM provider — no vendor lock-in
- **Backward compatibility**: Existing `~/.tamux/` data directory, config format, and IPC protocol must not break for current users
- **Single binary aspiration**: Reducing install friction is critical — aiming for single-binary or near-single-binary distribution
- **Platform parity**: Linux, macOS, Windows all first-class (Electron handles GUI; Rust handles daemon/CLI/TUI)

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| Daemon-first architecture | Work survives UI disconnects, enables multi-client access, state owned centrally | ✓ Good — core architectural advantage over all competitors |
| Rust for daemon + protocol | Performance, safety, cross-platform native binaries, no runtime dependency | ✓ Good — enables local-first with zero infrastructure |
| 4-layer self-orchestration | Separates execution from monitoring from reasoning from learning | ✓ Good — unique differentiator, 376 tests prove it works |
| Genetic skill evolution | Skills improve over time through variant branching and A/B testing | — Pending — infrastructure solid, needs real-world validation |
| Operator model learning | Agent adapts to user's style, risk tolerance, session rhythm | — Pending — feature gated, needs more operator exposure |
| Heartbeat-driven proactivity | Agent wakes periodically to check work, not just respond to input | — Pending — concept exists in M2, needs full wiring |
| SQLite for structured data + files for editable artifacts | Queryable truth (threads, tasks, traces) vs. human-editable (memory, skills) | ✓ Good — each storage type optimized for its access pattern |
| Multi-provider LLM support | No vendor lock-in, operator chooses their model and provider | ✓ Good — 20+ providers supported |
| Simple-first UX philosophy | Depth must be felt, not explained; complexity disclosed progressively | — Pending — this is the core challenge of the next milestone |

## Evolution

This document evolves at phase transitions and milestone boundaries.

**After each phase transition** (via `/gsd:transition`):
1. Requirements invalidated? → Move to Out of Scope with reason
2. Requirements validated? → Move to Validated with phase reference
3. New requirements emerged? → Add to Active
4. Decisions to log? → Add to Key Decisions
5. "What This Is" still accurate? → Update if drifted

**After each milestone** (via `/gsd:complete-milestone`):
1. Full review of all sections
2. Core Value check — still the right priority?
3. Audit Out of Scope — reasons still valid?
4. Update Context with current state

---
*Last updated: 2026-03-24 after Phase 9 (Distribution) completion*
