# Spawned Agent Thread Navigation Design

## Summary

Add a dedicated `Spawned Agents` panel to both the TUI and Electron chat surfaces so operators can follow spawned-agent work through a full nested tree, open a child thread directly, and return to the exact prior thread via a push/pop navigation stack.

## User-Facing Behavior

- Both TUI and Electron expose a separate `Spawned Agents` panel next to the active thread view.
- The panel shows the full spawned-agent tree, not just direct children.
- Each node shows:
  - title,
  - status,
  - runtime and session hints when available,
  - whether chat is openable for that node.
- Opening a child thread from this panel pushes the current thread id onto a thread-history stack and switches the active thread to the child.
- A `Back` action pops the last thread id from the stack and returns to that exact prior thread.
- Nodes without a `thread_id` remain visible in the tree but cannot open chat yet.
- Existing task inspectors may reuse the same nested tree model, but the primary interaction lives in the dedicated panel.

## Design Constraints

- Keep spawned-agent navigation out of the transcript body; use a separate panel/sidebar.
- Use one shared navigation model for TUI and Electron so behavior stays aligned.
- Prefer existing daemon metadata (`thread_id`, `parent_task_id`, `parent_thread_id`) over inventing a second relationship store.
- Opening a child thread must not destroy task/run context or flatten the visible nesting.
- Back navigation must be explicit stack behavior, not heuristic parent lookup.

## Technical Approach

### Shared model

- Add `thread_history_stack: Vec<String>` to client-side chat state in both UIs.
- Derive a `spawned_agent_tree` view model from existing task/run records.
- Use `parent_task_id` as the primary nesting edge.
- Use `parent_thread_id` only to find the visible root when the active thread is a parent thread rather than a child task thread.
- Mark nodes as:
  - `openable` when `thread_id` exists,
  - `live` when status is non-terminal,
  - `selected` when the node matches the current thread/task context.

### Navigation semantics

- `open_spawned_thread(from_thread_id, to_thread_id)`:
  - pushes `from_thread_id` unless it is already the top stack entry,
  - switches to `to_thread_id`,
  - recomputes the visible spawned-agent tree for the new thread context.
- `go_back_thread()`:
  - pops one thread id,
  - skips deleted or unavailable threads until a valid target is found,
  - disables itself when the stack is empty.
- Thread switches that happen outside the `Spawned Agents` panel do not mutate the stack.

### Electron

- Add a collapsible `Spawned Agents` side panel inside the existing agent chat workspace.
- Render the tree with per-node actions:
  - `Open Chat`,
  - `Inspect Task`,
  - `Open Session` when workspace/session data exists.
- Reuse the existing task/run data already surfaced in `TasksView`, but make the new panel thread-centric instead of task-centric.
- Keep the current task inspector compatible by reusing the same tree model where practical.

### TUI

- Add a dedicated `Spawned Agents` panel separate from the thread picker.
- Support keyboard navigation across the nested tree.
- `Enter` on an openable child thread pushes the current thread and opens the child.
- `Backspace` or another dedicated back action pops the thread-history stack.
- Show a small active-thread hint when the stack is non-empty so returning is discoverable.

## Edge Cases

- Child task exists before its thread exists:
  - show the node,
  - disable `Open Chat`.
- Parent task metadata is missing but descendant threads still exist:
  - render the partial tree instead of failing closed.
- Consecutive navigation `parent -> child -> grandchild` builds stack `[parent, child]`.
- Reopening the same child thread repeatedly should not push duplicate consecutive stack entries.
- If the popped thread no longer exists, continue popping until a valid thread is found or the stack is empty.

## Risks

- Thread-root inference may be ambiguous when daemon state is incomplete; partial-tree rendering is required to avoid hiding active child work.
- Two separate UI implementations can drift if they do not share the same navigation semantics and derived tree rules.
- Existing task-centric frontend controls may overlap visually with the new thread-centric panel if the reuse boundary is not kept clear.

## Validation

- Add tree-derivation tests for:
  - full nesting by `parent_task_id`,
  - root resolution by `parent_thread_id`,
  - partial trees with missing intermediate nodes.
- Add navigation-stack tests for:
  - push on child open,
  - no duplicate consecutive push,
  - ordered pop behavior,
  - skipping invalid threads during pop.
- Add UI tests for:
  - empty-state rendering,
  - disabled nodes without `thread_id`,
  - live status display,
  - `parent -> child -> grandchild -> back -> back`.
- Add focused TUI and Electron tests to confirm that thread switches outside the panel do not corrupt the stack.
