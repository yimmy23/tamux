# Compaction Strategy And Visibility Design

Status: Proposed and user-approved
Date: 2026-04-02

## Goal

Make auto-compaction operator-visible and operator-configurable across daemon, TUI, and React. Operators must be able to choose `heuristic`, `weles`, or `custom_model` compaction, configure dedicated model settings for non-heuristic strategies, and see every successful compaction as a persisted inline pseudo-message in the thread while keeping original messages visible.

## Current Context

The daemon already has token-aware request compaction in [`crates/amux-daemon/src/agent/compaction.rs`](/home/mkurman/gitlab/it/cmux-next/crates/amux-daemon/src/agent/compaction.rs), along with existing compaction knobs such as `auto_compact_context`, `compact_threshold_pct`, and `keep_recent_on_compact`. TUI and React already expose those basic thresholds in settings, but neither surface shows when compaction actually happens. React only shows a cumulative `Compacted: Nx` counter, and the current compaction output is synthesized transiently inside request building rather than recorded as a visible thread artifact.

The thread/message model is already persisted through first-class `AgentMessage` entries, which makes a persisted pseudo-message the cleanest fit for durable operator-visible compaction history. The thread model currently has no dedicated compaction artifact subtype, and the config model currently has no strategy selector or separate compaction provider configuration.

## Architecture

### 1. Strategy-Oriented Compaction Configuration

Compaction remains enabled by the existing `auto_compact_context` gate, but configuration grows a dedicated `compaction` block that controls how compaction is performed.

The effective configuration should include:

1. `strategy: heuristic | weles | custom_model`
2. Existing threshold controls and recent-message retention controls
3. A `weles` runtime override block with provider, model, and reasoning effort
4. A `custom_model` provider block that is fully separate from the main chat provider config

`heuristic` is the default strategy and preserves existing behavior unless the operator opts into `weles` or `custom_model`.

`weles` compaction is conceptually “LLM compaction via the daemon-owned WELES runtime,” but it remains a compaction strategy, not a generic governance event.

`custom_model` compaction must not reuse the main chat provider config implicitly. It gets its own dedicated provider/model/reasoning settings so the operator can keep normal chat on one model and compaction on another.

### 2. Persisted Compaction Artifacts

Each successful compaction inserts a persisted pseudo-message into the thread. This is a real thread artifact, not a transient UI projection.

The artifact has two responsibilities:

1. Operator transparency in thread history
2. A durable marker that a compaction run happened at a specific thread boundary

The visible message body must contain only the compacted text. It must not embed strategy/provider/model metadata in the visible content.

Visible content rules:

1. `heuristic` renders `rule based`
2. `weles` renders the WELES compacted text
3. `custom_model` renders the custom-model compacted text

Original compacted messages remain visible in the thread. The artifact is inserted at the compaction boundary as an inline marker rather than replacing the original history.

### 3. Split Between Visible Artifact And Runtime Payload

The visible pseudo-message is not the sole source of truth for future request building.

The daemon should track both:

1. `visible_content` for thread rendering
2. `compaction_payload` for future request construction

This matters most for `heuristic`: the operator-visible body is only `rule based`, but the request builder still needs the real rule-based compacted payload rather than the placeholder string.

To support this cleanly, the message model should gain a compaction-specific subtype or marker plus hidden compaction metadata. The runtime should prefer the stored compaction payload when constructing later LLM requests instead of reconstructing visibility from UI-only heuristics.

### 4. Rendering Contract In TUI And React

Compaction artifacts must render as a dedicated full-width inline row rather than a normal assistant bubble.

The exact operator-facing layout is:

```text
---- auto compaction ----
<visible compacted text>
------------------------
```

For heuristic compaction:

```text
---- auto compaction ----
rule based
------------------------
```

This row should be:

1. Persisted across reloads
2. Searchable
3. Copyable
4. Visually distinct but low-noise

Both TUI and React should derive the same rendering from the persisted message subtype rather than from local string matching.

## Data Model Changes

### Agent Config

The daemon config should grow a dedicated compaction configuration section in addition to the legacy flat fields already persisted for thresholds.

Recommended shape:

1. `compaction.strategy`
2. `compaction.weles.provider`
3. `compaction.weles.model`
4. `compaction.weles.reasoning_effort`
5. `compaction.custom_model.provider`
6. `compaction.custom_model.base_url`
7. `compaction.custom_model.auth_source`
8. `compaction.custom_model.api_transport`
9. `compaction.custom_model.model`
10. `compaction.custom_model.api_key` or existing daemon-owned auth-compatible shape
11. `compaction.custom_model.context_window_tokens`
12. `compaction.custom_model.reasoning_effort`

Legacy threshold fields stay in place and continue to drive trigger conditions.

### Agent Message

The daemon `AgentMessage` model should gain explicit compaction metadata instead of overloading assistant messages.

Recommended additions:

1. `message_kind: normal | compaction_artifact`
2. `compaction_strategy: Option<...>`
3. `compaction_payload: Option<String>`
4. `compaction_split_at: Option<usize>` or equivalent boundary metadata

The visible `content` remains the operator-facing pseudo-message body.

### Client View Models

TUI wire types and React store types should mirror the new message-kind and compaction metadata needed for rendering. They do not need every daemon internal field, but they do need enough structure to distinguish a compaction artifact from a normal assistant message without string heuristics.

## Runtime Flow

When compaction triggers:

1. The daemon computes the compaction candidate boundary with current budget logic.
2. It runs the selected strategy.
3. On success, it stores the runtime compaction payload.
4. It inserts a persisted compaction artifact message into the thread.
5. It emits a workflow notice so live sessions see the event immediately.

Strategy semantics:

1. `heuristic`
   - uses the current rule-based compaction path
   - visible artifact content is `rule based`
2. `weles`
   - runs a dedicated WELES compaction request using WELES compaction settings
   - visible artifact content is the returned compacted text
3. `custom_model`
   - runs a dedicated LLM compaction request using the separate custom compaction provider config
   - visible artifact content is the returned compacted text

For future requests, the request builder should prefer the stored compaction payload for the compacted slice rather than naïvely using the pseudo-message content.

## Failure Handling

Compaction failure must not silently erase operator context or leave a fake artifact behind.

Fallback behavior:

1. `heuristic` should not hard-fail normal turns; it remains the safe fallback path
2. `weles` failure falls back to `heuristic`
3. `custom_model` failure falls back to `heuristic`

Artifact rules:

1. Persist only the artifact for the strategy that actually succeeded
2. If a non-heuristic strategy fails and heuristic fallback succeeds, the artifact body becomes `rule based`
3. Emit a workflow notice describing the failed preferred strategy and fallback

## UI Contract

### React Settings

The existing “Context Compaction” section in [`frontend/src/components/settings-panel/AgentTab.tsx`](/home/mkurman/gitlab/it/cmux-next/frontend/src/components/settings-panel/AgentTab.tsx) should grow:

1. `Compaction Strategy` selector
2. Conditional `WELES` config controls
3. Conditional `Custom Model` provider/model/reasoning controls

### TUI Settings

The TUI settings state and handlers should mirror the same strategy selector and conditional configuration editing. The TUI should persist the same daemon config shape rather than inventing an alternate local schema.

### Chat Rendering

React and TUI should both render compaction artifacts as a dedicated full-width row, not as assistant prose. Existing normal assistant rendering remains unchanged for non-compaction messages.

## Testing

Daemon tests should cover:

1. Config roundtrip for strategy and dedicated compaction configs
2. Compaction artifact insertion into thread history
3. Retention of original messages
4. Use of runtime compaction payload instead of visible placeholder text
5. `weles` fallback to `heuristic`
6. `custom_model` fallback to `heuristic`

TUI tests should cover:

1. Settings editing for strategy selection and conditional fields
2. Hydration/rendering of compaction artifact messages
3. Full-width separator rendering behavior

React tests should cover:

1. Settings serialization/deserialization for the new config
2. Chat rendering of compaction artifacts
3. Preservation across hydration and reload

## Open Constraints

The design intentionally does not add a separate compaction-history table. Thread message persistence is already the correct abstraction and keeps the feature understandable across daemon, TUI, and React.
