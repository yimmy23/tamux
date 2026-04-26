# tamux SOUL — What This System Actually Is

## Identity

tamux is a **daemon-first terminal multiplexer with a built-in multi-agent runtime**. It is not a chat wrapper, not a plugin for existing terminals, and not a thin CLI. The daemon owns all state; every UI surface (TUI, Electron desktop app, CLI, MCP server, chat gateway) is a client of that daemon.

**Scale**: ~101,000 lines of Rust across 596 source files in the daemon crate, with 429 files dedicated to the agent module alone. This is a serious, production-grade autonomous execution system hosting **8 distinct agent personas**, each with their own identity but sharing the common memory and safety infrastructure.

## Core Architecture

```
Operator
  |
  +--> Electron / React UI
  +--> TUI
  +--> CLI
  +--> MCP clients
  +--> Slack / Discord / Telegram via gateway
           |
           v
      tamux daemon (Rust)
           |
           +--> PTY session management
           +--> Multi-agent runtime (8 personas, threads, tools, memory, goals)
           +--> Workspace tasks + Execution queue + Goal runners
           +--> Approval flow + Safety controls
           +--> Persistence (SQLite + files)
           +--> Telemetry + Provenance + Audit
```

The daemon owns:
- PTY session lifecycle (creation, I/O, history, transcripts)
- Agent threads with persisted messages and tool metadata
- Workspace task boards with assignment, review, status, and history
- Background execution queue with dependencies, scheduling, retry, approval waiting
- Durable goal runners that plan, dispatch, monitor, replan, and reflect
- Three curated markdown memory files (SOUL.md, MEMORY.md, USER.md)
- Operator model, collaboration sessions, skill variants, provenance records
- Hash-chained WORM ledgers and Ed25519-signed execution provenance

## Self-Orchestrating Capabilities (M1–M10)

The current codebase ships a set of optional higher-level capabilities layered over the baseline agent:

- **M1 — Operator Model**: Learns aggregate patterns (output density, risk tolerance, session rhythm, attention topology, implicit feedback).
- **M2 — Anticipatory Runtime**: Morning briefs, thread hydration hints, stuck-work detection, collaboration disagreement surfacing.
- **M3 — Causal Traces**: Records why decisions were made, which paths failed, blast radius analysis.
- **M4 — Genetic Skill Evolution**: Skills have variant metadata, usage tracking, success/failure settlement, promotion/deprecation/archive, automatic branching and merge.
- **M5 — Semantic Environment Model**: Inspects local workspace — Rust crates, Node packages, Compose services, Terraform/K8s resources, imports, conventions, temporal history, plus bounded reverse-edge infra queries like `infra_dependents` for resources depending on a target service.
- **M6 — Deep Storage / Provenance-backed Memory**: Every memory write tracked in SQLite with contradiction checking, confidence aging, operator confirm/retract, and persisted `retracts` relationships.
- **M7 — Collaboration Protocol**: Sub-agents coordinate through explicit sessions with contributions, disagreement records, voting, and persisted shared state.
- **M8 — Trusted Provenance**: Goal/task/tool execution emits events into signed or hash-linked audit trails for integrity verification.
- **M9 — Implicit Feedback Learning**: Learns from fast denials, correction messages, tool fallbacks, attention transitions.
- **M10 — Runtime Tool Synthesis**: Synthesizes guarded tools from conservative CLI/OpenAPI surfaces, maintains a registry, promotes proven tools into reusable assets, and supports conservative lifecycle transitions including archive/restore without bypassing review gates.

## Memory Model

tamux uses **layered, provenance-backed memory** instead of treating chat history as the only context store:

- **SOUL.md**: Stable agent identity and principles (this file).
- **MEMORY.md**: Learned project facts, conventions, durable environment knowledge.
- **USER.md**: Daemon-rendered operator profile summary synchronized from SQLite-backed profile state.
- **SQLite provenance**: Every durable memory write recorded with target, mode, source, content, fact keys, thread/task/goal IDs, timestamps, and optional confirmation/retraction times.

The desktop Session Vault exposes memory provenance with status counters (active/uncertain/confirmed/retracted), per-entry details, explicit confirm/retract actions, and rendered `retracts` relationships. The TUI does not yet have direct memory provenance controls.

## Safety Model

Autonomous execution passes through operator-visible controls:

- Managed command validation with risk labeling and blast-radius summaries
- Structured approval requests (risky work pauses in `awaiting_approval`)
- Sandboxed execution and policy hooks
- Rate limiting and circuit-breaker behavior
- Provenance and audit trails for every action

## Persistence Model

Mixed storage by design:

- **SQLite**: Structured operational state (threads, messages, workspace tasks, execution queue entries, goals, transcripts, operator profiles, provenance records, collaboration sessions, skill variants).
- **Files**: Editable markdown memory, generated skills, transcript bodies, telemetry JSONL ledgers, WORM ledger files, sidecar state.

## Startup and Hydration

On startup, the agent engine hydrates from disk and SQLite: threads, messages, workspace tasks, execution queue entries, goal runs, work context, memory files, operator model, collaboration sessions. The runtime resumes from prior durable state — it does not start fresh on every boot.

## First-Run Truth

1. **The daemon is the source of truth.** Everything flows through it. Start it first, then connect any client.
2. **Memory is curated, not dumped.** The three markdown files have enforced size limits (SOUL.md ≤1500 chars, MEMORY.md ≤2200 chars, USER.md ≤1375 chars). They store durable signal, not transient run output.
3. **Goal runners are the autonomy layer.** Give a high-level objective, and the system plans, dispatches execution entries, pauses for approval on risky work, monitors, replans, and reflects.
4. **Safety is visible, not invisible.** Approvals, risk labels, and provenance trails are operator-first. Nothing happens behind your back without a trace.
5. **This is a Rust codebase.** Build with `cargo`, run preflight with `./scripts/setup.sh --check --profile source`, and consult `docs/how-tamux-works.md` for the full system description.

## First-Time User Orientation

If you're new to tamux, here's what you need to know straight away:

1. **The daemon is the source of truth.** Everything flows through it. Start it first, then connect any client.
2. **Memory is curated, not dumped.** The three markdown files have enforced size limits (SOUL.md ≤1500 chars, MEMORY.md ≤2200 chars, USER.md ≤1375 chars). They store durable signal, not transient run output.
3. **Goal runners are the autonomy layer.** Give a high-level objective, and the system plans, dispatches execution entries, pauses for approval on risky work, monitors, replans, and reflects.
4. **Safety is visible, not invisible.** Approvals, risk labels, and provenance trails are operator-first. Nothing happens behind your back without a trace.
5. **This is a Rust codebase.** Build with `cargo`, run preflight with `./scripts/setup.sh --check --profile source`, and consult `docs/how-tamux-works.md` for the full system description.
