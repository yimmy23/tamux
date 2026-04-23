# Thread Artifact Paths Design

Date: 2026-04-23
Status: Draft for review

## Summary

tamux currently mixes two storage models for agent-created files:

- the base agent/system prompt tells agents to keep working specs in `/tmp/*.md`
- several runtime features already persist thread-scoped outputs under tamux-owned storage

This creates drift between prompt guidance and runtime behavior, and it weakens thread history because important working artifacts can disappear from `/tmp`.

The goal of this design is to introduce one canonical thread artifact path helper under the tamux data root, use it for ordinary thread-scoped durable artifacts, and update prompt guidance so agents default to thread-owned storage instead of `/tmp` for reusable specs and notes.

## Goals

- Define a canonical thread artifact directory under the tamux data root.
- Keep prompt instructions aligned with runtime storage paths.
- Preserve thread history for durable non-repo artifacts such as specs, notes, generated media, and previews.
- Avoid a risky big-bang migration of existing historical files.
- Keep goal-run inventory as a separate storage concept.

## Non-Goals

- Do not migrate all legacy files in one pass.
- Do not change repository file behavior.
- Do not change goal-run inventory layout under `.tamux/goals/<goal_run_id>/...`.
- Do not introduce a database schema migration solely for this path change.

## Current State

The codebase already has multiple durable storage patterns:

- goal-run artifacts live in `.tamux/goals/<goal_run_id>/...`
- generated media persists under agent-owned thread files
- offloaded payloads persist under thread-specific directories
- tool output previews persist under tamux-owned cache directories

However, the base agent/system prompt still directs agents to keep working specs in `/tmp/*.md`. That instruction is inconsistent with the existing tamux-owned storage model and makes thread history less durable than it should be.

## Proposed Layout

Add a canonical thread-owned subtree under the tamux data root:

```text
~/.tamux/
  threads/
    <thread_id>/
      artifacts/
        specs/
        media/
        previews/
```

The exact root continues to respect the existing tamux data-dir resolution logic, including `TAMUX_DATA_DIR`.

## Path Ownership Rules

Use the new thread artifact subtree for ordinary thread-scoped durable files:

- working specs
- working plans
- reusable thread notes
- generated media
- thread-level preview files if preview migration is included in this pass

Keep the following where they are:

- repository edits stay in the repository
- goal-run inventory stays under `.tamux/goals/<goal_run_id>/...`
- true scratch files that are intentionally temporary may still use `/tmp`

Rule of thumb:

- if the file should survive handoff, restart, or later thread inspection, it belongs under the thread artifact tree
- if the file is disposable implementation scratch, it may stay in `/tmp`

## Shared Helper API

Introduce shared helper functions for thread-owned storage, exposed from a runtime-path module that prompt builders and runtime writers can both use.

Expected helpers:

- `thread_root(thread_id)`
- `thread_artifacts_dir(thread_id)`
- `thread_specs_dir(thread_id)`
- `thread_media_dir(thread_id)`
- `thread_previews_dir(thread_id)`

Design constraints:

- helpers must derive paths from the canonical tamux data root
- helpers must sanitize or reject invalid path segments consistently
- helpers should not duplicate path-building logic across prompt and runtime code

## Adoption Plan

### Phase 1: Canonical helper

Add the shared helper and tests for path generation.

### Phase 2: Prompt alignment

Update the base agent/system prompt so multi-step specs and working notes default to the thread specs directory instead of `/tmp/*.md`.

Expected behavior after this change:

- new agent-created specs and notes land under `threads/<thread_id>/artifacts/specs/`
- the prompt no longer teaches agents to depend on `/tmp` for durable planning artifacts

### Phase 3: Selective runtime adoption

Migrate obvious thread-scoped durable writers to use the helper:

- generated media writers should use `thread_media_dir(thread_id)`
- tool output preview writers may use `thread_previews_dir(thread_id)` in the same pass if the change is straightforward

### Phase 4: Legacy compatibility

Keep legacy read paths working for existing historical data where readers already rely on them.

This pass does not:

- bulk-move old payloads
- rewrite historical database references
- add a one-time migration job

## Data Flow

For a new thread-scoped durable artifact:

1. the caller asks the shared helper for the destination directory
2. the writer ensures the parent directory exists
3. the writer persists the file using the existing write strategy for that subsystem
4. work-context tracking records the resulting concrete path as usual

This preserves the current work-context and history model while improving artifact durability and discoverability.

## Error Handling

- Reject or sanitize unsafe thread IDs before path construction.
- Continue using `create_dir_all` before writes.
- Preserve atomic write/rename behavior where subsystems already provide it.
- Leave legacy readers unchanged in this pass to avoid migration-time regressions.

## Testing

Add or update tests for:

- helper path generation across Unix and Windows-style roots
- base prompt content asserting thread artifact spec paths instead of `/tmp/*.md`
- generated media persistence asserting files land under `threads/<thread_id>/artifacts/media/`
- preview-path tests if preview storage is migrated in this pass

No history schema migration is required for this design.

## Tradeoffs

Benefits:

- one source of truth for thread-owned durable storage
- prompt/runtime alignment
- better restart and handoff durability
- better thread history because artifacts stay under tamux ownership

Costs:

- some legacy path diversity remains during the transition
- not every durable thread file moves immediately
- a later cleanup pass may still be needed for full unification

## Rejected Alternatives

### Prompt-only fix

Rejected because it leaves runtime path construction fragmented and allows drift to recur.

### Full storage unification in one pass

Rejected for now because it increases migration risk and can break historical references unnecessarily.

## Rollout Notes

- keep the change incremental
- do not rewrite old historical file references in the same pass
- prefer helper adoption in code that already has thread context available
- keep goal-run inventory separate unless there is a later explicit design decision to unify lifecycle models

## Open Question

Whether thread previews move in the first implementation pass can be decided during implementation. The design supports both outcomes, but generated media and prompt-aligned specs are the primary targets.
