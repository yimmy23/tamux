# Roadmap: tamux — The Agent That Lives

## Overview

**v1.0 (Complete):** Bridges tamux's engineering depth to user-felt experience — production hardening, heartbeat, transparency, memory consolidation, skill evolution, gateways, distribution, progressive UX.

**v2.0 (Current):** Community-friendly plugin ecosystem — declarative JSON manifests, daemon API proxy, OAuth2, cross-surface settings UI, skill bundling. Gmail/Calendar integration validates the system end-to-end.

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
- [x] **Phase 10: Progressive UX** - Capability tiers, concierge onboarding, typed bridge helper, consistent status (completed 2026-03-24)
- [x] **Phase 11: Setup Wizard Rewrite** - IPC-based config, synced providers, arrow-key navigation, tier-appropriate depth (completed 2026-03-24)
- [x] **Phase 12: CLI Polish** - Launch commands, fix stats, hide internals, audit IDs, settings subcommand (completed 2026-03-24)
- [x] **Phase 13: TUI UX Fixes** - Concierge in conversation, tier settings, feature settings, recent actions (completed 2026-03-24)

### v2.0: Plugin Ecosystem

- [x] **Phase 14: Plugin Manifest & Loader** - JSON Schema definition, manifest validation, daemon loader, SQLite storage (completed 2026-03-24)
- [x] **Phase 15: Plugin CLI & Install** - Extend plugins.rs for new manifest format, npm/GitHub/local install, daemon IPC registration (completed 2026-03-24)
- [x] **Phase 16: Plugin Settings UI** - Plugins tab in TUI and Electron, dynamic form rendering from manifest schema (completed 2026-03-24)
- [x] **Phase 17: API Proxy Layer** - HTTP proxy in daemon, Handlebars templates, SSRF protection, rate limiting (completed 2026-03-24)
- [ ] **Phase 18: OAuth2 Flow** - Authorization code + PKCE, encrypted token storage, refresh lifecycle, UI integration
- [ ] **Phase 19: Plugin Skills & Commands** - Bundled skill registration, command namespacing, agent integration
- [ ] **Phase 20: Gmail/Calendar Validation Plugin** - End-to-end proof: manifest, OAuth, API proxy, skills, install, configure, use

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
- [x] 01-02-PLAN.md — Shared Arc<HistoryStore> wiring + configurable broadcast channel capacity
- [x] 01-03-PLAN.md — Circuit breaker wiring into LLM call path with per-provider instances

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
**Plans:** 4 plans

Plans:
- [x] 02-01-PLAN.md — Heartbeat type contracts, config extension, and four built-in check functions
- [x] 02-02-PLAN.md — Cron scheduling, quiet hours/DND, and SQLite heartbeat_history persistence
- [x] 02-03-PLAN.md — LLM synthesis orchestration, digest broadcasting, and gateway_loop wiring
- [x] 02-04-PLAN.md — Gap closure: wire HeartbeatDigest rendering in TUI and Electron clients

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
**Plans:** 3/5 plans executed
**UI hint**: yes

Plans:
- [x] 03-01-PLAN.md — Daemon foundation: explanation module, action_audit table, AuditAction event, IPC messages
- [x] 03-02-PLAN.md — Daemon integration: heartbeat/escalation/causal-trace audit entry wiring
- [x] 03-03-PLAN.md — TUI audit feed: AuditState reducer, audit widget, escalation status line
- [x] 03-04-PLAN.md — Electron audit panel: auditStore, AuditPanel components, event listener
- [x] 03-05-PLAN.md — CLI audit subcommand and escalation cancel IPC flow

### Phase 4: Adaptive Heartbeat
**Goal**: The heartbeat learns the operator's rhythm and priorities, becoming a proactive assistant that surfaces the right things at the right time
**Depends on**: Phase 3
**Requirements**: BEAT-06, BEAT-07, BEAT-09
**Success Criteria** (what must be TRUE):
  1. Agent learns when the user is active and surfaces proactive items during those windows, staying quiet during off-hours
  2. Heartbeat checks evolve based on what the user actually cares about — frequently dismissed checks get deprioritized
  3. M2 Anticipatory Runtime outputs (morning brief, stuck hints, predictive hydration) are delivered through the heartbeat cycle
**Plans:** 3 plans

Plans:
- [x] 04-01-PLAN.md — Type contracts, config extensions, EMA pure functions, dismissal tracking schema, protocol messages
- [x] 04-02-PLAN.md — Activity-aware scheduling in gateway_loop, priority gating in heartbeat, AuditDismiss server handler
- [x] 04-03-PLAN.md — M2 anticipatory merge into heartbeat synthesis, learning transparency templates, client dismiss UX

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
**Plans:** 4 plans

Plans:
- [x] 05-01-PLAN.md — Foundation types, config, SQLite tables, idle detection and decay pure functions
- [x] 05-02-PLAN.md — Core consolidation engine: trace review, tombstone ops, heartbeat Phase 10 wiring
- [x] 05-03-PLAN.md — Cross-session continuity: goal run pausing, context restoration, learning store persistence
- [x] 05-04-PLAN.md — Heuristic system prompt injection and LLM-powered memory refinement

### Phase 6: Skill Discovery
**Goal**: The agent recognizes when it solved something novel and automatically creates skills that improve through real-world use
**Depends on**: Phase 5
**Requirements**: SKIL-01, SKIL-02, SKIL-03, SKIL-04, SKIL-05, SKIL-06
**Success Criteria** (what must be TRUE):
  1. When the agent solves a problem through high complexity (many tools, replans, eventual success), it automatically drafts a candidate skill
  2. Agent distinguishes novel solution approaches from known patterns — only genuinely new approaches trigger skill drafting
  3. Skills follow a visible maturity lifecycle (Draft to Testing to Active to Proven to Canonical) with real-world use counts driving promotion
  4. New and promoted skills are surfaced to the user naturally in conversation via concierge messaging
**Plans:** 3 plans

Plans:
- [x] 06-01-PLAN.md — Foundation types, pure functions, config extensions, history methods, preflight filter fix
- [x] 06-02-PLAN.md — Consolidation sub-tasks: candidate flagging, LLM skill drafting, mental tests, lifecycle promotions
- [x] 06-03-PLAN.md — Concierge skill announcements, HeartbeatDigest events, IPC protocol, CLI skill subcommand

### Phase 7: Community Skills
**Goal**: Operators can share skills with provenance and import community skills safely
**Depends on**: Phase 6
**Requirements**: SKIL-07, SKIL-08, SKIL-09, SKIL-10
**Success Criteria** (what must be TRUE):
  1. Operator can publish a proven skill to the community registry with full provenance metadata
  2. Imported community skills reset to Draft maturity and must prove themselves locally before promotion
  3. Every imported skill passes a mandatory static analysis security scan before it can execute
  4. Skill format is compatible with the agentskills.io standard (SKILL.md + metadata frontmatter)
**Plans:** 3 plans

Plans:
- [x] 07-01-PLAN.md — Security scanner, format conversion types, and protocol message extensions
- [x] 07-02-PLAN.md — Registry client, import/export/publish flows, and server IPC handlers
- [x] 07-03-PLAN.md — CLI subcommands for search, import, export, and publish

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
**Plans:** 4 plans
**UI hint**: yes

Plans:
- [x] 08-01-PLAN.md — Foundation types: gateway_health.rs, gateway_format.rs pure modules, AgentEvent::GatewayStatus, config feature flag
- [x] 08-02-PLAN.md — Health-aware polling: GatewayState health tracking, exponential backoff, status events, env var fallback, thread context
- [x] 08-03-PLAN.md — Send tool enhancements: thread-aware replies, format conversion, rate limiting, chunking, unreplied detection
- [x] 08-04-PLAN.md — Client status display: TUI sidebar indicators, Electron health badges, bridge deprecation feature flag

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
**Plans:** 4 plans

Plans:
- [x] 09-01-PLAN.md — Thin npm wrapper package with postinstall binary downloader and bin launcher
- [x] 09-02-PLAN.md — GitLab CI release pipeline with multi-platform build matrix and release automation
- [x] 09-03-PLAN.md — Unix install script (install.sh) and PowerShell installer (install.ps1)
- [x] 09-04-PLAN.md — First-run setup wizard with provider configuration and daemon auto-start

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
**Plans:** 5/5 plans complete
**UI hint**: yes

Plans:
- [x] 10-01-PLAN.md — Daemon foundation: CapabilityTier enum, resolve_tier() pure function, protocol messages, setup wizard self-assessment
- [x] 10-02-PLAN.md — Typed getBridge() helper replacing all 85 unsafe (window as any) casts across 39 frontend files
- [x] 10-03-PLAN.md — Concierge onboarding with tier-adapted content, tier transition announcements, disclosure queue
- [x] 10-04-PLAN.md — Client-side tier gating: Zustand tierStore, TierGatedSection component, TUI TierState
- [x] 10-05-PLAN.md — Unified status visibility: statusStore, enhanced StatusBar, TUI sidebar status, CLI status subcommand

### Phase 11: Setup Wizard Rewrite
**Goal**: Setup wizard correctly configures the daemon via IPC (not config.json), syncs with actual provider list, supports arrow-key navigation, and offers tier-appropriate configuration depth
**Depends on**: Phase 10
**Requirements**: DIST-05 (reopen), PRUX-01
**Gap Closure:** Closes UAT issues: wizard->daemon config path, provider list mismatch, input UX, advanced user path, newcomer security defaults, web search/gateway optional steps
**Success Criteria** (what must be TRUE):
  1. Setup wizard writes config to daemon DB via IPC, NOT to config.json
  2. Provider list matches daemon's actual provider definitions
  3. Navigation uses arrow keys/enter/tab (not number input)
  4. Advanced users get optional steps for web search, gateway, and tool configuration
  5. Newcomer tier defaults to strictest approval thresholds
**Plans:** 2/2 plans complete

Plans:
- [x] 11-01-PLAN.md — IPC-based wizard core: crossterm navigation, daemon-queried providers, IPC config writes
- [x] 11-02-PLAN.md — Tier-gated optional steps: security defaults, model selection, web search, gateway

### Phase 12: CLI Polish
**Goal**: CLI subcommands are complete, functional, and don't expose internal-only commands
**Depends on**: Phase 11
**Requirements**: DIST-01
**Gap Closure:** Closes UAT issues: missing tui/gui commands, broken stats, internal command hiding, audit ID display, settings subcommand
**Success Criteria** (what must be TRUE):
  1. `tamux tui` launches the TUI and `tamux gui` launches Electron
  2. `tamux stats` returns valid statistics without deserialization errors
  3. Internal commands (attach, new, scrub) are hidden from `tamux --help`
  4. `tamux audit` list output shows entry IDs usable with `--detail`
  5. `tamux settings` allows CLI-based configuration of all daemon settings
**Plans:** 1/2 plans complete

Plans:
- [x] 12-01-PLAN.md — Add tui/gui launch commands, stats alias, hide internals, fix audit ID display
- [ ] 12-02-PLAN.md — Settings subcommand with list/get/set via daemon IPC

### Phase 13: TUI UX Fixes
**Goal**: TUI correctly renders concierge onboarding in conversation, exposes tier and feature settings, and shows full status information
**Depends on**: Phase 11
**Requirements**: PRUX-03, PRUX-04, PRUX-06
**Gap Closure:** Closes UAT issues: concierge clipping, missing tier settings, missing feature settings, TUI recent_actions
**Success Criteria** (what must be TRUE):
  1. Concierge onboarding message renders in the conversation thread, not a clipped overlay panel
  2. TUI settings panel includes tier override selector (D-03)
  3. TUI advanced settings tab shows controls for heartbeat, memory, skills, gateway features
  4. TUI sidebar displays recent autonomous actions from AgentStatusResponse
**Plans:** 2/3 plans complete
**UI hint**: yes

Plans:
- [ ] 13-01-PLAN.md — Concierge onboarding in conversation thread + sidebar recent actions
- [x] 13-02-PLAN.md — TUI Features settings tab with tier, heartbeat, memory, and skills controls
- [x] 13-03-PLAN.md — Electron StatusBar shadcn re-integration (tier badge, activity, provider health, recent actions)

### Phase 14: Plugin Manifest & Loader
**Goal**: Define the plugin manifest format and build the daemon-side loader that reads, validates, and registers plugins from disk
**Depends on**: Nothing (v2.0 foundation)
**Requirements**: PLUG-01, PLUG-02, PLUG-03, PLUG-04, PLUG-05, PLUG-06, PLUG-07, PLUG-08, PLUG-09
**Success Criteria** (what must be TRUE):
  1. A JSON Schema for plugin.json v1 exists and validates all manifest fields (settings, api, commands, skills, auth)
  2. Daemon loads all valid manifests from `~/.tamux/plugins/*/plugin.json` on startup
  3. Invalid manifests are rejected with clear error messages (which field, what's wrong)
  4. Loaded plugins are persisted in SQLite `plugins` table with full metadata
  5. New IPC messages exist for plugin operations (list, get, enable, disable)
  6. Manifest size limits enforced (100KB, 50 endpoints, 30 settings)
**Plans:** 2/2 plans complete

Plans:
- [x] 14-01-PLAN.md — Plugin manifest serde structs, JSON Schema v1, and IPC message variants
- [x] 14-02-PLAN.md — Plugin loader, SQLite persistence, PluginManager, and server.rs IPC wiring

### Phase 15: Plugin CLI & Install
**Goal**: Users can install, uninstall, and manage plugins from the CLI, with the daemon registering plugins via IPC
**Depends on**: Phase 14
**Requirements**: INST-01, INST-02, INST-03, INST-04, INST-05, INST-06, INST-07, INST-08
**Success Criteria** (what must be TRUE):
  1. `tamux plugin install <npm-package>` fetches from npm, validates manifest, registers with daemon
  2. `tamux plugin install <github-url>` clones repo, validates manifest, registers with daemon
  3. `tamux plugin install <local-path>` symlinks/copies, validates manifest, registers with daemon
  4. `tamux plugin uninstall <name>` removes files, deregisters from daemon, cleans up bundled skills
  5. `tamux plugin list` shows all installed plugins with status
  6. Plugin install rejects on command/skill name conflicts with clear error
**Plans:** 2/2 plans complete

Plans:
- [x] 15-01-PLAN.md — IPC messages (PluginInstall/Uninstall), PluginManager register/unregister/conflict-check, server handlers
- [x] 15-02-PLAN.md — CLI plugin add/remove/ls/enable/disable with npm/GitHub/local source detection and daemon IPC

### Phase 16: Plugin Settings UI
**Goal**: Both TUI and Electron render a Plugins settings tab with dynamic forms generated from plugin manifest schemas
**Depends on**: Phase 14
**Requirements**: PSET-01, PSET-02, PSET-03, PSET-04, PSET-05, PSET-06, PSET-07
**Success Criteria** (what must be TRUE):
  1. Electron settings panel has a "Plugins" tab listing installed plugins with enable/disable toggle
  2. TUI settings panel has a "Plugins" tab listing installed plugins with enable/disable toggle
  3. Selecting a plugin renders its settings form dynamically from manifest schema
  4. Secret fields are masked in UI, encrypted in storage
  5. Settings changes flow from UI -> IPC -> daemon SQLite (single source of truth)
  6. OAuth-enabled plugins show "Connect" / "Connected" / "Reconnect" button
**Plans:** 3/3 plans complete
**UI hint**: yes

Plans:
- [x] 16-01-PLAN.md — IPC protocol messages, daemon persistence/handlers, agent bridge, Electron/TUI client methods
- [x] 16-02-PLAN.md — Electron PluginsTab: pluginStore, dynamic settings form, auth badges, test connection
- [x] 16-03-PLAN.md — TUI Plugins tab: SettingsTab::Plugins, render_plugins_tab widget, settings_handlers wiring

### Phase 17: API Proxy Layer
**Goal**: Daemon proxies HTTP requests for plugins per their API contract definitions, with template rendering and security protections
**Depends on**: Phase 14
**Requirements**: APRX-01, APRX-02, APRX-03, APRX-04, APRX-05, APRX-06, APRX-07
**Success Criteria** (what must be TRUE):
  1. Daemon makes HTTP requests on behalf of plugins using endpoint definitions from manifest
  2. Request URLs, params, and headers are rendered via Handlebars templates
  3. Response bodies are transformed via response templates into agent-friendly text
  4. Requests to internal IP ranges (127.x, 10.x, 172.16-31.x, 192.168.x, 169.254.x) are blocked
  5. Template rendering has 1-second timeout and strict mode
  6. Rate limits from manifest enforced via token bucket
  7. Errors returned to agent with actionable context
**Plans:** 2/2 plans complete

Plans:
- [x] 17-01-PLAN.md — SSRF validation, rate limiter, Handlebars template engine, PluginApiError, IPC protocol messages
- [x] 17-02-PLAN.md — HTTP execution, PluginManager.api_call() orchestration, tool executor wiring, server.rs IPC handler

### Phase 18: OAuth2 Flow
**Goal**: Plugins requiring OAuth2 can authenticate users through the daemon, with tokens encrypted at rest and automatically refreshed
**Depends on**: Phase 17
**Requirements**: AUTH-01, AUTH-02, AUTH-03, AUTH-04, AUTH-05, AUTH-06, AUTH-07
**Success Criteria** (what must be TRUE):
  1. Daemon handles full OAuth2 authorization code + PKCE flow for plugins
  2. Temporary localhost HTTP listener receives OAuth callback
  3. Tokens encrypted at rest in SQLite `plugin_credentials` table
  4. Automatic token refresh at 80% TTL, with user-facing reconnect on failure
  5. Tokens never appear in agent context or LLM API calls
  6. Plugin credentials redacted in all daemon logging
  7. UI shows auth status per plugin: Not Configured / Connected / Token Expired
**Plans:** TBD

### Phase 19: Plugin Skills & Commands
**Goal**: Plugins can bundle YAML skills and register commands that integrate seamlessly with the existing agent system
**Depends on**: Phase 15
**Requirements**: PSKL-01, PSKL-02, PSKL-03, PSKL-04, PSKL-05, PSKL-06
**Success Criteria** (what must be TRUE):
  1. Bundled YAML skills installed to `~/.tamux/skills/plugins/<plugin-name>/` on plugin install
  2. Skills removed on plugin uninstall with no orphans
  3. Skills can reference plugin API endpoints (e.g., `plugin:gmail-calendar:list_events`)
  4. Plugin commands registered as namespaced slash commands (`/pluginname.command`)
  5. Agent discovers plugin skills through standard skill system (no special awareness needed)
**Plans:** 1/2 plans executed

Plans:
- [x] 19-01-PLAN.md — Skill bundling module, command registry, IPC messages, and agent command dispatch
- [ ] 19-02-PLAN.md — CLI plugin commands subcommand and agent system prompt plugin skills awareness

### Phase 20: Gmail/Calendar Validation Plugin
**Goal**: A real Gmail/Calendar plugin that proves the entire plugin ecosystem works end-to-end — from install to agent answering "what's on my calendar?"
**Depends on**: Phase 14, 15, 16, 17, 18, 19
**Requirements**: GMAI-01, GMAI-02, GMAI-03, GMAI-04, GMAI-05, GMAI-06, GMAI-07, GMAI-08, GMAI-09
**Success Criteria** (what must be TRUE):
  1. Plugin published to npm as `tamux-plugin-gmail-calendar`
  2. `tamux plugin install tamux-plugin-gmail-calendar` installs and registers successfully
  3. Google OAuth2 flow completes: user clicks "Connect", authenticates with Google, token stored
  4. Agent answers "what's on my calendar today?" with real calendar data
  5. Agent answers "what's in my inbox?" with real email subjects/senders
  6. `/gmail.inbox` and `/calendar.today` commands work from all surfaces
  7. Plugin configurable in Plugins tab in both TUI and Electron
**Plans:** TBD
**UI hint**: yes

## Progress

**Execution Order:**
v1.0: Phases 1-13 (complete). v2.0: Phase 14 first, then 15-17 can overlap, 18 after 17, 19 after 15, 20 last.

```
Phase 14 (Manifest & Loader) ─────┬──► Phase 15 (CLI & Install) ──► Phase 19 (Skills & Commands) ──┐
                                   ├──► Phase 16 (Settings UI)                                       ├──► Phase 20 (Gmail/Calendar)
                                   └──► Phase 17 (API Proxy) ──► Phase 18 (OAuth2) ─────────────────┘
```

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| **v1.0** | | | |
| 1. Production Foundation | 3/3 | Complete | 2026-03-23 |
| 2. Core Heartbeat | 4/4 | Complete | 2026-03-23 |
| 3. Transparent Autonomy | 5/5 | Complete | 2026-03-23 |
| 4. Adaptive Heartbeat | 3/3 | Complete | 2026-03-23 |
| 5. Memory Consolidation | 4/4 | Complete | 2026-03-23 |
| 6. Skill Discovery | 3/3 | Complete | 2026-03-23 |
| 7. Community Skills | 3/3 | Complete | 2026-03-23 |
| 8. Gateway Completion | 4/4 | Complete | 2026-03-23 |
| 9. Distribution | 4/4 | Complete | 2026-03-23 |
| 10. Progressive UX | 5/5 | Complete | 2026-03-24 |
| 11. Setup Wizard Rewrite | 2/2 | Complete | 2026-03-24 |
| 12. CLI Polish | 2/2 | Complete | 2026-03-24 |
| 13. TUI UX Fixes | 3/3 | Complete | 2026-03-24 |
| **v2.0** | | | |
| 14. Plugin Manifest & Loader | 2/2 | Complete    | 2026-03-24 |
| 15. Plugin CLI & Install | 2/2 | Complete    | 2026-03-24 |
| 16. Plugin Settings UI | 3/3 | Complete    | 2026-03-24 |
| 17. API Proxy Layer | 2/2 | Complete   | 2026-03-24 |
| 18. OAuth2 Flow | 0/? | Not started | - |
| 19. Plugin Skills & Commands | 1/2 | In Progress|  |
| 20. Gmail/Calendar Validation | 0/? | Not started | - |
