# tamux MEMORY — Validated Core Facts

## Project Scale & Structure
- **Daemon LOC**: ~101,000 lines of Rust across 596 source files.
- **Agent Module**: 429 files dedicated to agent runtime logic.
- **Workspace Root**: `/home/mkurman/gitlab/it/cmux-next`
- **Daemon Package Name**: `tamux-daemon` (crate path is `crates/amux-daemon`, but use the Cargo package name for `cargo -p` invocations).
- **Build System**: Cargo workspace, Node.js frontend, uv for Python tooling.
- **Agent Personas**: The system hosts **8 distinct agent personas** sharing a common daemon runtime and memory infrastructure.

## Core Architecture Facts
- **Daemon-First Design**: The daemon owns all state. TUI, Electron app, CLI, MCP server, and chat gateway are clients.
- **Multi-Agent Runtime**: 8 specialized agents operate concurrently or via handoff, each with isolated SOUL identities but shared access to the common memory model, task queue, and safety controls.
- **Persistence Split**: SQLite for structured state (threads, tasks, goals, provenance); Files for markdown memory, skills, ledgers, transcripts.
- **Startup Hydration**: Resumes from prior durable state on boot — threads, memory, operator model, collaboration, goals.
- **Memory Files**: SOUL.md (identity/principles), MEMORY.md (durable facts/conventions), USER.md (operator profile from SQLite). All have enforced size limits and contradiction checking.
- **Provenance Model**: Every memory write tracked in SQLite with target, mode, source, fact keys, timestamps, and optional confirm/retract status. Desktop Session Vault exposes full provenance UI; TUI controls are pending.

## Validated Implementation Depth
- **Heartbeat/Governance**: Deep implementation for agent health monitoring, lifecycle management, and execution policy enforcement.
- **Goal Runners**: Durable autonomy layer — accepts objectives, plans steps, creates child tasks, dispatches, monitors, replans on failure, reflects on completion, optionally updates memory/skills.
- **Task Queue**: Supports dependencies, scheduling, retry policy, session affinity, parent/child relationships, approval waiting.
- **Tool Execution**: Bounded loop with persisted tool messages, execution traces, causal traces, provenance events, operator feedback learning.
- **Skill System**: Procedural memory with variant metadata, usage tracking, success/failure settlement, promotion/deprecation, automatic branching, merge/convergence.
- **Collaboration Protocol**: Sub-agents coordinate via explicit sessions with contributions, disagreement records, voting, persisted shared state.
- **Semantic Environment Model**: Can inspect workspace for Rust crates, Node packages, Compose services, Terraform/K8s resources, imports, conventions, temporal history.
- **Runtime Tool Synthesis**: Generates guarded tools from CLI/OpenAPI surfaces, maintains registry, promotes proven tools.

## Self-Orchestrating Capabilities (M1–M10)
1. **M1 Operator Model**: Learns output density, risk tolerance, session rhythm, attention topology, implicit feedback.
2. **M2 Anticipatory Runtime**: Morning briefs, thread hydration, stuck-work hints, collaboration disagreement surfacing.
3. **M3 Causal Traces**: Records decision rationale, failed paths, blast radius.
4. **M4 Genetic Skill Evolution**: Variant tracking, success/failure settlement, lifecycle management.
5. **M5 Semantic Environment**: Workspace topology inspection across multiple ecosystems.
6. **M6 Deep Storage**: Provenance-backed memory with contradiction checking, confidence aging, operator confirm/retract.
7. **M7 Collaboration Protocol**: Explicit multi-agent coordination with voting and shared state.
8. **M8 Trusted Provenance**: Hash-chained or Ed25519-signed audit trails for execution integrity.
9. **M9 Implicit Feedback Learning**: Learns from denials, corrections, fallbacks, attention shifts.
10. **M10 Runtime Tool Synthesis**: CLI/OpenAPI to guarded tool generation with promotion path.

## Safety & Approval Model
- Managed command validation with risk labeling
- Structured approval requests (risky work pauses in `awaiting_approval`)
- Sandboxed execution and policy hooks
- Rate limiting and circuit-breaker behavior
- Provenance and audit trails for every action

## First-Time User Guidelines
1. The daemon is the source of truth — start it first.
2. Memory is curated, not dumped — three markdown files with size limits.
3. Goal runners are the autonomy layer — give high-level objectives.
4. Safety is visible — approvals, risk labels, provenance trails are operator-first.
5. This is a Rust codebase — build with `cargo`, preflight with `./scripts/setup.sh --check --profile source`.
6. **8 Agent Personas**: The system routes work to specialized agents (Svarog, Rarog, Weles, etc.) based on capability tags. Handoffs preserve context.

## Active Conventions & Constraints
- Use `tamux-daemon` as the Cargo package name, not the path name.
- MEMORY.md updates must be validated against codebase reality, not assumptions.
- Memory provenance is desktop-first; TUI lacks direct confirm/retract controls currently.
- All core systems verified functional — no major structural gaps in architecture.

## Recent Structural Changes (2026-04-10)
- Deprecated `JustifySkip` variant removed from `SkillRecommendationAction` enum; `None` is now the no-match action.
- Skill recommendation tests updated to expect `SkillRecommendationAction::None` for low-confidence matches.
- All skill recommendation tests passing (21/21).
- `docs/SOUL.md` and `docs/MEMORY.md` updated to reflect multi-agent architecture (8 personas).
- `memory.rs` patched with new `DEFAULT_MEMORY` and `DEFAULT_USER` constants for agent initialization.