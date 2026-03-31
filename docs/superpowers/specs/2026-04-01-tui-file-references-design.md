# TUI File References, Clickable Tool Paths, and Diff Preview Design

Date: 2026-04-01

## Summary

This design adds lightweight file references to the TUI without attaching file contents to prompts. Users can type `@path` in the input, complete it with `Tab`, and submit a message that tells the agent which files matter without blowing up the prompt. The chat transcript then renders structured file references for relevant tool calls and file edits so the user can click into the existing preview pane and inspect either file contents or git-style diffs.

The key product decisions are:

- `@path` is a reference, not an attachment.
- `Tab` completes file references when the cursor is inside an `@...` token.
- Submitted prompts carry normalized referenced-file metadata, not file contents.
- `read_file` tool rows render a clickable file chip such as `read_file [path]`.
- File edits render a clickable file chip that opens the same preview surface.
- Diff rendering reuses existing work-context preview plumbing instead of adding a second inline diff viewer inside the transcript.

## Goals

- Let users reference local files from the TUI input with minimal friction.
- Avoid prompt bloat from automatic file attachment or content inlining.
- Make `read_file` results visibly point at the referenced file.
- Make edited files inspectable from the chat transcript via a git diff-like preview.
- Reuse the existing file preview and diff request pipeline where possible.

## Non-Goals

- Uploading file contents automatically as part of normal `@path` usage.
- Building a full attachment browser or attachment lifecycle for file references.
- Rendering large inline diffs directly inside the chat transcript body.
- Designing a general-purpose mention system for users, agents, goals, or threads.
- Changing daemon tool semantics beyond what is required to expose file paths cleanly.

## Problem Statement

The current TUI has three related gaps:

- The input supports pasted file paths and `/attach`, but not a lightweight “tell the agent this file matters” workflow.
- `read_file` results appear as generic tool rows, so users cannot click through to inspect the referenced file in the preview pane.
- File edits are recorded in work-context views, but chat does not expose an immediate “show me the edited file/diff” affordance at the place where the edit happened.

This creates unnecessary friction. Users either attach file contents, which is expensive in context, or they type raw paths into free text and hope the agent notices. Even when the agent does use `read_file` or edit a file, the transcript does not expose the most useful follow-up action: open the file preview or diff.

## Approach

Recommended approach: add a path-reference layer inside the TUI and route all file clicks through the existing preview state.

- The input layer detects `@...` tokens and offers completion based on the TUI process working directory.
- Submission converts resolved `@...` tokens into a compact “referenced files” footer added to the outgoing prompt.
- Chat rendering extracts file-path metadata from tool arguments and edit/file messages and exposes them as clickable hit targets.
- Clicking a file path populates the same preview/diff state already used by work-context views, then switches the main conversation pane to that preview surface.

This keeps the user experience richer without introducing a second file-view model.

## Design

### 1. Input-Level `@path` References

The input buffer gains lightweight file-reference semantics:

- A token beginning with `@` is treated as a candidate file reference.
- Valid forms include relative paths like `@src/main.rs` and absolute paths like `@/tmp/debug.log`.
- The literal `@path` text remains visible in the input. There is no hidden attachment or token replacement while editing.
- Only the token under the cursor participates in completion; the rest of the input remains unchanged.

Resolution rules:

- Relative paths resolve against the TUI process current working directory.
- `~` expands to the user home directory.
- A resolved path must exist locally to become a structured file reference.
- Nonexistent paths remain plain text and do not produce reference metadata.

This keeps the editing model simple and predictable.

### 2. `Tab` Completion Behavior

`Tab` gets an input-specific path completion mode before global focus cycling.

Behavior when focus is in the input:

- If the cursor is inside an `@...` token, `Tab` attempts path completion instead of moving focus.
- Matching entries are read from the filesystem relative to the partial token.
- If there is a single match, complete the token immediately.
- If there are multiple matches with a shared prefix, extend to the longest shared prefix.
- If there are still multiple ambiguous matches, show an input notice summarizing the possibilities or that the match remains ambiguous.
- Directory completions keep a trailing slash so the user can continue completing nested paths.

Fallback:

- If the cursor is not inside an `@...` token, existing `Tab` focus navigation behavior remains unchanged.

This gives `@` handling a global feel inside the input without taking over all `Tab` behavior across the app.

### 3. Submit-Time Reference Expansion

On submit, the TUI scans the prompt for resolved `@...` tokens and appends a compact machine-readable footer. Example shape:

- natural user text remains unchanged
- footer appended once:
  `Referenced files: /abs/path/a.rs, /abs/path/b.md`
  `Inspect these with read_file before making assumptions.`

Rules:

- Do not inline file contents.
- Deduplicate repeated references.
- Normalize paths to absolute paths in the footer so the agent sees exactly what to inspect.
- Preserve the original user-visible text, including the `@...` tokens.

This keeps the user’s message natural while giving the agent explicit, reliable file targets.

### 4. Clickable File Targets in Chat

The chat transcript gains explicit file hit targets in addition to the existing message-level actions.

For `read_file`:

- Parse the tool argument JSON and extract the `path`.
- Render the tool summary as `read_file [path]` where `[path]` is visually distinct and clickable.
- Clicking the path opens file preview for that path.

For file edits:

- Detect file paths from edit-oriented tool calls such as `write_file`, `create_file`, `append_to_file`, `replace_in_file`, and `apply_file_patch`.
- Render a clickable `[path]` chip in the tool row or tool detail area.
- Clicking the chip opens the same preview surface.

Hit-testing:

- Chat hit-testing must distinguish between clicking the tool row generally and clicking the file chip specifically.
- Existing selection and action-bar behavior must continue to work outside the chip bounds.

### 5. Preview and Diff Routing

All file clicks route into the existing preview state instead of building a new viewer.

Routing behavior:

- If the referenced file lives under a git repo root, request git diff first and show the preview pane in diff mode.
- If there is no repo root, request plain file preview.
- If a repo-backed file has no diff available, show a clear “no diff preview available” message and allow plain preview fallback when possible.
- The preview opens in the main pane area that normally shows the conversation, matching the current work-context preview behavior.

State model:

- Introduce a small chat-originated preview selection path in TUI state so chat clicks can open the same preview pane used by work-context/sidebar flows.
- Reuse existing `TaskState.file_previews` and `TaskState.git_diffs` caches rather than duplicating content storage.

The preview pane remains the single place where file contents and diffs are inspected.

### 6. Edited-File Visibility in the Transcript

Edited files should be discoverable directly from the conversation that produced them.

Behavior:

- When an edit tool completes successfully and exposes a path, the transcript shows a file chip for that path.
- Clicking that chip opens a git diff-like preview when the file is repo-backed.
- This does not require inline diff rendering in chat; the chat transcript remains compact and readable.

This gives users immediate post-edit inspection without leaving the conversation context mentally, even though the preview is shown in the existing side pane.

## Data Model Changes

### Input / App State

- Add helpers to detect the active `@...` token at the cursor.
- Add helpers to resolve and autocomplete candidate paths.
- Add a submit-time extractor that returns normalized referenced files for the current prompt.

### Chat State / Rendering

- Extend chat hit targets with file-specific click targets.
- Add rendering helpers that can expose path chips for tool messages.
- Keep path extraction close to chat/tool rendering so the view layer stays deterministic.

### Preview State

- Reuse `TaskState.file_previews` and `TaskState.git_diffs`.
- Add minimal app state needed to track a chat-selected preview target when the user opens a file directly from the transcript.

## Error Handling

- Invalid or nonexistent `@...` paths remain plain text and do not block submit.
- Completion failures due to unreadable directories show a status/input notice rather than an error modal.
- Missing `path` in tool arguments means the tool row stays non-clickable and renders normally.
- Invalid JSON tool arguments must fail closed: no path chip rather than broken rendering.
- Binary files continue to use the existing “binary preview is not available” messaging.

## Testing Strategy

### Input Tests

- Detect the active `@...` token under the cursor.
- Complete a single matching file path.
- Extend to a shared prefix when multiple matches exist.
- Preserve focus-cycling `Tab` behavior when no active `@...` token exists.
- Submit-time extraction deduplicates and normalizes referenced paths.

### Chat Rendering / Hit-Testing Tests

- `read_file` tool rows render a clickable path chip when `path` is present in arguments.
- Edit tool rows render a clickable path chip when `path` is present in arguments.
- Clicking inside the chip resolves to a file hit target.
- Clicking outside the chip still selects or toggles the message as before.
- Invalid JSON or missing `path` does not create a false clickable region.

### Preview Routing Tests

- Clicking a non-repo file requests a file preview and opens the preview pane.
- Clicking a repo-backed file requests a git diff and opens the preview pane.
- Repo-backed files with no cached diff render the existing loading/empty-diff messaging correctly.

## Rollout Plan

1. Input token detection and `Tab` completion
2. Submit-time referenced-file footer generation
3. Chat file-chip rendering and hit-testing
4. Chat-to-preview routing using existing preview caches and requests
5. Regression coverage for plain preview and diff preview flows

## Risks

- `Tab` completion can regress existing focus navigation if input-specific precedence is not narrow.
- Path parsing can become fragile if it assumes JSON argument shapes too aggressively.
- Chat hit-testing can drift from rendering if chip bounds are computed separately from the actual rendered spans.
- Repo-root detection for chat-selected files must match the existing work-context diff behavior or users will get inconsistent preview modes.

## Recommendation

Implement the input `@path` flow and chat file chips together, but keep all file inspection routed through the existing preview pane. That gives the user the requested workflow with the least architectural churn and keeps the transcript compact while still making file reads and edits immediately inspectable.
