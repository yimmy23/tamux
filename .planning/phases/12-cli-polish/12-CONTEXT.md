# Phase 12: CLI Polish - Context

**Gathered:** 2026-03-24
**Status:** Ready for planning

<domain>
## Phase Boundary

Fix 7 CLI issues from UAT testing: add launch commands, fix broken stats, hide internal commands, show audit IDs, and add settings subcommand. All changes are in `crates/amux-cli/src/main.rs` and `crates/amux-cli/src/client.rs`.

</domain>

<decisions>
## Implementation Decisions

### Launch Commands
- **D-01:** `tamux tui` spawns `tamux-tui` binary. `tamux gui` spawns the Electron app. Both detect binary location (same directory as tamux binary, then PATH). Pass remaining args through. Print clear error if binary not found.

### Settings Subcommand
- **D-02:** Key-value get/set pattern like git config: `tamux settings list` shows all config. `tamux settings get <key>` reads one value. `tamux settings set <key> <value>` writes one value. All operations use IPC to daemon (daemon reads config from DB, not files).
- **D-03:** Settings keys use dot-notation matching daemon's config structure: `heartbeat.interval`, `heartbeat.quiet_hours`, `provider`, `model`, `tier.user_override`, `gateway.slack_token`, etc.

### Internal Commands
- **D-04:** Hide `attach`, `new`, and `scrub` from `tamux --help` using `#[clap(hide = true)]`. Commands still work for backward compatibility but don't clutter the help output.

### Fix Stats
- **D-05:** Fix `tamux stats` protocol deserialization error. The error "tag for enum is not valid, found 117" indicates a protocol version mismatch — likely new DaemonMessage variants added in Phases 9-10 that the CLI doesn't handle. Add catch-all handling for unknown variants.

### Fix Audit IDs
- **D-06:** `tamux audit` list output must show entry IDs in each row so users can use `--detail <id>`. Format: `[id] [timestamp] [type] description`.

### Claude's Discretion
- Exact binary detection logic (same-dir vs PATH precedence)
- Settings list formatting (table vs key=value)
- Stats deserialization fix approach (protocol version check vs catch-all)

</decisions>

<canonical_refs>
## Canonical References

### CLI Crate
- `crates/amux-cli/src/main.rs` — CLI entry point, clap Commands enum, all subcommand handlers
- `crates/amux-cli/src/client.rs` — IPC connection, AgentBridgeCommand enum, message dispatch
- `.planning/v1.0-UAT-FEEDBACK.md` §CLI — All 7 CLI issues

### Protocol
- `crates/amux-protocol/src/messages.rs` — ClientMessage/DaemonMessage enums (stats error likely from new variants)

</canonical_refs>

<code_context>
## Existing Code Insights

### Integration Points
- `main.rs` Commands enum — add Tui, Gui, Settings variants; add `#[clap(hide = true)]` to Attach, New, Scrub
- `client.rs` — settings get/set needs IPC roundtrip via AgentGetConfig/AgentSetConfigItem
- Protocol `DaemonMessage` — stats handler needs to handle unknown variants gracefully

</code_context>

<specifics>
## Specific Ideas

- `tamux settings` should feel like `git config` — familiar to developers
- Binary detection: check `std::env::current_exe()` parent dir first, then `which::which()`
- Audit ID display: prepend numeric ID to each audit entry line

</specifics>

<deferred>
## Deferred Ideas

None — all 7 issues are in scope

</deferred>

---

*Phase: 12-cli-polish*
*Context gathered: 2026-03-24*
