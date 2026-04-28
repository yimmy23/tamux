# Zorai React Shell Redesign

Date: 2026-04-27
Status: Approved design draft

## Goal

Replace the current React frontend with a visually new, agent-centric Zorai shell that matches the TUI's thread, goal, workspace, and orchestration model while making terminal multiplexing a secondary tool rather than the main product metaphor.

## Decisions

- The default product surface is a thread-first ChatGPT-like app.
- The current React shell is replaced outright, not hidden behind a temporary switch.
- Visible UI branding changes to Zorai first. Package metadata, binaries, app IDs, domains, protocol names, and bridge aliases are deferred to a later rebrand migration.
- The frontend reuses existing daemon-backed agent runtime behavior where it already works.
- CDUI/YAML is removed from the active frontend path.
- Terminal multiplexing remains available as a secondary tool destination.
- The selected visual direction is Quiet Orchestration: dark, compact, mature, minimal glow, sharp hierarchy, and 8px-or-less radii.

## Product Shell

Zorai opens to a full-screen, thread-first chat workspace. The shell uses a two-rail navigation model:

- The far-left rail is stable global navigation.
- The second rail changes based on the selected area.
- The center is the primary work surface, normally a thread conversation.
- The right panel is contextual and collapsible.

Global navigation contains:

- Threads
- Goals
- Workspaces
- Tools/Terminal
- Activity
- Settings

For Threads, the second rail shows thread search and thread lists. For Goals, it shows active and historical goal runs. For Workspaces, it shows workspace boards and task lists. For Tools, it shows terminal, browser, file, command, system, and session-vault destinations.

The right panel opens when context is useful: selected goal runs, approvals, spawned agents, work context, files, traces, workspace task details, or runtime diagnostics. On desktop the panel can be pinned, but the default state preserves a clean chat-focused center.

## Runtime Strategy

Zorai should reuse the existing agent runtime and daemon bridge behavior where it is already working:

- thread hydration
- message loading and sending
- participant directives and participant suggestions
- spawned-thread navigation
- goal run creation and controls
- approvals
- daemon events
- provider and model settings
- notifications
- audio/STT/TTS
- operator profile flows
- audit and escalation events

The implementation may extract existing panel-shaped runtime code into reusable hooks and services. It should not rewrite daemon-backed behavior only to make the shell look new.

## CDUI/YAML Removal

CDUI/YAML should be removed, not kept as an alternate default path. `frontend/src/main.tsx` should render Zorai directly with no CDUI preference switch.

Removal scope includes:

- `CDUIApp`
- YAML view files under `frontend/src/views`
- CDUI loader, mode, and visibility utilities
- dynamic renderer path
- view builder overlay and store
- base component and command registries used only by CDUI
- CDUI preference persistence and feature switch wiring

Files should be deleted only after imports are removed and the frontend still builds. This removal also deletes the YAML-first "configurable surface" product model.

## Main Views

### Threads

Threads are the default view. The full-screen thread UI supports:

- thread creation, search, selection, and deletion
- message history and streaming
- attachments
- stop streaming
- participant display and actions
- participant suggestions
- pinned and compaction context
- goal launch from composer
- spawned-thread navigation and return paths
- audio features where already available

The existing `agent-chat-panel` behavior can be reused, but the visual shell must be new. The current side-panel presentation should be broken into full-screen primitives instead of stretched wider.

### Goals

Goals adapt the TUI Mission Control model. The view should expose:

- goal run list
- status and lifecycle controls
- plan and step state
- approvals
- active execution thread routing
- files
- checkpoints
- tasks
- usage
- needs-attention state
- runtime roster/context where available

Threads and Goals remain distinct: Goals is the orchestration surface, Threads is the conversation surface. Opening a goal's active thread should preserve a return path back to the goal.

### Workspaces

Workspaces should use the TUI/workspace domain model, not the current React workspace/surface/pane model.

The first Zorai workspace view should show:

- Todo, In Progress, In Review, and Done columns
- workspace tasks
- task title, type, priority, assignee, reviewer, reporter, and status
- task detail
- definition of done
- linked thread or goal target
- task history
- review result and retry loop where available

Workspace boards are planning and review surfaces around threads and goals. They are not pane containers.

### Tools

Tools are secondary destinations for capabilities that are still useful but no longer define the app shell:

- terminal sessions
- file manager
- browser panels
- command log and command history
- system monitor
- session vault
- generated tools where still supported outside CDUI

Tools should be visually framed as operator/agent utilities. They should not restore workspace tabs, surface tabs, pane splits, or the multiplexer-first layout as the default experience.

### Activity

Activity consolidates operational state:

- cognitive and operational events
- approvals
- notifications
- audit actions
- escalation updates
- queue state
- status and health signals

This gives operators a single place to inspect what Zorai and the daemon are doing.

### Settings

Settings keep the existing functional scope but receive new visual treatment:

- providers and models
- tools
- web search
- chat
- gateway
- agent settings
- audio features
- advanced settings
- about/version information

## Visual Design

The selected visual direction is Quiet Orchestration.

Principles:

- dark base with restrained contrast
- compact spacing for repeat operator work
- clear hierarchy between global nav, contextual rail, main thread, and right context
- 8px-or-less border radius except where existing controls require otherwise
- minimal glow and no decorative orbs
- no marketing-style hero layout
- no nested cards
- thread content gets the most space
- orchestration metadata appears as quiet context, not dashboard clutter

The UI should feel visually new even when behavior is reused. Existing panel-shaped components should be refactored into shell-native pieces.

## Implementation Shape

Recommended module structure:

```text
frontend/src/App.tsx
frontend/src/zorai/ZoraiApp.tsx
frontend/src/zorai/shell/
frontend/src/zorai/features/threads/
frontend/src/zorai/features/goals/
frontend/src/zorai/features/workspaces/
frontend/src/zorai/features/tools/
frontend/src/zorai/features/activity/
frontend/src/zorai/features/settings/
frontend/src/zorai/styles/
```

Responsibilities:

- `App.tsx` becomes a small Zorai entry point or delegates to `zorai/ZoraiApp.tsx`.
- `zorai/shell` owns layout, navigation, contextual right panel, responsive behavior, and app-level empty/loading/error states.
- `zorai/features/threads` wraps and refactors the current agent-chat runtime into the default full-screen conversation view.
- `zorai/features/goals` adapts goal-run and TUI Mission Control concepts.
- `zorai/features/workspaces` implements daemon workspace task boards.
- `zorai/features/tools` contains terminal/file/browser/command/system utilities.
- `zorai/features/activity` consolidates events, approvals, audit, and notifications.
- `zorai/styles` defines Quiet Orchestration tokens and reusable layout primitives.

Large files should be split as part of the migration. Avoid creating new files over 500 lines. Current large files such as `App.tsx`, `ChatView.tsx`, and `agent-chat-panel/runtime/layout.tsx` should be decomposed rather than mirrored.

## Migration Plan Shape

The implementation should be staged so each step leaves a buildable frontend:

1. Add Zorai shell and visual tokens while still reusing runtime code.
2. Move Threads into the new shell as the default usable path.
3. Add contextual right panel surfaces for goals, participants, approvals, traces, and work context.
4. Add Goals, Workspaces, Tools, Activity, and Settings destinations.
5. Remove old pane/surface shell imports that no longer serve the product.
6. Remove CDUI/YAML/runtime-builder files and preference plumbing.
7. Run frontend unit tests, lint, build, and browser screenshot checks.

## Testing Strategy

Frontend validation should include:

- `npm run test:unit`
- `npm run lint`
- `npm run build`
- Playwright screenshots of the default shell at desktop and mobile widths

Regression tests should focus on:

- thread selection and creation
- send-message payloads
- participant rendering and actions
- participant suggestions
- goal-run controls
- contextual right panel routing
- global nav and contextual rail selection
- CDUI removal from bootstrap
- settings access after shell replacement

Manual smoke checks should cover:

- opening Zorai to the Threads view
- sending a message to an existing or new thread
- launching a goal from the composer
- opening an active goal in the contextual panel
- opening a terminal from Tools
- opening Settings from global nav

## Risks

- Existing runtime components are panel-shaped and may resist clean extraction.
- Removing CDUI/YAML can break plugin or builder-specific assumptions if imports remain hidden.
- Workspace board behavior may require daemon API gaps or additional bridge wiring.
- The visual redesign can regress functional flows if runtime extraction is too aggressive.
- Existing dirty worktree state should be protected during implementation; unrelated changes must not be reverted.

## Non-Goals

- Full binary/package/protocol rebrand from tamux/amux to Zorai.
- Rewriting daemon APIs before the UI shell exists.
- Making terminal multiplexing the default shell again.
- Keeping CDUI/YAML as an alternate product mode.
- Rebuilding the entire agent runtime from scratch in the first pass.
