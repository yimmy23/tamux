# DB-Backed Provider and Model Statistics Design

**Date:** 2026-04-12

## Goal

Add daemon-backed usage statistics sourced from SQLite history so both the TUI and React UI can show:

- per-provider token totals: input, output, total, and cost,
- detailed per-model usage with provider attribution,
- top 5 models ranked by total tokens descending,
- top 5 models ranked by total cost descending,
- shared time-window filters: `today`, `7d`, `30d`, `all`.

The TUI must expose this via a dedicated modal opened by `/statistics` and command-palette entry `statistics`, with clickable tabs, clickable filters, and scrollable content. The React app must extend the existing `Usage` surface with a daemon-backed statistics subview instead of deriving the new statistics from loaded messages.

## Current State

### Daemon and History

- Persisted thread/message history lives in SQLite via [`crates/amux-daemon/src/history`](../../../crates/amux-daemon/src/history).
- `agent_messages` already stores:
  - `provider`
  - `model`
  - `input_tokens`
  - `output_tokens`
  - `total_tokens`
- Per-message cost is not currently persisted in SQLite. The live `Done` event includes `cost`, but history rows do not.
- Goal-run cost tracking exists, but that is not sufficient for model/provider rankings because the new view must aggregate historical message-level data.

### TUI

- Slash commands are parsed in [`crates/amux-tui/src/app/keyboard_enter.rs`](../../../crates/amux-tui/src/app/keyboard_enter.rs).
- Built-in command execution is handled in [`crates/amux-tui/src/app/commands.rs`](../../../crates/amux-tui/src/app/commands.rs).
- Modal types live in [`crates/amux-tui/src/state/modal.rs`](../../../crates/amux-tui/src/state/modal.rs).
- There is already a modal stack, command palette, mouse handling, and scrollable modal infrastructure that can be extended.

### React / Electron

- The current usage UI is [`frontend/src/components/agent-chat-panel/UsageView.tsx`](../../../frontend/src/components/agent-chat-panel/UsageView.tsx).
- That view currently derives usage from loaded thread/message state, not from daemon history.
- Electron already proxies daemon requests such as `agent-get-status` through:
  - [`frontend/electron/main/agent-ipc-handlers.cjs`](../../../frontend/electron/main/agent-ipc-handlers.cjs)
  - [`frontend/electron/preload.cjs`](../../../frontend/electron/preload.cjs)

## Product Decisions

### Source of Truth

Statistics come from persisted SQLite history, not from in-memory thread state.

### UI Placement

- React: extend the existing `Usage` tab with a new statistics subview.
- TUI: add a new dedicated `Statistics` modal.

### Filters

Both UIs support the same windows:

- `today`
- `7d`
- `30d`
- `all`

### Historical Cost Gaps

Older rows without persisted cost will be treated as `0` for cost aggregation and rankings, and the daemon will surface a `has_incomplete_cost_history` flag so both UIs can display a warning that historical spend totals may be partial.

This avoids fake recomputation from mutable rate cards while keeping the feature usable immediately.

## Proposed Architecture

### 1. Persist per-message cost in history

Extend `agent_messages` with a nullable `cost_usd` column.

Files likely affected:

- [`crates/amux-daemon/src/history/schema_sql.rs`](../../../crates/amux-daemon/src/history/schema_sql.rs)
- [`crates/amux-daemon/src/history/schema_migrations.rs`](../../../crates/amux-daemon/src/history/schema_migrations.rs)
- [`crates/amux-daemon/src/history/row_mapping.rs`](../../../crates/amux-daemon/src/history/row_mapping.rs)
- [`crates/amux-daemon/src/history/threads.rs`](../../../crates/amux-daemon/src/history/threads.rs)
- [`crates/amux-protocol/src/messages/support.rs`](../../../crates/amux-protocol/src/messages/support.rs)
- [`crates/amux-daemon/src/agent/metadata.rs`](../../../crates/amux-daemon/src/agent/metadata.rs)
- [`crates/amux-daemon/src/agent/persistence/save.rs`](../../../crates/amux-daemon/src/agent/persistence/save.rs)
- [`crates/amux-daemon/src/agent/persistence.rs`](../../../crates/amux-daemon/src/agent/persistence.rs)

Design notes:

- Keep `cost_usd` as nullable in storage so pre-migration rows remain valid.
- Store cost as a first-class DB column instead of burying it inside JSON metadata. This keeps queries simple and index-friendly.
- Thread hydration should populate message cost for any views that still use thread data.

### 2. Add daemon statistics query API

Add a dedicated statistics request/response in the daemon protocol and IPC layer.

The request should carry:

- `window`: `today | 7d | 30d | all`

The response should carry:

- totals:
  - `input_tokens`
  - `output_tokens`
  - `total_tokens`
  - `cost_usd`
  - `provider_count`
  - `model_count`
- provider rows:
  - `provider`
  - `input_tokens`
  - `output_tokens`
  - `total_tokens`
  - `cost_usd`
- model rows:
  - `provider`
  - `model`
  - `input_tokens`
  - `output_tokens`
  - `total_tokens`
  - `cost_usd`
- rankings:
  - `top_models_by_tokens`
  - `top_models_by_cost`
- diagnostics:
  - `window`
  - `generated_at`
  - `has_incomplete_cost_history`

Files likely affected:

- protocol message enums in `crates/amux-protocol/src/messages/...`
- daemon server request routing in `crates/amux-daemon/src/server/...`
- client bridges in TUI and Electron

### 3. Query aggregation in SQLite

Add history-store query helpers that aggregate directly from `agent_messages`.

Recommended query behavior:

- filter on `created_at` lower bound for `today`, `7d`, and `30d`,
- exclude rows lacking both provider and model only if they would pollute the rankings,
- coalesce null token fields to `0`,
- coalesce null cost fields to `0` for sums,
- separately count rows with null cost so `has_incomplete_cost_history` can be computed.

Suggested query split:

- one query for totals,
- one grouped query for providers,
- one grouped query for provider/model rows,
- derive both top-5 rankings from the model rows in Rust after fetch.

Deriving rankings in Rust is preferred over duplicate SQL because:

- ranking rules stay centralized,
- tie-breaking is easier to keep stable,
- both rankings can share the same normalized row set.

Stable sort rules:

- `top_models_by_tokens`: sort by `total_tokens desc`, then `cost_usd desc`, then `provider asc`, then `model asc`
- `top_models_by_cost`: sort by `cost_usd desc`, then `total_tokens desc`, then `provider asc`, then `model asc`

## TUI Design

### Modal

Add `ModalKind::Statistics`.

The modal is separate from `Status` because:

- it has its own loading lifecycle,
- it needs tab state and filter state,
- it needs rich tables/rankings rather than monospaced text blob rendering.

### Entry Points

Add:

- slash command: `/statistics`
- command-palette item: `statistics`
- help text entry in the help modal

Expected behavior:

- opening the modal triggers a daemon statistics request using the current active window,
- changing the filter triggers a refetch,
- the modal remains open while loading and renders a loading state.

### Tabs

Add four statistics tabs:

- `Overview`
- `Providers`
- `Models`
- `Rankings`

Tab contents:

- `Overview`
  - totals ribbon
  - incomplete-cost warning if needed
  - quick summaries for active window
- `Providers`
  - rows for provider totals
- `Models`
  - detailed per-model rows
- `Rankings`
  - top 5 by total tokens
  - top 5 by cost

### Interaction Model

The modal must support:

- keyboard navigation between tabs,
- keyboard navigation between filter chips,
- vertical scrolling inside the active tab content,
- mouse clicks on tabs,
- mouse clicks on filters,
- mouse wheel scrolling inside the modal content area.

State additions likely needed in the TUI model:

- statistics payload cache,
- statistics loading flag,
- statistics error message,
- active statistics tab,
- active statistics window,
- statistics scroll offset,
- per-render hitboxes for tab/filter click targets if not already reusable from current modal mouse helpers.

Files likely affected:

- `crates/amux-tui/src/state/modal.rs`
- `crates/amux-tui/src/app/commands.rs`
- `crates/amux-tui/src/app/keyboard_enter.rs`
- `crates/amux-tui/src/app/rendering.rs`
- `crates/amux-tui/src/app/mouse_helpers.rs`
- `crates/amux-tui/src/app/modal_handlers*.rs`
- new render helper module for statistics modal
- TUI wire/projection state files where daemon responses are decoded

## React Design

### Extend UsageView

Keep the current usage explorer, but add a top-level mode switch inside `UsageView`:

- `Explorer`
- `Statistics`

`Explorer` remains the current client-derived view.

`Statistics` becomes daemon-backed and uses the new IPC/API path.

### Statistics Subview

The React statistics subview should render:

- totals cards,
- shared time-window filter buttons/select,
- incomplete-cost warning banner when applicable,
- provider table,
- model table,
- top-5-by-tokens panel,
- top-5-by-cost panel.

The React view should not re-derive these aggregates from `threads` and `messagesByThread`; it should render the daemon response directly.

### Electron Bridge

Add a new preload API and IPC handler, following the existing `agentGetStatus` pattern.

Likely files:

- [`frontend/electron/main/agent-ipc-handlers.cjs`](../../../frontend/electron/main/agent-ipc-handlers.cjs)
- [`frontend/electron/preload.cjs`](../../../frontend/electron/preload.cjs)
- Type declarations for preload API
- React runtime/provider files that already load agent data

## Data Shapes

Suggested Rust-facing row types:

```rust
pub struct ProviderStatisticsRow {
    pub provider: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub cost_usd: f64,
}

pub struct ModelStatisticsRow {
    pub provider: String,
    pub model: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub cost_usd: f64,
}

pub struct AgentStatisticsSnapshot {
    pub window: String,
    pub generated_at: u64,
    pub has_incomplete_cost_history: bool,
    pub totals: AgentStatisticsTotals,
    pub providers: Vec<ProviderStatisticsRow>,
    pub models: Vec<ModelStatisticsRow>,
    pub top_models_by_tokens: Vec<ModelStatisticsRow>,
    pub top_models_by_cost: Vec<ModelStatisticsRow>,
}
```

The final names can follow the repo’s existing protocol naming conventions, but the payload should stay close to this shape.

## Migration and Backward Compatibility

- Add `cost_usd` using `ensure_column(...)` migration style.
- Older DBs continue to load.
- Old rows have null cost until touched by new writes.
- UI warns when historical cost is incomplete.
- Existing thread list/detail, status modal, and usage explorer remain functional.

## Error Handling

### Daemon

- Invalid or unknown filter values should fall back to `all` or return a typed error before query execution.
- Statistics query failures should return a clear error string to the client.

### TUI

- Show loading state while request is in flight.
- Replace content with a readable error state if the request fails.
- Keep modal open so the user can retry by changing filter or reopening.

### React

- Show loading skeleton or lightweight loading state.
- Show retry-friendly inline error.
- Preserve existing `Explorer` functionality even if statistics fetch fails.

## Testing Strategy

### Daemon

- migration test proving `cost_usd` is added safely,
- persistence test proving message cost round-trips,
- query tests for:
  - all-time totals,
  - filtered windows,
  - provider aggregation,
  - model aggregation,
  - top-5 token ranking order,
  - top-5 cost ranking order,
  - incomplete-cost-history flag.

### TUI

- `/statistics` opens statistics modal instead of sending chat,
- command-palette `statistics` path opens modal,
- filter changes trigger daemon request,
- tab switching works by keyboard,
- mouse clicks on tabs and filters switch state,
- scrolling moves the active modal content.

### React / Electron

- preload and IPC tests for new statistics bridge,
- component-level tests if existing patterns are available,
- otherwise at minimum:
  - `npm run lint`
  - `npm run build`
  - manual smoke for Usage -> Statistics subview.

## Out of Scope

- retroactive recomputation of missing historical cost from mutable rate cards,
- replacing the existing usage explorer,
- adding CSV/JSON export for the new statistics payload unless it falls out naturally from the current `UsageView` controls,
- session/thread-level statistics redesign beyond what already exists in `UsageView`.

## Recommended Implementation Order

1. Persist `cost_usd` in history rows and hydrate it back into message models.
2. Add daemon statistics query helpers and protocol request/response types.
3. Wire TUI client decoding and modal state.
4. Implement TUI `/statistics` command, modal rendering, mouse handling, and scrolling.
5. Add Electron IPC bridge and preload API.
6. Extend React `UsageView` with daemon-backed statistics subview.
7. Add focused tests and run Rust/frontend verification.

## Acceptance Criteria

- A completed LLM message persists provider, model, input tokens, output tokens, total tokens, and cost to SQLite history.
- The daemon can return statistics for `today`, `7d`, `30d`, and `all`.
- The daemon response includes provider totals, model totals, and two top-5 rankings.
- TUI supports `/statistics` and opens a dedicated tabbed, clickable, scrollable modal.
- React `Usage` view contains a statistics subview backed by daemon history, not loaded in-memory messages.
- Both UIs display the same rankings for the same filter window.
- Historical missing cost is clearly marked as partial rather than silently misrepresented as complete.
