---
gsd_state_version: 1.0
milestone: v2.0
milestone_name: Plugin Ecosystem
status: Phase complete — ready for verification
stopped_at: Completed 19-01-PLAN.md
last_updated: "2026-03-24T23:39:20.853Z"
progress:
  total_phases: 20
  completed_phases: 15
  total_plans: 58
  completed_plans: 55
---

# Project State

## Project Reference

See: .planning/PROJECT.md (updated 2026-03-24)

**Core value:** An agent that feels alive and gets smarter over time -- while remaining simple enough that anyone can understand what it's doing and why.
**Current focus:** Phase 19 — plugin-skills-commands

## Current Position

Phase: 19 (plugin-skills-commands) — EXECUTING
Plan: 2 of 2

## Performance Metrics

**Velocity:**

- Total plans completed: 0
- Average duration: -
- Total execution time: 0 hours

**By Phase:**

| Phase | Plans | Total | Avg/Plan |
|-------|-------|-------|----------|
| - | - | - | - |

**Recent Trend:**

- Last 5 plans: -
- Trend: -

*Updated after each plan completion*
| Phase 01 P01 | 25min | 3 tasks | 2 files |
| Phase 01 P02 | 36min | 2 tasks | 30 files |
| Phase 01 P03 | 10min | 2 tasks | 9 files |
| Phase 02 P01 | 9min | 2 tasks | 5 files |
| Phase 02-core-heartbeat P02 | 11min | 2 tasks | 7 files |
| Phase 02 P03 | 4min | 2 tasks | 2 files |
| Phase 02 P04 | 4min | 2 tasks | 7 files |
| Phase 03 P01 | 8min | 2 tasks | 7 files |
| Phase 03 P04 | 5min | 2 tasks | 10 files |
| Phase 03 P02 | 5min | 2 tasks | 3 files |
| Phase 03 P03 | 5min | 2 tasks | 10 files |
| Phase 03 P05 | 7min | 2 tasks | 6 files |
| Phase 04 P01 | 9min | 2 tasks | 6 files |
| Phase 04 P02 | 8min | 2 tasks | 6 files |
| Phase 04 P03 | 10min | 2 tasks | 15 files |
| Phase 05 P01 | 12min | 2 tasks | 6 files |
| Phase 05 P03 | 5min | 2 tasks | 2 files |
| Phase 05 P02 | 5min | 2 tasks | 3 files |
| Phase 05 P04 | 9min | 2 tasks | 4 files |
| Phase 06 P01 | 6min | 2 tasks | 6 files |
| Phase 06 P02 | 7min | 1 tasks | 4 files |
| Phase 06 P03 | 9min | 2 tasks | 7 files |
| Phase 07-community-skills P01 | 569 | 2 tasks | 8 files |
| Phase 07-community-skills P02 | 23m 15s | 2 tasks | 9 files |
| Phase 07-community-skills P03 | 3m 23s | 2 tasks | 2 files |
| Phase 08 P01 | 6min | 2 tasks | 6 files |
| Phase 08 P02 | 6min | 2 tasks | 2 files |
| Phase 08 P03 | 17min | 3 tasks | 2 files |
| Phase 08 P04 | 8min | 3 tasks | 12 files |
| Phase 09 P02 | 2min | 1 tasks | 1 files |
| Phase 09 P03 | 2min | 2 tasks | 2 files |
| Phase 09 P01 | 3min | 2 tasks | 5 files |
| Phase 09 P04 | 4min | 2 tasks | 3 files |
| Phase 10 P02 | 13min | 2 tasks | 38 files |
| Phase 10 P01 | 14min | 2 tasks | 8 files |
| Phase 10-progressive-ux P04 | 9min | 2 tasks | 16 files |
| Phase 10 P03 | 7min | 2 tasks | 7 files |
| Phase 10 P05 | 10min | 2 tasks | 9 files |
| Phase 11 P01 | 4min | 2 tasks | 2 files |
| Phase 11 P02 | 6min | 2 tasks | 1 files |
| Phase 12 P01 | 1min | 1 tasks | 1 files |
| Phase 13 P03 | 2min | 1 tasks | 1 files |
| Phase 13 P01 | 3min | 2 tasks | 4 files |
| Phase 13-tui-ux-fixes P02 | 7min | 1 tasks | 6 files |
| Phase 12 P02 | 2min | 2 tasks | 2 files |
| Phase 14 P01 | 7min | 2 tasks | 7 files |
| Phase 14 P02 | 8min | 2 tasks | 5 files |
| Phase 15 P01 | 4min | 2 tasks | 4 files |
| Phase 15 P02 | 4min | 2 tasks | 4 files |
| Phase 16 P01 | 7min | 2 tasks | 9 files |
| Phase 16 P02 | 4min | 2 tasks | 3 files |
| Phase 16 P03 | 10min | 2 tasks | 11 files |
| Phase 17 P01 | 9min | 2 tasks | 8 files |
| Phase 17 P02 | 8min | 2 tasks | 6 files |
| Phase 19 P01 | 4min | 2 tasks | 7 files |

## Accumulated Context

### Decisions

Decisions are logged in PROJECT.md Key Decisions table.
Recent decisions affecting current work:

- [Roadmap]: SQLite WAL + tokio-rusqlite 0.6.0 must ship before heartbeat (concurrent write safety)
- [Roadmap]: Heartbeat split into core (Phase 2) and adaptive (Phase 4) to deliver "alive" feeling early
- [Roadmap]: Phases 8 (Gateway) and 9 (Distribution) depend only on Phase 1, can run in parallel with main chain
- [Roadmap]: Community skills (Phase 7) separated from skill discovery (Phase 6) due to security concerns from ClawHub incident
- [Phase 01]: tokio-rusqlite 0.6.0 for rusqlite 0.32 compat; busy_timeout=5000 for D-13; refresh_thread_stats moved to standalone fn
- [Phase 01]: Arc<HistoryStore> shared via server.rs; Handle::block_on() for PTY sync-to-async bridge; channel capacities configurable via serde defaults
- [Phase 01]: Approach B (wrap at call sites) for circuit breaker wiring; Arc<CircuitBreakerRegistry> shared between AgentEngine and ConciergeEngine
- [Phase 02]: Async test helper constructs full AgentEngine with real HistoryStore rather than mocking for integration correctness
- [Phase 02]: check_unreplied_messages returns items_found=0 in Phase 2 (conservative) -- full detection deferred to Phase 8
- [Phase 02]: Severity escalation uses threshold multipliers (1.5x/3x for todos, 2x/4x for goals) for progressive urgency
- [Phase 02-core-heartbeat]: Pure function check_quiet_window extracted for testability; tokio::sync::Notify for config change notification to background loops
- [Phase 02]: Extracted orchestration decisions as pure functions for deterministic testing of heartbeat logic
- [Phase 02]: Single LLM call per heartbeat cycle (BEAT-08/D-09); silent default broadcast (D-14/BEAT-03); persist every cycle regardless of LLM outcome (Pitfall 4/D-12)
- [Phase 02]: Used accent_secondary (amber) for priority-2 heartbeat indicators; non-existent accent_warn in TUI theme
- [Phase 02]: HeartbeatDigest items as tuple transport in ClientEvent, converted to HeartbeatDigestItemVm in event handler
- [Phase 03]: AuditEntryRow kept daemon-internal; AuditEntryPublic in protocol for cross-crate use (drops raw_data_json for lean payloads)
- [Phase 03]: Dynamic SQL parameter binding in list_action_audit for flexible filtering without N query variants
- [Phase 03]: Cleanup combines max_age_days and max_entries for dual retention strategy on action_audit
- [Phase 03]: AuditPanel uses same 440px slide-over pattern as NotificationPanel for consistent UX
- [Phase 03]: ConfidenceBadge hidden for confident band per D-10; HeartbeatDigest explanation appended to notification body per D-01
- [Phase 03]: HeartbeatDigest confidence set to None; confidence applies to causal trace audit entries not heartbeat digests
- [Phase 03]: Escalation audit helper is a pure function in escalation.rs; AgentEngine callers do the actual persist/broadcast
- [Phase 03]: Audit cleanup piggybacks on heartbeat cycle after entry creation; scope-guarded by config.audit.scope toggles
- [Phase 03]: AuditState follows TaskState reducer pattern; HeartbeatDigestVm explanation is Option<String> for backward compat; confidence dots hidden at confident band per D-10
- [Phase 03]: Skipped duplicate AuditQuery handler (Plan 01 already added it); used humantime for timestamp formatting
- [Phase 03]: EscalationCancel race condition: already-resolved returns success (not error) per Pitfall 6
- [Phase 04]: EMA alpha default 0.3 for ~5-session adaptation; priority floor 0.1 per D-05; AuditDismiss stubbed for Plan 02 wiring
- [Phase 04]: Action type keys use actual DB values (stale_todo, stuck_goal, etc.) for feedback loop queries
- [Phase 04]: Decay rate 0.05 per dismissal, recovery rate 0.1 per acted-on; 7-day lookback window for feedback signals
- [Phase 04]: Morning brief flag cleared only after successful LLM synthesis to prevent data loss on failure
- [Phase 04]: Learning transparency uses >2hr peak shift and <0.5 weight thresholds for pattern change detection
- [Phase 05]: TaskStatus::InProgress for active task detection; tombstone replaced_at=created_at; ExecutionTraceRow Option<T> for nullable columns
- [Phase 05]: ContextArchiveRow in history.rs follows Row pattern; conversion to ArchiveEntry in persistence.rs
- [Phase 05]: Continuity flag stored as two consolidation_state keys (thread_id + topic) for one-shot clearing
- [Phase 05]: HeuristicStore.update_tool uses succeeded:bool matching existing API (not effectiveness:f64 from plan)
- [Phase 05]: Facts with no provenance record skipped during decay (no meaningful confidence without last_confirmed_at)
- [Phase 05]: Learned patterns threshold: usage_count >= 5 AND effectiveness >= 0.6 for system prompt injection
- [Phase 05]: Memory refinement handles one conflict group per tick (budget-safe); send_refinement_llm_call uses provider api_transport
- [Phase 06]: SkillMaturityStatus supports legacy kebab-case parsing for backward compat; pure functions take pre-fetched patterns for testability
- [Phase 06]: Separate skill_draft_watermark from trace_review_watermark to avoid coupling (Pitfall 7)
- [Phase 06]: One LLM call max per tick for drafting and mental tests; 2/3 mental test pass threshold; send_refinement_llm_call promoted to pub(super)
- [Phase 06]: Skill announcements on AgentEngine via concierge.rs impl block; dual notification for canonical promotions; bincode requires no serde skip attributes
- [Phase 07-community-skills]: agentskills.io remains the lean interchange format while tamux imports synthesize default local extensions for lifecycle tracking.
- [Phase 07-community-skills]: New community skill IPC variants return explicit daemon errors until Plan 07-02 wires full registry handlers, preserving exhaustive matches and green builds.
- [Phase 07-community-skills]: Verified publishers skip only the tier-3 review branch while pattern and structural scans still run for every import.
- [Phase 07-community-skills]: Community imports always write into ~/.tamux/skills/community and immediately reset variant status to draft after registration.
- [Phase 07-community-skills]: Registry publish uses multipart uploads with env-token auth so the IPC path is complete without introducing a config migration in this plan.
- [Phase 07-community-skills]: CLI import resolves publisher verification from registry search results so daemon scan policy receives the trust signal it expects.
- [Phase 07-community-skills]: Search output uses compact fixed-width columns with truncation so community results stay readable in terminals.
- [Phase 08]: String type for GatewayStatus.status field to avoid coupling wire format to internal health module
- [Phase 08]: Sentinel character approach for Slack bold conversion to prevent double-converting italic markers
- [Phase 08]: Safe Telegram MarkdownV2 escaping (escape everything) as baseline; formatting-preserving version deferred
- [Phase 08]: ThreadContext uses Option fields per platform for type safety; Slack poll interval defaults to 60s; HeartbeatDigest emitted on gateway health transitions
- [Phase 08]: Combined Tasks 1+2 into single commit since format conversion, rate limiting, chunking, and thread replies are interleaved in same function rewrites
- [Phase 08]: try_lock for thread context lookups from reply_contexts to avoid blocking when gateway_state lock is contended
- [Phase 08]: Unreplied detection severity escalation: Low at threshold, Medium at 2x, High at 4x -- matching check_stale_todos pattern
- [Phase 08]: GatewayStatusVm on TuiModel (not ChatState) for cross-thread UI state; IPC-backed config via existing agentGetConfig/agentSetConfigItem pattern
- [Phase 08]: Electron bridges disabled by default via gateway_electron_bridges_enabled feature flag; WhatsApp excluded from gating per D-06
- [Phase 09]: glab CLI for GitLab Release creation; cross tool for ARM64 cross-compilation; manual stages for macOS builds and npm publish
- [Phase 09]: POSIX sh (not bash) for install.sh to maximize portability; desktop app prompt uses terminal detection to skip when piped
- [Phase 09]: Non-blocking failure: all postinstall errors exit 0 to never break npm install
- [Phase 09]: Two-layer fallback: postinstall downloads first, bin wrapper retries at runtime if binary missing
- [Phase 09]: install.js exported as module for require() from bin wrapper fallback; poll-based binary wait (500ms/60s) for async completion
- [Phase 09]: Async setup wizard using reqwest since CLI already uses tokio; Optional subcommand for first-run detection; amux_data_dir() used over tamux_data_dir() (not re-exported)
- [Phase 10]: AmuxBridge type expanded with 30+ missing method declarations to replace any-typed casts
- [Phase 10]: getAgentBridge() delegates to shared getBridge() rather than being eliminated to preserve existing imports
- [Phase 10]: CapabilityTier module pub(crate) visibility for server.rs import; AgentSetTierOverride no-response pattern; manual JSON serialization for non-Serialize types
- [Phase 10-progressive-ux]: Tier gating wraps SettingsPanel tab content (5 tabs) rather than individual sub-sections; TUI uses pre-computed boolean flags for per-frame efficiency
- [Phase 10]: ConciergeActionType enum extended with typed variants for compile-time safety over raw strings
- [Phase 10]: Tier change detection runs every heartbeat; disclosure queue on AgentEngine for shared heartbeat access
- [Phase 10]: Object-to-array transform in statusStore for provider_health and gateway_statuses (daemon sends object maps, store normalizes to arrays)
- [Phase 10]: TUI agent_status_line maps existing agent_activity event strings to user-friendly colored status indicators
- [Phase 11]: Auth source string read from ProviderAuthState rather than hardcoded mapping; legacy needs_setup_at kept as read-only fallback; AgentSetTierOverride fire-and-forget with 100ms delay
- [Phase 11]: SecurityLevel serialized as kebab-case; model selection falls back to text_input; gateway restricted to Power User+ per D-09
- [Phase 12]: find_sibling_binary checks exe parent dir first, then bare name for PATH lookup -- no which dependency added
- [Phase 13]: Used existing StatusIndicator and Badge components from shadcn redesign for Phase 10 re-integration consistency
- [Phase 13]: concierge_banner_visible() returns false to disable overlay; action buttons as text in chat message; recent actions from HeartbeatDigest events
- [Phase 13-tui-ux-fixes]: Features tab at index 9 (between Concierge and Advanced); optimistic local raw config update alongside IPC SetConfigItem for responsive UI; security level in Features uses permissive/balanced/strict distinct from Advanced tab levels
- [Phase 12]: Settings operations use IPC roundtrip to daemon (AgentGetConfig/AgentSetConfigItem), never config.json; dot-notation keys converted to JSON pointer format
- [Phase 14]: jsonschema 0.45 for JSON Schema Draft 2020-12 validation; compile once, reuse validator
- [Phase 14]: Permissive deserialization with serde(flatten) captures unknown fields per D-01
- [Phase 14]: Plugin module at crate::plugin (daemon-level, not inside agent/) to avoid circular dependency
- [Phase 14]: HistoryStore.conn made pub(crate) for cross-module SQLite access from plugin persistence
- [Phase 14]: PluginManager reads from SQLite for list_plugins (not just in-memory) to reflect IPC-toggled enabled state
- [Phase 14]: Settings schema extracted from manifest_json as standalone JSON for future dynamic form rendering
- [Phase 15]: Combined Tasks 1+2 into single commit due to Rust exhaustive match; conflict detection uses in-memory map for loaded state; re-install skips conflict check for same-named plugins
- [Phase 15]: reqwest::blocking::Client for GitHub tarball download to avoid block_on panic inside tokio runtime
- [Phase 15]: Two-step install: files first then IPC; failed registration triggers file cleanup; daemon-unreachable prints warning
- [Phase 16]: Base64 encoding for secret values as placeholder until Phase 18 AES-256-GCM encryption
- [Phase 16]: HEAD request probe with 5s timeout for connectivity testing; 401/403 treated as success
- [Phase 16]: Secret masking at PluginManager layer; persistence returns raw decoded values for daemon use
- [Phase 16]: Save-on-blur for text/password/number via wrapper div onBlur; immediate for toggle/select
- [Phase 16]: Auth status hardcoded to not_configured in Phase 16; real OAuth wiring deferred to Phase 18
- [Phase 16]: PluginSettingsState stored separately on TuiModel (not inside SettingsState) because plugin data is dynamic and schema-driven
- [Phase 16]: DaemonCommand enum extended with 7 plugin variants for TUI plugin IPC routing through main.rs dispatcher
- [Phase 17]: url crate added as direct dependency for SSRF URL parsing
- [Phase 17]: Handlebars helpers as manual HelperDef impl for parameter control; serde skip_serializing_if incompatible with bincode
- [Phase 17]: render_request uses spawn_blocking + 1s timeout for template isolation; PluginApiCall stub until Plan 17-02
- [Phase 17]: OnceLock for plugin_manager field on AgentEngine to avoid runtime locking overhead while supporting post-construction initialization
- [Phase 17]: plugin_api_call tool always available (not gated on config); PluginManager handles disabled/missing checks internally
- [Phase 19]: LLM augmentation over bypass: plugin commands inject system hints so LLM naturally uses plugin API tool, preserving agent tool-calling loop
- [Phase 19]: OnceLock for plugin_manager on AgentEngine: set after both are constructed in server.rs, avoids circular dependency

### Pending Todos

None yet.

### Blockers/Concerns

- tokio-rusqlite must be 0.6.0 (NOT 0.7.0) for rusqlite 0.32 compatibility
- cargo-dist CI generation targets GitHub Actions; custom GitLab CI config needed for Phase 9
- Memory consolidation decay parameters (lambda=0.01, ~69hr half-life) need empirical validation in Phase 5

## Session Continuity

Last session: 2026-03-24T23:39:20.850Z
Stopped at: Completed 19-01-PLAN.md
Resume file: None
