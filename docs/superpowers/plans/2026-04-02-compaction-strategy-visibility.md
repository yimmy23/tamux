# Compaction Strategy And Visibility Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add operator-selectable compaction strategies, dedicated non-chat compaction model settings, and persisted inline compaction artifacts that render in both TUI and React while leaving original messages visible.

**Architecture:** The daemon remains the source of truth for compaction triggering and message history. It gains a strategy-aware compaction pipeline plus a persisted `compaction_artifact` message kind that stores both a visible artifact body and a hidden runtime payload. TUI and React extend their settings and chat rendering layers to edit the new config and render compaction artifacts as full-width separator rows.

**Tech Stack:** Rust workspace (`amux-daemon`, `amux-tui`, `amux-protocol`), TypeScript/React frontend, existing daemon config persistence, existing agent event and thread models.

---

### Task 1: Add daemon config types for compaction strategy and dedicated provider blocks

**Files:**
- Modify: `crates/amux-daemon/src/agent/types/config_core.rs`
- Modify: `crates/amux-daemon/src/agent/types/runtime_config.rs`
- Modify: `crates/amux-tui/src/state/config.rs`
- Modify: `frontend/src/lib/agentStore/settings.ts`
- Modify: `frontend/src/lib/agentStore/types.ts`
- Modify: `frontend/src/lib/agentDaemonConfig.ts`
- Test: `crates/amux-daemon/src/agent/compaction/tests.rs`

- [ ] **Step 1: Write the failing daemon config tests**

Add targeted tests in `crates/amux-daemon/src/agent/compaction/tests.rs` or a nearby config test module that expect strategy/default config roundtrip for:

```rust
strategy == heuristic
weles.provider/model/reasoning_effort
custom_model.provider/base_url/auth_source/api_transport/model/context_window_tokens/reasoning_effort
```

- [ ] **Step 2: Run the targeted daemon tests and verify failure**

Run: `cargo test -p amux-daemon compaction -- --nocapture`

Expected: FAIL because the new config types do not exist yet.

- [ ] **Step 3: Add the daemon config structs and defaults**

Implement small config structs/enums in:

1. `crates/amux-daemon/src/agent/types/config_core.rs`
2. `crates/amux-daemon/src/agent/types/runtime_config.rs`

Include serde defaults so older configs still load cleanly and default to `heuristic`.

- [ ] **Step 4: Mirror the new config shape into TUI and React state**

Update:

1. `crates/amux-tui/src/state/config.rs`
2. `frontend/src/lib/agentStore/settings.ts`
3. `frontend/src/lib/agentStore/types.ts`
4. `frontend/src/lib/agentDaemonConfig.ts`

So both clients can edit and serialize the same daemon config shape.

- [ ] **Step 5: Re-run the daemon tests**

Run: `cargo test -p amux-daemon compaction -- --nocapture`

Expected: PASS for config serialization/default coverage.

### Task 2: Add compaction artifact message metadata to daemon, TUI, and React

**Files:**
- Modify: `crates/amux-daemon/src/agent/types/thread_message_types.rs`
- Modify: `crates/amux-tui/src/wire.rs`
- Modify: `frontend/src/lib/agentStore/types.ts`
- Modify: `frontend/src/lib/agentStore/history.ts`
- Test: `crates/amux-daemon/src/agent/compaction/tests.rs`

- [ ] **Step 1: Write the failing daemon message-model test**

Add a test that expects a persisted message to preserve:

```rust
message_kind = compaction_artifact
compaction_strategy = ...
compaction_payload = Some(...)
content = visible_body_only
```

- [ ] **Step 2: Run the targeted daemon test and verify failure**

Run: `cargo test -p amux-daemon compaction -- --nocapture`

Expected: FAIL because the message model lacks compaction fields.

- [ ] **Step 3: Extend the daemon message model**

Update `crates/amux-daemon/src/agent/types/thread_message_types.rs` with:

1. a `message_kind` enum
2. compaction strategy metadata
3. hidden runtime payload field(s)

Keep normal message defaults unchanged.

- [ ] **Step 4: Extend TUI/React wire and store models**

Update:

1. `crates/amux-tui/src/wire.rs`
2. `frontend/src/lib/agentStore/types.ts`
3. `frontend/src/lib/agentStore/history.ts`

So compaction artifact messages hydrate correctly from daemon/persistence.

- [ ] **Step 5: Re-run the targeted daemon test**

Run: `cargo test -p amux-daemon compaction -- --nocapture`

Expected: PASS.

### Task 3: Implement strategy-aware compaction execution and fallback behavior

**Files:**
- Modify: `crates/amux-daemon/src/agent/compaction.rs`
- Modify: `crates/amux-daemon/src/agent/provider_resolution.rs`
- Modify: `crates/amux-daemon/src/agent/work_context.rs`
- Test: `crates/amux-daemon/src/agent/compaction/tests.rs`

- [ ] **Step 1: Write the failing strategy behavior tests**

Add tests for:

1. `heuristic` producing visible content `rule based`
2. `weles` producing a compaction artifact and runtime payload
3. `custom_model` producing a compaction artifact and runtime payload
4. `weles` fallback to `heuristic`
5. `custom_model` fallback to `heuristic`
6. request preparation using compaction payload instead of visible placeholder text

- [ ] **Step 2: Run the targeted daemon tests and verify failure**

Run: `cargo test -p amux-daemon compaction -- --nocapture`

Expected: FAIL because strategy-aware compaction does not exist yet.

- [ ] **Step 3: Refactor compaction into explicit run output**

In `crates/amux-daemon/src/agent/compaction.rs`, split the current path into:

1. candidate detection
2. strategy execution
3. artifact construction
4. request message synthesis

The result object should carry both visible content and the runtime compaction payload.

- [ ] **Step 4: Implement `heuristic`, `weles`, and `custom_model` resolution**

Use:

1. existing heuristic logic for `heuristic`
2. WELES-specific compaction config for `weles`
3. the separate compaction provider block for `custom_model`

Keep provider/model resolution local to compaction rather than mutating main chat config.

- [ ] **Step 5: Add fallback behavior and workflow notices**

If `weles` or `custom_model` fails:

1. emit a workflow notice
2. fall back to `heuristic`
3. persist only the successful fallback artifact

- [ ] **Step 6: Re-run the daemon tests**

Run: `cargo test -p amux-daemon compaction -- --nocapture`

Expected: PASS.

### Task 4: Persist compaction artifacts into thread history without removing original messages

**Files:**
- Modify: `crates/amux-daemon/src/agent/agent_loop/send_message/loop_core.rs`
- Modify: `crates/amux-daemon/src/agent/agent_loop/send_message/setup.rs`
- Modify: `crates/amux-daemon/src/agent/thread_crud.rs`
- Modify: `crates/amux-daemon/src/agent/persistence.rs`
- Test: `crates/amux-daemon/src/agent/tests/messaging/part3.rs`

- [ ] **Step 1: Write the failing thread-history test**

Add a test that verifies:

1. compaction inserts a `compaction_artifact` message into the thread
2. original older messages remain visible
3. artifact ordering matches the compaction boundary

- [ ] **Step 2: Run the targeted daemon messaging test and verify failure**

Run: `cargo test -p amux-daemon messaging -- --nocapture`

Expected: FAIL because no compaction artifact is persisted.

- [ ] **Step 3: Insert the persisted artifact at compaction time**

Update the send-message flow so successful compaction writes the artifact into the live thread and persisted thread storage once per compaction run, without deleting original messages.

- [ ] **Step 4: Re-run the targeted daemon messaging test**

Run: `cargo test -p amux-daemon messaging -- --nocapture`

Expected: PASS.

### Task 5: Expose strategy and conditional provider editors in TUI

**Files:**
- Modify: `crates/amux-tui/src/state/config.rs`
- Modify: `crates/amux-tui/src/state/settings.rs`
- Modify: `crates/amux-tui/src/app/config_io.rs`
- Modify: `crates/amux-tui/src/app/config_io_helpers.rs`
- Modify: `crates/amux-tui/src/app/settings_handlers/impl_part4.rs`
- Modify: `crates/amux-tui/src/app/settings_handlers/impl_part5.rs`
- Modify: `crates/amux-tui/src/app/settings_handlers/impl_part6.rs`
- Modify: `crates/amux-tui/src/app/modal_handlers.rs`
- Modify: `crates/amux-tui/src/app/modal_handlers_enter.rs`
- Test: `crates/amux-tui/src/app/tests/config_io.rs`
- Test: `crates/amux-tui/src/state/tests/settings.rs`

- [ ] **Step 1: Write the failing TUI settings tests**

Add tests that expect:

1. strategy field presence
2. WELES compaction config roundtrip
3. custom-model compaction config roundtrip
4. settings cursor/navigation support for the new fields

- [ ] **Step 2: Run the targeted TUI tests and verify failure**

Run: `cargo test -p amux-tui config_io settings -- --nocapture`

Expected: FAIL because the new fields are not wired.

- [ ] **Step 3: Add the new settings fields and serialization**

Wire the compaction strategy and conditional configs through TUI state, config loading, and config patch generation.

- [ ] **Step 4: Add editing support in settings handlers**

Expose:

1. strategy selector
2. WELES provider/model/reasoning editor
3. custom-model provider/base-url/auth/transport/model/context-window/reasoning editor

- [ ] **Step 5: Re-run the targeted TUI tests**

Run: `cargo test -p amux-tui config_io settings -- --nocapture`

Expected: PASS.

### Task 6: Render compaction artifacts as full-width rows in TUI

**Files:**
- Modify: `crates/amux-tui/src/wire.rs`
- Modify: `crates/amux-tui/src/app/events.rs`
- Modify: `crates/amux-tui/src/app/events/events_activity.rs`
- Modify: `crates/amux-tui/src/app/rendering.rs`
- Modify: `crates/amux-tui/src/widgets/sidebar.rs`
- Modify: `crates/amux-tui/src/widgets/landing.rs`
- Test: `crates/amux-tui/src/app/tests/events.rs`

- [ ] **Step 1: Write the failing TUI render/event tests**

Add tests that expect a compaction artifact message to render as:

```text
---- auto compaction ----
rule based / compacted text
------------------------
```

and not as a normal assistant bubble.

- [ ] **Step 2: Run the targeted TUI tests and verify failure**

Run: `cargo test -p amux-tui events -- --nocapture`

Expected: FAIL because compaction artifacts render like normal messages or are unknown.

- [ ] **Step 3: Add compaction-aware rendering**

Update TUI event hydration and rendering so compaction artifact messages produce the dedicated full-width row while preserving search/hydration behavior.

- [ ] **Step 4: Re-run the targeted TUI tests**

Run: `cargo test -p amux-tui events -- --nocapture`

Expected: PASS.

### Task 7: Expose compaction strategy and dedicated provider config in React settings

**Files:**
- Modify: `frontend/src/lib/agentStore/settings.ts`
- Modify: `frontend/src/lib/agentDaemonConfig.ts`
- Modify: `frontend/src/components/settings-panel/AgentTab.tsx`
- Test: `frontend/src/lib/agentStore/protectedSubagents.spec.ts`

- [ ] **Step 1: Write the failing React settings test**

Add or extend a focused frontend store/config test that expects the new compaction settings to serialize into the daemon config payload correctly.

- [ ] **Step 2: Run the focused frontend test and verify failure**

Run: `npm test -- --runInBand protectedSubagents.spec.ts`

Expected: FAIL because the compaction config is not part of the serialized settings yet.

- [ ] **Step 3: Extend React settings state and daemon config builder**

Add:

1. strategy state
2. WELES compaction config state
3. custom-model compaction provider state

and serialize them in `frontend/src/lib/agentDaemonConfig.ts`.

- [ ] **Step 4: Render the new controls in the settings panel**

Update `frontend/src/components/settings-panel/AgentTab.tsx` to show the strategy selector and the conditional WELES/custom-model editors.

- [ ] **Step 5: Re-run the focused frontend test**

Run: `npm test -- --runInBand protectedSubagents.spec.ts`

Expected: PASS or replace this with the exact new focused test path you added.

### Task 8: Render compaction artifacts in React chat view

**Files:**
- Modify: `frontend/src/components/agent-chat-panel/chat-view/helpers.ts`
- Modify: `frontend/src/components/agent-chat-panel/chat-view/types.ts`
- Modify: `frontend/src/components/agent-chat-panel/ChatView.tsx`
- Modify: `frontend/src/components/agent-chat-panel/chat-view/MessageBubble.tsx`
- Create: `frontend/src/components/agent-chat-panel/chat-view/CompactionArtifactRow.tsx`

- [ ] **Step 1: Write the failing React rendering test**

Add a focused test or local render assertion that expects compaction artifacts to render as a separator row instead of a normal message bubble.

- [ ] **Step 2: Run the targeted React render test and verify failure**

Run: `npm test -- --runInBand <new-compaction-render-test>`

Expected: FAIL because compaction artifacts are currently treated as ordinary messages.

- [ ] **Step 3: Add a dedicated compaction display item and row component**

Update display-item construction so compaction artifacts become their own item type, then render them with a new `CompactionArtifactRow` component using the agreed full-width format.

- [ ] **Step 4: Re-run the targeted React render test**

Run: `npm test -- --runInBand <new-compaction-render-test>`

Expected: PASS.

### Task 9: Run verification and fix any integration breakage

**Files:**
- Modify: Any files required by failures from verification

- [ ] **Step 1: Run daemon compaction/messaging coverage**

Run: `cargo test -p amux-daemon compaction messaging -- --nocapture`

Expected: PASS.

- [ ] **Step 2: Run TUI coverage for config and events**

Run: `cargo test -p amux-tui config_io events settings -- --nocapture`

Expected: PASS.

- [ ] **Step 3: Run frontend verification**

Run: `cd frontend && npm run lint`

Expected: PASS.

- [ ] **Step 4: Run frontend build**

Run: `cd frontend && npm run build`

Expected: PASS.

- [ ] **Step 5: Fix any breakages and re-run the relevant command**

Only patch the files implicated by the failing command output.
