# Media Feature Capabilities And Thread Image Generation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add derived media feature capabilities plus thread-bound image generation in TUI, while preserving the current STT/TTS behavior exactly.

**Architecture:** Keep raw modality metadata untouched and add a smaller derived feature layer on top of fetched and selected models. Extend the daemon media path so user-forced image generation can create or reuse a thread, persist the image into thread-owned storage, record it in work context, and render it inline. Use optional fetch-time filters only where they reduce OpenRouter picker noise, but keep local feature derivation as the final source of truth.

**Tech Stack:** Rust workspace, `reqwest`, existing daemon media tools, tamux protocol messages, ratatui TUI state/client bridge, targeted `cargo test` suites.

---

## File Map

- `crates/amux-protocol/src/messages/client.rs`
  - Extend client->daemon IPC for filtered model fetches and user-forced image generation.
- `crates/amux-protocol/src/messages/daemon.rs`
  - Add daemon->client image-generation result payload.
- `crates/amux-protocol/src/messages/tests/mod.rs`
  - Lock message round-trips for the new IPC payloads.
- `crates/amux-daemon/src/agent/llm_client/helpers.rs`
  - Keep fetched-model metadata parsing, add optional fetch query plumbing, and add daemon-side fetched-model feature helpers.
- `crates/amux-daemon/src/agent/llm_client/tests/part3.rs`
  - Cover model-fetch query strings and metadata-preserving feature detection inputs.
- `crates/amux-daemon/src/agent/tool_executor/catalog/part_c.rs`
  - Gate `generate_image` exposure using derived image-generation capability.
- `crates/amux-daemon/src/agent/tool_executor/mod.rs`
  - Allow IPC execution of `generate_image` in addition to STT/TTS.
- `crates/amux-daemon/src/agent/tool_executor/media_tools.rs`
  - Centralize media feature predicates, capability-specific image-generation validation, and thread-persisted image generation flow.
- `crates/amux-daemon/src/agent/tool_executor/tests/part5.rs`
  - Lock tool-catalog visibility and media schema behavior.
- `crates/amux-daemon/src/server/dispatch_part4.rs`
  - Thread optional fetch filters through the async `fetch_models` operation path.
- `crates/amux-daemon/src/server/dispatch_part6.rs`
  - Route the new image-generation IPC request/result.
- `crates/amux-daemon/src/server/tests_part2_provider_models.rs`
  - Keep async fetch-model behavior stable after filter arguments are added.
- `crates/amux-daemon/src/agent/work_context.rs`
  - Reuse or extend artifact recording for generated image files persisted outside the repo.
- `crates/amux-daemon/src/agent/messaging.rs`
  - Reuse `get_or_create_thread(...)` semantics for no-thread image generation if visibility changes are required.
- `crates/amux-tui/src/state/mod.rs`
  - Add filtered model-fetch command fields and the new image-generation daemon command.
- `crates/amux-tui/src/main.rs`
  - Bridge the new daemon commands to the client IPC methods.
- `crates/amux-tui/src/client/mod.rs`
  - Add `ClientEvent::ImageGenerationResult`.
- `crates/amux-tui/src/client/impl_part1.rs`
  - Accept the new daemon message type in the reader.
- `crates/amux-tui/src/client/impl_part3.rs`
  - Translate the daemon image-generation result into a client event.
- `crates/amux-tui/src/client/impl_part5.rs`
  - Send filtered fetch requests and user-forced image-generation requests.
- `crates/amux-tui/src/app/settings_handlers/impl_part1.rs`
  - Preserve current STT/TTS filtering while passing optional OpenRouter fetch filters from audio pickers.
- `crates/amux-tui/src/app/settings_handlers/tests/tests_part1.rs`
  - Lock fetch-command arguments emitted by audio picker openers.
- `crates/amux-tui/src/app/tests/modal_handlers.rs`
  - Keep existing audio picker filtering behavior stable as the feature layer is introduced.
- `crates/amux-tui/src/app/commands.rs`
  - Parse `/image <prompt>` and issue the new daemon command against the active thread or no thread.
- `crates/amux-tui/src/app/events.rs`
  - Surface image-generation success/failure status without disturbing current STT/TTS event handling.
- `crates/amux-tui/src/app/tests/events.rs`
  - Lock TUI event/status behavior for image generation and ensure TTS activity behavior stays unchanged.

## Scope Notes

- This plan intentionally does **not** add a new frontend image-generation workflow.
- Existing frontend model metadata helpers stay out of the write scope unless Rust-side feature naming proves unclear and a tiny parity helper is needed later.
- Current STT/TTS behavior is a regression boundary; if a refactor makes those tests harder to preserve, stop and simplify the implementation.

### Task 1: Lock Media Feature Contracts And Audio Regressions

**Files:**
- Modify: `crates/amux-daemon/src/agent/llm_client/tests/part3.rs`
- Modify: `crates/amux-daemon/src/agent/tool_executor/tests/part5.rs`
- Modify: `crates/amux-tui/src/app/settings_handlers/tests/tests_part1.rs`
- Modify: `crates/amux-tui/src/app/tests/modal_handlers.rs`
- Modify: `crates/amux-tui/src/app/tests/events.rs`

- [ ] **Step 1: Write failing tests for the new contracts before changing implementation**

Add focused tests proving:
- OpenRouter filtered model fetch can request upstream modality filters without dropping metadata preservation
- current STT/TTS picker behavior remains directional and unchanged
- `generate_image` is not exposed for non-image-generation-capable model contexts
- `/image` result handling does not alter existing TTS footer activity behavior

- [ ] **Step 2: Run the targeted tests to verify they fail for the intended reasons**

Run: `cargo test -p tamux-daemon openrouter_filtered_model_fetch generate_image`
Expected: FAIL because fetch filtering and image-generation capability gating are not implemented yet.

Run: `cargo test -p tamux-tui audio_model_picker image_generation tts_request_surfaces_pending_footer_activity_until_audio_starts`
Expected: FAIL only for the new image/fetch assertions, while the existing TTS footer test stays green.

- [ ] **Step 3: Keep the failures isolated**

If any existing STT/TTS regression test fails before implementation starts, stop and fix the test setup instead of changing production behavior.

- [ ] **Step 4: Commit the red test baseline**

```bash
git add crates/amux-daemon/src/agent/llm_client/tests/part3.rs crates/amux-daemon/src/agent/tool_executor/tests/part5.rs crates/amux-tui/src/app/settings_handlers/tests/tests_part1.rs crates/amux-tui/src/app/tests/modal_handlers.rs crates/amux-tui/src/app/tests/events.rs
git commit -m "test: lock media capability and audio regression contracts"
```

### Task 2: Add Filtered Model Fetch Plumbing Without Breaking Existing Fetches

**Files:**
- Modify: `crates/amux-protocol/src/messages/client.rs`
- Modify: `crates/amux-protocol/src/messages/tests/mod.rs`
- Modify: `crates/amux-daemon/src/agent/llm_client/helpers.rs`
- Modify: `crates/amux-daemon/src/agent/llm_client/tests/part3.rs`
- Modify: `crates/amux-daemon/src/server/dispatch_part4.rs`
- Modify: `crates/amux-daemon/src/server/tests_part2_provider_models.rs`
- Modify: `crates/amux-tui/src/state/mod.rs`
- Modify: `crates/amux-tui/src/main.rs`
- Modify: `crates/amux-tui/src/client/impl_part5.rs`
- Modify: `crates/amux-tui/src/app/settings_handlers/impl_part1.rs`
- Modify: `crates/amux-tui/src/app/settings_handlers/tests/tests_part1.rs`

- [ ] **Step 1: Extend the fetch-model command shape with optional filter arguments**

Add optional fetch filter fields to:
- `ClientMessage::AgentFetchModels`
- `DaemonCommand::FetchModels`
- the TUI client bridge

Keep the default path unfiltered when no filter is supplied.

- [ ] **Step 2: Write the failing fetch-path tests**

Add tests proving:
- OpenRouter filtered fetch appends the right query parameter(s) to `/models`
- unfiltered fetch still uses the exact old request shape
- audio picker openers emit fetch filters only when the provider/endpoint warrants it

- [ ] **Step 3: Implement minimal filtered-fetch plumbing**

Implement:
- optional query building in `fetch_models(...)`
- transport of filter args through dispatch and TUI bridge
- targeted OpenRouter filters for STT/TTS picker opens in `open_audio_model_picker(...)`

Do **not** change normal main-model fetch behavior in this task.

- [ ] **Step 4: Re-run the targeted tests**

Run: `cargo test -p tamux-protocol client_message_roundtrips`
Expected: PASS with the new optional fields.

Run: `cargo test -p tamux-daemon fetch_models async_request_does_not_block_ping`
Expected: PASS, including the new filtered-fetch test and existing async fetch behavior.

Run: `cargo test -p tamux-tui activating_audio_stt_model_fetches_remote_models_for_audio_provider activating_audio_tts_model_fetches_remote_models_for_audio_provider`
Expected: PASS with the new filter-bearing daemon commands.

- [ ] **Step 5: Commit the fetch plumbing**

```bash
git add crates/amux-protocol/src/messages/client.rs crates/amux-protocol/src/messages/tests/mod.rs crates/amux-daemon/src/agent/llm_client/helpers.rs crates/amux-daemon/src/agent/llm_client/tests/part3.rs crates/amux-daemon/src/server/dispatch_part4.rs crates/amux-daemon/src/server/tests_part2_provider_models.rs crates/amux-tui/src/state/mod.rs crates/amux-tui/src/main.rs crates/amux-tui/src/client/impl_part5.rs crates/amux-tui/src/app/settings_handlers/impl_part1.rs crates/amux-tui/src/app/settings_handlers/tests/tests_part1.rs
git commit -m "feat: add filtered model fetch plumbing"
```

### Task 3: Add Derived Image-Generation Capability And Tool Gating In The Daemon

**Files:**
- Modify: `crates/amux-daemon/src/agent/llm_client/helpers.rs`
- Modify: `crates/amux-daemon/src/agent/tool_executor/catalog/part_c.rs`
- Modify: `crates/amux-daemon/src/agent/tool_executor/media_tools.rs`
- Modify: `crates/amux-daemon/src/agent/tool_executor/tests/part5.rs`
- Modify: `crates/amux-tui/src/app/modal_handlers_enter.rs` only if shared capability naming needs alignment
- Modify: `crates/amux-tui/src/app/tests/modal_handlers.rs` only if capability parity coverage needs extension

- [ ] **Step 1: Write failing tests for derived `image_generation` capability**

Add tests proving:
- a model with image output/generation support is recognized as `image_generation`
- STT/TTS directional behavior is unchanged by the new helper
- `generate_image` is not advertised when the current model context is not image-generation capable

- [ ] **Step 2: Run the daemon and TUI regression tests**

Run: `cargo test -p tamux-daemon media_tools_expose_expected_core_parameters`
Expected: FAIL for the new capability-gating expectation.

Run: `cargo test -p tamux-tui selecting_main_audio_capable_model_prompts_for_stt_reuse audio_model_picker_keeps_input_only_models_out_of_tts`
Expected: PASS, confirming the current audio behavior still holds before implementation.

- [ ] **Step 3: Implement the minimal feature helper and tool gating**

Implement:
- a daemon-side fetched-model feature predicate layer that keeps existing audio semantics intact
- image-generation capability checks used by `generate_image`
- conservative tool-catalog gating in `catalog/part_c.rs`

Do **not** replace the current raw modality fields or coarse pricing normalization.

- [ ] **Step 4: Re-run the focused tests**

Run: `cargo test -p tamux-daemon generate_image media_tools`
Expected: PASS, including the new image-generation capability tests.

Run: `cargo test -p tamux-tui selecting_main_audio_capable_model_prompts_for_stt_reuse audio_model_picker_keeps_input_only_models_out_of_tts`
Expected: PASS unchanged.

- [ ] **Step 5: Commit the daemon capability layer**

```bash
git add crates/amux-daemon/src/agent/llm_client/helpers.rs crates/amux-daemon/src/agent/tool_executor/catalog/part_c.rs crates/amux-daemon/src/agent/tool_executor/media_tools.rs crates/amux-daemon/src/agent/tool_executor/tests/part5.rs crates/amux-tui/src/app/modal_handlers_enter.rs crates/amux-tui/src/app/tests/modal_handlers.rs
git commit -m "feat: derive media feature capabilities"
```

### Task 4: Implement Thread-Persisted Image Generation In The Daemon IPC Path

**Files:**
- Modify: `crates/amux-protocol/src/messages/client.rs`
- Modify: `crates/amux-protocol/src/messages/daemon.rs`
- Modify: `crates/amux-protocol/src/messages/tests/mod.rs`
- Modify: `crates/amux-daemon/src/server/dispatch_part6.rs`
- Modify: `crates/amux-daemon/src/agent/tool_executor/mod.rs`
- Modify: `crates/amux-daemon/src/agent/tool_executor/media_tools.rs`
- Modify: `crates/amux-daemon/src/agent/work_context.rs`
- Modify: `crates/amux-daemon/src/agent/messaging.rs` only if thread creation helper visibility must be widened
- Modify: daemon tests in the closest existing media/thread test files touched by the implementation

- [ ] **Step 1: Add failing IPC and persistence tests**

Add tests proving:
- a user-forced image-generation request can target an existing thread
- a request with no thread id creates a new main-agent thread first
- the generated file is persisted into thread-owned storage, not left temp-only
- a work-context artifact entry is recorded for the persisted image
- a thread message with image content blocks is appended inline

- [ ] **Step 2: Run the targeted protocol and daemon tests to verify the red state**

Run: `cargo test -p tamux-protocol speech_to_text text_to_speech`
Expected: PASS, confirming the existing audio message round-trips are still stable before adding image IPC.

Run: `cargo test -p tamux-daemon image_generation thread`
Expected: FAIL only for the new image-generation IPC/persistence tests.

- [ ] **Step 3: Implement the minimal daemon path**

Implement:
- `AgentGenerateImage` / `AgentImageGenerationResult` protocol messages
- `execute_media_tool_for_ipc("generate_image", ...)`
- thread-aware image generation in `media_tools.rs`
- durable thread-file persistence plus work-context recording
- inline thread message append with image content blocks

Prefer reusing existing thread creation and persistence helpers over inventing a parallel storage path.

- [ ] **Step 4: Re-run the focused daemon tests**

Run: `cargo test -p tamux-daemon image_generation speech_to_text text_to_speech`
Expected: PASS, including the new image flow and the existing STT/TTS media tests.

- [ ] **Step 5: Commit the daemon image-generation flow**

```bash
git add crates/amux-protocol/src/messages/client.rs crates/amux-protocol/src/messages/daemon.rs crates/amux-protocol/src/messages/tests/mod.rs crates/amux-daemon/src/server/dispatch_part6.rs crates/amux-daemon/src/agent/tool_executor/mod.rs crates/amux-daemon/src/agent/tool_executor/media_tools.rs crates/amux-daemon/src/agent/work_context.rs crates/amux-daemon/src/agent/messaging.rs
git commit -m "feat: persist generated images into threads"
```

### Task 5: Add The TUI `/image` Command And Result Handling

**Files:**
- Modify: `crates/amux-tui/src/state/mod.rs`
- Modify: `crates/amux-tui/src/main.rs`
- Modify: `crates/amux-tui/src/client/mod.rs`
- Modify: `crates/amux-tui/src/client/impl_part1.rs`
- Modify: `crates/amux-tui/src/client/impl_part3.rs`
- Modify: `crates/amux-tui/src/client/impl_part5.rs`
- Modify: `crates/amux-tui/src/app/commands.rs`
- Modify: `crates/amux-tui/src/app/events.rs`
- Modify: `crates/amux-tui/src/app/tests/events.rs`
- Modify: command-focused TUI tests in `crates/amux-tui/src/app/tests/...` if a dedicated slash-command test file is needed

- [ ] **Step 1: Write failing tests for `/image <prompt>`**

Add tests proving:
- `/image` with an open thread emits the new daemon command with that thread id
- `/image` without an open thread still emits the command without fabricating a local thread id
- success updates the status line without disturbing existing TTS behavior
- failure shows a warning/error notice

- [ ] **Step 2: Run the targeted TUI tests to verify they fail**

Run: `cargo test -p tamux-tui image_command image_generation_result`
Expected: FAIL because the new command/event path is not implemented yet.

Run: `cargo test -p tamux-tui tts_request_surfaces_pending_footer_activity_until_audio_starts`
Expected: PASS unchanged.

- [ ] **Step 3: Implement the minimal TUI bridge**

Implement:
- new `DaemonCommand` and client send/receive plumbing
- `/image` slash-command parsing and prompt validation
- event/status handling for success and error feedback

Do **not** add a second local-only rendering path; the daemon-owned thread update should remain the source of truth for the inline image.

- [ ] **Step 4: Re-run the focused TUI tests**

Run: `cargo test -p tamux-tui image_command image_generation_result tts_request_surfaces_pending_footer_activity_until_audio_starts`
Expected: PASS.

- [ ] **Step 5: Commit the TUI command flow**

```bash
git add crates/amux-tui/src/state/mod.rs crates/amux-tui/src/main.rs crates/amux-tui/src/client/mod.rs crates/amux-tui/src/client/impl_part1.rs crates/amux-tui/src/client/impl_part3.rs crates/amux-tui/src/client/impl_part5.rs crates/amux-tui/src/app/commands.rs crates/amux-tui/src/app/events.rs crates/amux-tui/src/app/tests/events.rs
git commit -m "feat: add thread-bound /image command"
```

### Task 6: Verify The Integrated Behavior And Guard STT/TTS

**Files:**
- Modify: test files only if verification exposes a missing assertion

- [ ] **Step 1: Run the targeted protocol/daemon/TUI suites together**

Run: `cargo test -p tamux-protocol`
Expected: PASS.

Run: `cargo test -p tamux-daemon fetch_models image_generation speech_to_text text_to_speech`
Expected: PASS, including the new thread-persisted image-generation coverage.

Run: `cargo test -p tamux-tui audio_model_picker image_command tts_request_surfaces_pending_footer_activity_until_audio_starts`
Expected: PASS, proving the old STT/TTS UX stayed intact.

- [ ] **Step 2: Run focused crate builds**

Run: `cargo build -p tamux-protocol -p tamux-daemon -p tamux-tui`
Expected: PASS.

- [ ] **Step 3: Perform a manual TUI smoke check against a live daemon if credentials are available**

Verify:
- audio STT picker still shows only STT-capable models
- audio TTS picker still shows only TTS-capable models
- `/image <prompt>` in an open thread appends an inline image and thread artifact
- `/image <prompt>` with no open thread creates a new main-agent thread and records the image there

- [ ] **Step 4: Summarize residual risk**

Document:
- any OpenRouter filter parameters still inferred rather than hard-confirmed in local integration
- any providers limited to local post-fetch filtering because they do not support upstream modality queries

