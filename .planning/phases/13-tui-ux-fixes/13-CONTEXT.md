# Phase 13: TUI UX Fixes - Context

**Gathered:** 2026-03-24
**Status:** Ready for planning

<domain>
## Phase Boundary

Fix 4 TUI issues from UAT testing + 1 tech debt item + StatusBar re-integration. Concierge renders in conversation, tier/feature settings added to TUI, recent_actions in sidebar, and Phase 10 status/tier additions re-applied to redesigned shadcn StatusBar.

</domain>

<decisions>
## Implementation Decisions

### Concierge Onboarding Location
- **D-01:** Concierge onboarding message renders as a regular chat message in the concierge thread, NOT in a separate overlay panel. Uses existing message rendering infrastructure. No clipping possible. User can scroll back to it. Feels natural — the agent is "talking" to you.
- **D-02:** Remove or deprecate the dedicated concierge overlay panel. The concierge widget (`widgets/concierge.rs`) can still render action buttons inline with the message, but the content itself goes through the chat message pipeline.

### TUI Feature Settings
- **D-03:** TUI advanced settings tab must include controls for:
  1. **Heartbeat:** cron interval, quiet hours window, toggle individual check types (Phase 2/4)
  2. **Memory & learning:** consolidation toggle, decay half-life, heuristic promotion threshold (Phase 5)
  3. **Skills:** auto-discovery toggle, promotion threshold, skill feed visibility (Phase 6)
  4. **Tier & security:** capability tier override selector (D-03 from Phase 10), security/approval level (Phase 11)
- **D-04:** All settings read/write via daemon IPC (AgentGetConfig/AgentSetConfigItem). Match the Electron SettingsPanel's feature coverage.

### TUI Recent Actions
- **D-05:** TUI sidebar displays recent autonomous actions from AgentStatusResponse. Add a brief section below the existing status line showing last 3 actions with one-line summaries. Poll via AgentStatusQuery (same as Electron's 10s polling).

### StatusBar Re-Integration (Electron)
- **D-06:** Re-add Phase 10 status/tier additions to the new shadcn-based StatusBar.tsx:
  - Import useStatusStore and useTierStore
  - Add tier Badge (capitalize tier name, show for Familiar+)
  - Add activity StatusIndicator from status store (idle/thinking/executing/etc.)
  - Add recent actions tooltip (last action summary on hover)
  - Add provider health warning indicator
  - Use shadcn Badge/Tooltip components to match the redesigned component style

### Claude's Discretion
- Exact TUI settings widget layout (tabs vs scrollable list)
- How to render action buttons inline with concierge chat messages
- AgentStatusQuery polling interval in TUI (match Electron's 10s or different)
- StatusBar layout details for new indicators

</decisions>

<canonical_refs>
## Canonical References

### TUI
- `crates/amux-tui/src/widgets/concierge.rs` — Current concierge overlay widget (to be deprecated/moved)
- `crates/amux-tui/src/state/concierge.rs` — Concierge state (welcome_visible, actions)
- `crates/amux-tui/src/widgets/sidebar.rs` — Sidebar with status indicators (add recent_actions)
- `crates/amux-tui/src/app/mod.rs` — TUI settings tab rendering, key handlers
- `crates/amux-tui/src/state/settings.rs` — Settings state

### Electron
- `frontend/src/components/StatusBar.tsx` — Redesigned with shadcn (needs Phase 10 re-integration)
- `frontend/src/lib/statusStore.ts` — Status store (already created in Phase 10)
- `frontend/src/lib/tierStore.ts` — Tier store (already created in Phase 10)

### Protocol
- `crates/amux-protocol/src/messages.rs` — AgentStatusQuery, AgentGetConfig, AgentSetConfigItem

### UAT Feedback
- `.planning/v1.0-UAT-FEEDBACK.md` §TUI — All 4 TUI issues

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `statusStore.ts` and `tierStore.ts` — Already created in Phase 10, just need importing in new StatusBar
- `state/tier.rs` — TUI TierState with boolean feature flags (Phase 10), use for settings visibility gating
- `state/settings.rs` — Existing TUI settings state, extend with new feature tabs
- `widgets/chat.rs` — Chat message rendering pipeline, use for concierge messages

### Integration Points
- `concierge.rs` (daemon) — deliver_onboarding writes to concierge thread; TUI needs to render these as chat messages
- `sidebar.rs` — Add AgentStatusQuery polling and recent_actions section
- `StatusBar.tsx` — Add store imports and new indicators to existing shadcn component

</code_context>

<specifics>
## Specific Ideas

- Concierge in conversation thread means the onboarding feels like the agent greeting you, not a product popup
- Feature settings should mirror Electron SettingsPanel coverage so both clients feel equivalent
- StatusBar shadcn re-integration uses Badge for tier, Tooltip for recent actions — matches the redesign aesthetic

</specifics>

<deferred>
## Deferred Ideas

- Gateway settings in TUI (Slack/Discord/Telegram tokens) — complex multi-field forms, better suited for Electron or `tamux settings`
- TUI concierge action button mouse interaction improvements
- Visual theme customization in TUI settings

None — discussion stayed within phase scope

</deferred>

---

*Phase: 13-tui-ux-fixes*
*Context gathered: 2026-03-24*
