# Phase 11: Setup Wizard Rewrite - Context

**Gathered:** 2026-03-24
**Status:** Ready for planning

<domain>
## Phase Boundary

Rewrite the setup wizard to correctly configure the daemon via IPC (not config.json), sync with the daemon's actual provider list, use arrow-key navigation, offer tier-appropriate optional setup steps, and set security defaults based on tier + user preference. This is a gap closure phase fixing UAT issues from v1.0 testing.

**CRITICAL CONSTRAINT:** The daemon reads ALL config from its SQLite database, NOT from config.json. The entire setup wizard config-write path must go through daemon IPC (ClientMessage::AgentSetConfigItem or equivalent). config.json must not be written or referenced as a daemon config source.

</domain>

<decisions>
## Implementation Decisions

### Config Write Path
- **D-01:** Daemon starts first with empty/default config (no provider set). Wizard connects via IPC socket, sets provider/key/etc via AgentSetConfigItem messages. Daemon works with no provider — it just can't do LLM calls until one is set.
- **D-02:** Wizard detects if daemon is already running and connects to existing socket. If not running, wizard starts the daemon first, waits for socket, then connects. `tamux setup` works both for first-run and reconfiguration.
- **D-03:** Remove ALL config.json references from the wizard. No JSON file writes for daemon config. The old `setup_wizard.rs` atomic-write-to-config.json code must be replaced with IPC calls.

### Wizard UX and Navigation
- **D-04:** Crossterm-powered arrow-key select lists. Arrow keys move highlight, Enter confirms, Esc goes back. Clean but not full TUI — each step is a focused screen. Like `cargo init` or `npm init` style.
- **D-05:** Provider and API key steps are required (wizard won't proceed without them). Optional steps (web search, gateway, model, data dir) can be skipped with Esc or a 'Skip' option. User can always re-run `tamux setup` later.
- **D-06:** Keep the tier self-assessment question from Phase 10 (D-06): "How familiar are you with AI agents?" → maps to Newcomer/Familiar/Power User/Expert. This is set via IPC to daemon.

### Provider List and Optional Steps
- **D-07:** Wizard queries daemon via IPC for available providers. Daemon returns its actual provider definitions. Single source of truth — never out of sync. No hardcoded provider list in wizard.
- **D-08:** Wizard flow after required steps (tier, provider, API key):
  1. Default model selection (pick from provider's available models) — optional, skippable
  2. Web search tool API key (firecrawl/exa/tavily) — optional, skippable
  3. Gateway setup (Slack/Discord/Telegram tokens) — optional, skippable, only offered at Familiar+ tier
  4. Data directory confirmation (~/.tamux/ or custom) — optional, skippable
  5. Connectivity test — required (validates provider + key work)
- **D-09:** Advanced users (Power User/Expert tier from self-assessment) see all optional steps. Newcomers see only provider, key, and connectivity test. Familiar sees everything except gateway.

### Newcomer Security Defaults
- **D-10:** Newcomers must approve ALL agent actions: shell commands, file writes, file deletes, web requests, tool calls. Maximum safety. Tier-based relaxation: Familiar skips read-only tools, Power User skips safe tools, Expert approves only destructive actions.
- **D-11:** Wizard includes a separate security preference question: "How cautious should tamux be?" with options like "Ask me before doing anything" / "Ask for risky actions only" / "I trust it, minimize interruptions". This is independent from tier and set via IPC to daemon's policy config.

### Claude's Discretion
- Exact crossterm rendering details (colors, border style, prompt formatting)
- Which daemon IPC messages to use for each config item (AgentSetConfigItem vs dedicated messages)
- How to handle daemon startup timeout/failure in the wizard
- Connectivity test implementation details (reuse existing or simplify)
- Exact wording of security preference options

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Critical Architecture
- `.planning/v1.0-UAT-FEEDBACK.md` — All 16 UAT issues, especially Setup Wizard section (items 1-5) and TUI item 3 (config.json vs DB)
- `crates/amux-daemon/src/agent/config.rs` — AgentConfig struct, provider definitions, how daemon actually reads config
- `crates/amux-daemon/src/history.rs` — SQLite DB where daemon config is persisted
- `crates/amux-daemon/src/server.rs` — AgentSetConfigItem handler, how IPC config updates work

### Existing Wizard (to be rewritten)
- `crates/amux-cli/src/setup_wizard.rs` — Current wizard implementation (writes config.json — WRONG, must be rewritten to use IPC)
- `crates/amux-cli/src/main.rs` — CLI entry point, needs_setup() detection, Setup subcommand
- `crates/amux-cli/src/client.rs` — IPC client, AgentBridgeCommand enum, socket connection logic

### Protocol
- `crates/amux-protocol/src/messages.rs` — ClientMessage::AgentSetConfigItem, AgentGetConfig, AgentSetTierOverride
- `crates/amux-daemon/src/agent/types.rs` — AgentConfig, ProviderConfig, TierConfig, PolicyConfig

### Phase 10 Tier System
- `crates/amux-daemon/src/agent/capability_tier.rs` — CapabilityTier enum, TierConfig, resolve_tier()
- `crates/amux-daemon/src/agent/operator_model.rs` — OperatorModel (signals for tier)

### UX Reference
- `.planning/good_ux.md` — Personas (R/A), autonomy ↔ control trade-off, security defaults philosophy

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `client.rs` IPC connection logic — already connects to daemon socket, sends/receives messages. Reuse for wizard IPC.
- `AgentSetConfigItem` message — already exists in protocol for setting individual config values via IPC.
- `AgentGetConfig` message — already exists for reading current config. Use to query provider list.
- `crossterm` — already a dependency in amux-cli (added in Phase 10 for setup wizard). Reuse for arrow-key navigation.

### Integration Points
- `setup_wizard.rs` — Complete rewrite target. Replace config.json writes with IPC calls.
- `main.rs` — `needs_setup()` detection needs to check daemon DB state (is provider set?), not config.json existence.
- `server.rs` — May need new IPC messages for "list available providers" if AgentGetConfig doesn't return provider definitions.

### Patterns to Follow
- Electron frontend already configures daemon via IPC (`agentSetConfigItem` in preload.cjs → main.cjs → bridge subprocess → daemon). The CLI wizard should follow the exact same pattern.
- TUI settings panel reads/writes config via daemon IPC. Same approach for CLI wizard.

</code_context>

<specifics>
## Specific Ideas

- The wizard should feel like a natural terminal experience (cargo init style), not a form
- Provider list must come from daemon at runtime — this is the single source of truth fix
- Security question is separate from tier because an expert might still want high caution on a new machine
- `tamux setup` should be the go-to command for both first-run and reconfiguration
- Remove all references to config.json as a daemon config source from CLAUDE.md and code comments

</specifics>

<deferred>
## Deferred Ideas

- Wizard in Electron (React component version) — would be nice but CLI-first is the priority
- OAuth flow for providers that support it — complexity, defer to future
- Import config from another machine — nice for power users but not first-run priority

None — discussion stayed within phase scope

</deferred>

---

*Phase: 11-setup-wizard-rewrite*
*Context gathered: 2026-03-24*
