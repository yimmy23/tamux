# TUI File References, Clickable Tool Paths, and Diff Preview Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `@path` references with `Tab` completion in the TUI input, append compact referenced-file metadata on submit, render clickable file chips in chat tool rows, and open file or diff previews in the main pane without attaching file contents to prompts.

**Architecture:** Keep file-reference parsing and completion local to `amux-tui`. Reuse the existing daemon preview/diff IPC and `TaskState` caches for content, but add a dedicated main-pane file preview route so chat-originated file clicks do not depend on sidebar work-context entries. Parse tool argument JSON in the chat layer only for supported file tools and fail closed when arguments are missing or invalid.

**Tech Stack:** Rust workspace (`ratatui`, `crossterm`, `serde_json`, `std::fs`/`std::path`), existing TUI chat/widget split (`widgets/chat/part*.rs`), existing preview/diff daemon requests.

**Spec:** `docs/superpowers/specs/2026-04-01-tui-file-references-design.md`

---

## File Map

### Create

- `crates/amux-tui/src/state/input_refs.rs`
  Purpose: parse active `@...` tokens, resolve paths, compute completion candidates, and build submit-time referenced-file footers without bloating `state/input.rs`.
- `crates/amux-tui/src/widgets/file_preview.rs`
  Purpose: render a standalone file/diff preview in the main pane for chat-originated file clicks, reusing `TaskState` preview/diff caches and close-preview affordances.

### Modify

- `crates/amux-tui/src/state/input.rs`
  Purpose: expose token/cursor helpers and include `input_refs.rs` without further inflating the already-large file.
- `crates/amux-tui/src/state/chat_types.rs`
  Purpose: add chat hit targets for clickable file chips.
- `crates/amux-tui/src/app/mod.rs`
  Purpose: add minimal state for a main-pane file preview target.
- `crates/amux-tui/src/app/keyboard.rs`
  Purpose: give input-local `@` completion precedence over global `Tab` focus cycling.
- `crates/amux-tui/src/app/commands.rs`
  Purpose: append referenced-file footer on submit and add a helper that opens chat-selected file previews/diffs.
- `crates/amux-tui/src/app/mouse_helpers.rs`
  Purpose: execute the new chat file hit target.
- `crates/amux-tui/src/app/rendering.rs`
  Purpose: render the standalone file preview main-pane route.
- `crates/amux-tui/src/widgets/mod.rs`
  Purpose: export the new `file_preview` widget.
- `crates/amux-tui/src/widgets/chat/mod.rs`
  Purpose: include any new helpers/tests used by clickable file chip rendering.
- `crates/amux-tui/src/widgets/chat/part1.rs`
  Purpose: render path chips for `read_file` and edit-oriented tool rows.
- `crates/amux-tui/src/widgets/chat/part4.rs`
  Purpose: hit-test path chip bounds precisely.
- `crates/amux-tui/src/widgets/work_context_view.rs`
  Purpose: optionally extract or reuse preview rendering helpers so plain file preview and diff preview stay visually consistent.
- `crates/amux-tui/src/state/tests/part1.rs`
- `crates/amux-tui/src/state/tests/part2.rs`
- `crates/amux-tui/src/widgets/chat/tests/tests_part1.rs`
- `crates/amux-tui/src/widgets/chat/tests/tests_part2.rs`
- `crates/amux-tui/src/widgets/tests/work_context_view.rs`
- `crates/amux-tui/src/app/tests/tests_part3.rs`
- `crates/amux-tui/src/app/tests/tests_part5.rs`
- `crates/amux-tui/src/app/tests/tests_part6.rs`
  Purpose: regression coverage for input references, chat hit-testing, preview routing, and submit behavior.

### Keep Unchanged Unless Forced by Implementation

- `crates/amux-tui/src/client/impl_part4.rs`
- `crates/amux-tui/src/client/impl_part5.rs`
- `crates/amux-tui/src/main.rs`
- `crates/amux-tui/src/state/task.rs`

Rationale: existing file preview and git diff request plumbing already exists and should be reused before widening IPC or task-state storage.

---

## Task 1: Add input-level `@path` parsing and `Tab` completion

**Files:**
- Create: `crates/amux-tui/src/state/input_refs.rs`
- Modify: `crates/amux-tui/src/state/input.rs`
- Modify: `crates/amux-tui/src/app/keyboard.rs`
- Test: `crates/amux-tui/src/state/tests/part1.rs`
- Test: `crates/amux-tui/src/state/tests/part2.rs`
- Test: `crates/amux-tui/src/app/tests/tests_part6.rs`

- [ ] **Step 1: Write the failing input parsing tests**

Add tests covering:

- active `@...` token detection under the cursor
- no active token when cursor is outside a reference
- path resolution keeps nonexistent references as plain text
- completion extends a single match and keeps directory trailing slash

- [ ] **Step 2: Write the failing keyboard behavior tests**

Add app-level tests covering:

- `Tab` in input completes an active file reference instead of changing focus
- `Tab` still cycles focus when the cursor is not inside an `@...` token

- [ ] **Step 3: Run the targeted tests and verify failure**

Run:

```bash
cargo test -q -p tamux-tui active_at_token -- --nocapture
cargo test -q -p tamux-tui tab_completes_active_file_reference_instead_of_changing_focus -- --nocapture
```

Expected: failures because no `@` token model or input-local `Tab` completion exists.

- [ ] **Step 4: Implement minimal input reference helpers**

Implement in `crates/amux-tui/src/state/input_refs.rs`:

- active-token detection from `InputState::buffer()` + cursor offset
- path expansion for relative paths, absolute paths, and `~`
- filesystem completion that returns either a full completion or a longest shared prefix
- a small result type that tells the keyboard layer whether it consumed `Tab`

Keep `crates/amux-tui/src/state/input.rs` focused by exposing only small wrappers into the new helper module.

- [ ] **Step 5: Wire `Tab` precedence in the keyboard layer**

Update `crates/amux-tui/src/app/keyboard.rs` so:

- when `focus == FocusArea::Input` and an active `@...` token exists, `KeyCode::Tab` runs completion first
- only fall back to `focus_next()` when no completion was possible or no active token exists
- ambiguous completions use the existing input notice/status mechanism instead of modal UI

- [ ] **Step 6: Re-run the targeted tests**

Run:

```bash
cargo test -q -p tamux-tui active_at_token -- --nocapture
cargo test -q -p tamux-tui tab_completes_active_file_reference_instead_of_changing_focus -- --nocapture
cargo test -q -p tamux-tui tab_focus_cycles_when_not_inside_file_reference -- --nocapture
```

Expected: pass.

- [ ] **Step 7: Commit**

```bash
git add crates/amux-tui/src/state/input.rs \
        crates/amux-tui/src/state/input_refs.rs \
        crates/amux-tui/src/app/keyboard.rs \
        crates/amux-tui/src/state/tests/part1.rs \
        crates/amux-tui/src/state/tests/part2.rs \
        crates/amux-tui/src/app/tests/tests_part6.rs
git commit -m "feat: add tui file reference completion"
```

---

## Task 2: Append compact referenced-file metadata on submit

**Files:**
- Modify: `crates/amux-tui/src/state/input_refs.rs`
- Modify: `crates/amux-tui/src/app/commands.rs`
- Test: `crates/amux-tui/src/app/tests/tests_part3.rs`
- Test: `crates/amux-tui/src/app/tests/tests_part6.rs`

- [ ] **Step 1: Write the failing submit-path tests**

Add tests covering:

- submitting a prompt with `@relative/path` appends a referenced-files footer with normalized absolute paths
- duplicate references collapse to one footer entry
- nonexistent `@...` tokens stay in the prompt text but do not generate footer metadata
- file references do not inline file contents or create attachments

- [ ] **Step 2: Run the targeted tests and verify failure**

Run:

```bash
cargo test -q -p tamux-tui submit_prompt_appends_referenced_files_footer -- --nocapture
cargo test -q -p tamux-tui submit_prompt_does_not_inline_referenced_file_contents -- --nocapture
```

Expected: failures because `submit_prompt` currently forwards raw prompt text only.

- [ ] **Step 3: Implement footer generation**

Implement in `crates/amux-tui/src/state/input_refs.rs`:

- extraction of resolved referenced files from the full prompt
- deduplication and normalization to absolute paths
- a helper that appends the footer exactly once and leaves the visible user text unchanged

Update `crates/amux-tui/src/app/commands.rs::submit_prompt` so it:

- still drains true attachments as before
- applies referenced-file footer generation after attachment wrapping
- stores the final sent content in the local user message and daemon command payload

- [ ] **Step 4: Re-run the targeted tests**

Run:

```bash
cargo test -q -p tamux-tui submit_prompt_appends_referenced_files_footer -- --nocapture
cargo test -q -p tamux-tui submit_prompt_deduplicates_referenced_files -- --nocapture
cargo test -q -p tamux-tui submit_prompt_does_not_inline_referenced_file_contents -- --nocapture
```

Expected: pass.

- [ ] **Step 5: Commit**

```bash
git add crates/amux-tui/src/state/input_refs.rs \
        crates/amux-tui/src/app/commands.rs \
        crates/amux-tui/src/app/tests/tests_part3.rs \
        crates/amux-tui/src/app/tests/tests_part6.rs
git commit -m "feat: append referenced file metadata in tui prompts"
```

---

## Task 3: Render clickable file chips for `read_file` and file-edit tool rows

**Files:**
- Modify: `crates/amux-tui/src/state/chat_types.rs`
- Modify: `crates/amux-tui/src/widgets/chat/mod.rs`
- Modify: `crates/amux-tui/src/widgets/chat/part1.rs`
- Modify: `crates/amux-tui/src/widgets/chat/part4.rs`
- Test: `crates/amux-tui/src/widgets/chat/tests/tests_part1.rs`
- Test: `crates/amux-tui/src/widgets/chat/tests/tests_part2.rs`

- [ ] **Step 1: Write the failing chat rendering tests**

Add tests covering:

- `read_file` tool rows render a clickable `[path]` chip when `tool_arguments` contains `path`
- edit tools (`write_file`, `create_file`, `append_to_file`, `replace_in_file`, `apply_file_patch`) render a clickable `[path]` chip when `path` is present
- invalid JSON or missing `path` does not create a file chip

- [ ] **Step 2: Write the failing hit-test tests**

Add tests covering:

- clicking inside the rendered chip returns a dedicated `ChatHitTarget::ToolFilePath { ... }`
- clicking outside the chip but on the same row still resolves to the existing message/tool hit target

- [ ] **Step 3: Run the targeted tests and verify failure**

Run:

```bash
cargo test -q -p tamux-tui read_file_tool_row_renders_clickable_path_chip -- --nocapture
cargo test -q -p tamux-tui hit_test_returns_tool_file_path_target -- --nocapture
```

Expected: failures because chat hit targets currently only understand message/toggle/action targets.

- [ ] **Step 4: Implement minimal path-chip rendering**

Implement:

- a new `ChatHitTarget` variant carrying the clicked path and the originating tool name
- a small helper in `widgets/chat/part1.rs` that parses `tool_arguments` JSON and extracts `path` for the supported file tools only
- rendering logic that appends a visually distinct `[path]` chip to the tool row while keeping the rest of the row unchanged

Keep parsing local and fail closed when the JSON shape is wrong.

- [ ] **Step 5: Implement chip-aware hit-testing**

Update `widgets/chat/part4.rs` so hit-testing:

- measures the rendered chip from the same spans used in rendering
- returns the new file-path hit target only inside the chip bounds
- preserves existing behavior outside those bounds

- [ ] **Step 6: Re-run the targeted tests**

Run:

```bash
cargo test -q -p tamux-tui read_file_tool_row_renders_clickable_path_chip -- --nocapture
cargo test -q -p tamux-tui edit_tool_row_renders_clickable_path_chip -- --nocapture
cargo test -q -p tamux-tui hit_test_returns_tool_file_path_target -- --nocapture
cargo test -q -p tamux-tui invalid_tool_arguments_do_not_create_file_chip -- --nocapture
```

Expected: pass.

- [ ] **Step 7: Commit**

```bash
git add crates/amux-tui/src/state/chat_types.rs \
        crates/amux-tui/src/widgets/chat/mod.rs \
        crates/amux-tui/src/widgets/chat/part1.rs \
        crates/amux-tui/src/widgets/chat/part4.rs \
        crates/amux-tui/src/widgets/chat/tests/tests_part1.rs \
        crates/amux-tui/src/widgets/chat/tests/tests_part2.rs
git commit -m "feat: render clickable file chips in tui chat"
```

---

## Task 4: Open chat-selected file previews and repo-backed diffs in the main pane

**Files:**
- Create: `crates/amux-tui/src/widgets/file_preview.rs`
- Modify: `crates/amux-tui/src/app/mod.rs`
- Modify: `crates/amux-tui/src/app/commands.rs`
- Modify: `crates/amux-tui/src/app/mouse_helpers.rs`
- Modify: `crates/amux-tui/src/app/rendering.rs`
- Modify: `crates/amux-tui/src/widgets/mod.rs`
- Modify: `crates/amux-tui/src/widgets/work_context_view.rs`
- Test: `crates/amux-tui/src/widgets/tests/work_context_view.rs`
- Test: `crates/amux-tui/src/app/tests/tests_part5.rs`

- [ ] **Step 1: Write the failing preview-routing tests**

Add tests covering:

- clicking a chat file chip opens a main-pane file preview route
- non-repo files request `DaemonCommand::RequestFilePreview`
- repo-backed files request `DaemonCommand::RequestGitDiff` with repo root plus repo-relative file path
- closing the preview returns to the conversation pane

- [ ] **Step 2: Run the targeted tests and verify failure**

Run:

```bash
cargo test -q -p tamux-tui clicking_chat_file_chip_requests_file_preview -- --nocapture
cargo test -q -p tamux-tui clicking_repo_backed_chat_file_chip_requests_git_diff -- --nocapture
```

Expected: failures because chat file hit targets are not yet routed anywhere and there is no standalone main-pane file preview state.

- [ ] **Step 3: Implement a dedicated main-pane file preview target**

Update app state to carry a minimal preview target:

- absolute path
- optional repo root
- optional repo-relative path

Add a helper in `crates/amux-tui/src/app/commands.rs` that:

- resolves repo root locally by walking parent directories for `.git`
- switches the main pane into file preview mode
- requests git diff for repo-backed paths or plain file preview otherwise

- [ ] **Step 4: Implement the standalone preview widget**

Create `crates/amux-tui/src/widgets/file_preview.rs` that:

- renders the selected file path and a close-preview control
- shows diff text when a cached diff exists
- falls back to cached file preview for non-repo files
- preserves existing “loading preview”, “no diff preview available”, and binary-file messaging

Extract any shared text-wrapping or preview helpers from `work_context_view.rs` only when necessary to avoid duplication.

- [ ] **Step 5: Wire mouse handling and rendering**

Update:

- `crates/amux-tui/src/app/mouse_helpers.rs` to route `ChatHitTarget::ToolFilePath`
- `crates/amux-tui/src/app/rendering.rs` to render the new `widgets::file_preview` main-pane route
- `crates/amux-tui/src/widgets/mod.rs` exports

- [ ] **Step 6: Re-run the targeted tests**

Run:

```bash
cargo test -q -p tamux-tui clicking_chat_file_chip_requests_file_preview -- --nocapture
cargo test -q -p tamux-tui clicking_repo_backed_chat_file_chip_requests_git_diff -- --nocapture
cargo test -q -p tamux-tui closing_chat_file_preview_returns_to_conversation -- --nocapture
```

Expected: pass.

- [ ] **Step 7: Commit**

```bash
git add crates/amux-tui/src/widgets/file_preview.rs \
        crates/amux-tui/src/app/mod.rs \
        crates/amux-tui/src/app/commands.rs \
        crates/amux-tui/src/app/mouse_helpers.rs \
        crates/amux-tui/src/app/rendering.rs \
        crates/amux-tui/src/widgets/mod.rs \
        crates/amux-tui/src/widgets/work_context_view.rs \
        crates/amux-tui/src/widgets/tests/work_context_view.rs \
        crates/amux-tui/src/app/tests/tests_part5.rs
git commit -m "feat: open chat-selected file previews in tui"
```

---

## Task 5: Final focused verification and manual smoke pass

**Files:**
- Modify: none unless a previous verification step exposes a defect

- [ ] **Step 1: Run focused Rust verification**

Run:

```bash
cargo test -q -p tamux-tui active_at_token -- --nocapture
cargo test -q -p tamux-tui tab_completes_active_file_reference_instead_of_changing_focus -- --nocapture
cargo test -q -p tamux-tui submit_prompt_appends_referenced_files_footer -- --nocapture
cargo test -q -p tamux-tui read_file_tool_row_renders_clickable_path_chip -- --nocapture
cargo test -q -p tamux-tui hit_test_returns_tool_file_path_target -- --nocapture
cargo test -q -p tamux-tui clicking_chat_file_chip_requests_file_preview -- --nocapture
cargo test -q -p tamux-tui clicking_repo_backed_chat_file_chip_requests_git_diff -- --nocapture
```

Expected: pass.

- [ ] **Step 2: Run full crate verification**

Run:

```bash
cargo fmt --all
cargo test -p tamux-tui
```

Expected: formatting clean and full `tamux-tui` test suite passes.

- [ ] **Step 3: Manual smoke-check list**

Verify manually in `cargo run --release --bin amux-tui`:

- typing `@` plus `Tab` completes a real file path from the current working directory
- `Tab` still changes focus when no active `@...` token is under the cursor
- submitted prompts with `@path` do not attach file contents
- the sent prompt contains the referenced-files footer only once
- `read_file` rows show clickable `[path]` chips
- clicking a `read_file` chip opens the main-pane preview
- clicking an edit-tool chip opens a diff-like preview for repo-backed files
- closing preview returns to the conversation view cleanly

- [ ] **Step 4: Commit verification-only follow-ups if needed**

If verification exposed no defects, skip this step. If it exposed fixes, commit with a narrow message such as:

```bash
git add <exact files>
git commit -m "fix: polish tui file reference preview flow"
```
