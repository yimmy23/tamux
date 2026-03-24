# tamux — The Agent That Lives

## What This Is

tamux is a daemon-first, self-orchestrating AI agent runtime that lives on your machine, remembers everything it learns, ships work while you sleep, and gets smarter every day. It's a local desktop application (Electron + TUI + CLI) powered by a Rust daemon that owns all state — threads, tasks, goal runs, memory, telemetry, and terminal sessions. Multiple clients (desktop GUI, terminal UI, CLI, MCP server, chat gateways) connect to the same daemon, so long-running work survives UI disconnects and can be reattached from any surface.

tamux is not a chatbot wrapper. It is the most architecturally deep open-source agent runtime in existence — 4-layer self-orchestration, genetic skill evolution, sub-agent management, crash-recoverable goal runs, WORM audit trails, and an operator model that learns how you work. The next milestone makes tamux community-friendly — a declarative plugin ecosystem where anyone can extend the agent with JSON manifests, installable via CLI or npm.

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
- ✓ Configurable heartbeat loop with cron scheduling, check framework, client delivery — v1.0 Phase 2
- ✓ Heartbeat-driven TODO monitoring: stale tasks, stuck goal runs, unreplied messages — v1.0 Phase 2
- ✓ M2 Anticipatory Runtime wired into heartbeat cycle (morning brief, stuck hints) — v1.0 Phase 4
- ✓ Operator-aware scheduling: learned activity patterns, proactive timing — v1.0 Phase 4
- ✓ Heartbeat learns check priorities from operator model patterns — v1.0 Phase 4
- ✓ Memory consolidation during idle: trace review, MEMORY.md update, fact decay — v1.0 Phase 5
- ✓ Automatic heuristic promotion from successful tool sequences — v1.0 Phase 5
- ✓ Proactive memory refinement: reorganize and compress for signal density — v1.0 Phase 5
- ✓ Cross-session context continuity: seamless thread resumption after restart — v1.0 Phase 5
- ✓ Automatic skill drafting from novel problem-solving trajectories — v1.0 Phase 6
- ✓ Skill maturity lifecycle: Draft → Testing → Active → Proven → Canonical — v1.0 Phase 6
- ✓ Skill discovery feed: surface new/promoted skills in conversation — v1.0 Phase 6
- ✓ Circuit breaker wired into LLM call path — v1.0 Phase 1
- ✓ SQLite WAL mode, shared HistoryStore via Arc — v1.0 Phase 1
- ✓ Daemon-side gateway: Slack, Discord, Telegram — v1.0 Phase 8
- ✓ Configurable broadcast channel capacity for high-throughput PTY — v1.0 Phase 1
- ✓ Clear status visibility: user always knows what agent is doing — v1.0 Phase 3
- ✓ Transparent autonomy: every action has explanation surface — v1.0 Phase 3
- ✓ Community skill import with security scanning — v1.0 Phase 7
- ✓ Progressive UX with capability tiers — v1.0 Phase 10
- ✓ Setup wizard with IPC-based config — v1.0 Phase 11
- ✓ CLI polish: launch commands, settings subcommand — v1.0 Phase 12
- ✓ TUI UX: concierge in conversation, tier/feature settings — v1.0 Phase 13
- ✓ Declarative plugin manifest format (JSON) with versioned schema — v2.0 Phase 14
- ✓ Plugin loader: daemon reads manifests from `~/.tamux/plugins/`, validates, registers — v2.0 Phase 14
- ✓ Plugin IPC messages for list/get/enable/disable operations — v2.0 Phase 14
- ✓ SQLite persistence for plugins, plugin settings, and plugin credentials — v2.0 Phase 14

### Active

**v2.0: Plugin Ecosystem**

**Plugin System Core**
- [ ] API proxy layer: daemon proxies HTTP requests per plugin API contracts with transformation templates
- [ ] OAuth2 flow support: authorization code + token refresh for plugins requiring authenticated APIs
- [ ] Plugin settings storage: per-plugin config persisted in daemon, accessible via IPC
- [ ] Plugin command registration: slash commands and CLI subcommands declared in manifest

**Plugin Distribution & Install**
- [ ] `tamux plugin install <source>` CLI command (npm package, GitHub repo, or local path)
- [ ] `tamux plugin uninstall <name>` cleanup with config removal
- [ ] `tamux plugin list` show installed plugins with status
- [ ] npm/npx distribution: plugins publishable as npm packages with standard structure
- [ ] GitHub distribution: install directly from repo URL

**Plugin UI Surfaces**
- [ ] Plugins settings tab in Electron: render plugin config fields from manifest
- [ ] Plugins settings tab in TUI: render plugin config fields from manifest
- [ ] Plugin command palette integration: plugin commands appear alongside built-in commands

**Plugin Skill Bundling**
- [ ] Plugins can bundle YAML skills that reference plugin API contracts
- [ ] Bundled skills auto-registered on plugin install, removed on uninstall
- [ ] Agent discovers plugin capabilities through standard skill system

**Validation: Gmail/Calendar Plugin**
- [ ] Gmail plugin manifest: read inbox, search, send (via Gmail REST API)
- [ ] Calendar plugin manifest: list events, create events (via Google Calendar API)
- [ ] Google OAuth2 flow working end-to-end through daemon proxy
- [ ] Agent answers "what's on my calendar today?" using plugin skills
- [ ] Plugin installable from npm, configurable in Plugins tab

**Carried Forward (from v1.0)**
- [ ] Community skill registry: publish, subscribe, discover skills with provenance metadata
- [ ] Continuous project understanding: living model that evolves with changes
- [ ] Intent inference: agent knows which bug from context + traces + operator model
- [ ] Contextual tool selection: learned heuristics for optimal tool picks
- [ ] Zero-friction first run: single binary or `npx` install
- [ ] Concierge onboarding: guided first experience
- [ ] Graceful complexity disclosure: reveal depth as user grows
- [ ] Unified bridge helper: replace `(window as any).tamux` casts

### Out of Scope

- Cloud-hosted SaaS deployment — tamux is local-first by design, not a hosted service
- Training or fine-tuning LLMs — tamux orchestrates existing models, it doesn't train them
- Mobile app — desktop-first (Electron + TUI); mobile access via gateway messaging platforms
- Multi-user/multi-tenant — single-operator desktop tool; auth complexity not justified
- Real-time collaborative editing — the agent collaborates with the operator, not multiple humans simultaneously
- Browser extension — not an IDE plugin or browser copilot; it's a standalone runtime

## Current Milestone: v2.0 Plugin Ecosystem

**Goal:** Community-friendly declarative plugin system — anyone can extend tamux with JSON manifests that declare commands, settings, API contracts, and bundled skills. Plugins install via CLI/npm and work across TUI and Electron.

**Target features:**
- Declarative plugin manifest format (JSON) with versioned schema
- Daemon API proxy layer with transformation templates and OAuth2 support
- Plugin install/uninstall/list CLI commands
- Plugins settings tab in both TUI and Electron (rendered from manifest)
- Plugin command registration and skill bundling
- npm and GitHub distribution
- Gmail/Calendar integration as validation plugin (proves full loop)

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
*Last updated: 2026-03-24 after Phase 14 (Plugin Manifest & Loader) completion*
