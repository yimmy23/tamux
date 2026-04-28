# Zorai React Shell Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the React frontend shell with a visually new Zorai thread-first orchestration shell, reuse existing agent runtime behavior, and remove CDUI/YAML from bootstrap.

**Architecture:** `App.tsx` becomes a thin Zorai entry point. New `frontend/src/zorai` modules own shell navigation, visual tokens, feature destinations, and contextual panels while reusing the existing `agent-chat-panel` runtime for live thread behavior. CDUI files are removed after bootstrap and imports no longer reference them.

**Tech Stack:** React 19, TypeScript, Zustand stores, existing Electron bridge APIs, Vitest, Vite.

---

## File Structure

- Create: `frontend/src/zorai/ZoraiApp.tsx` - root Zorai composition and event listeners formerly needed at app level.
- Create: `frontend/src/zorai/shell/navigation.ts` - app nav model, labels, and default view helpers.
- Create: `frontend/src/zorai/shell/navigation.test.ts` - verifies default route and visible destinations.
- Create: `frontend/src/zorai/shell/ZoraiShell.tsx` - two-rail layout, global nav, contextual rail, main surface, right panel slot.
- Create: `frontend/src/zorai/shell/ZoraiContextPanel.tsx` - collapsible contextual right panel.
- Create: `frontend/src/zorai/features/threads/ThreadsView.tsx` - full-screen thread experience using existing agent runtime context.
- Create: `frontend/src/zorai/features/goals/GoalsView.tsx` - goal run overview adapted from existing goal runtime data.
- Create: `frontend/src/zorai/features/workspaces/WorkspacesView.tsx` - workspace-board placeholder backed by available workspace state, explicitly not pane-first.
- Create: `frontend/src/zorai/features/tools/ToolsView.tsx` - secondary tools destination.
- Create: `frontend/src/zorai/features/activity/ActivityView.tsx` - consolidated events/approvals/notifications.
- Create: `frontend/src/zorai/features/settings/SettingsView.tsx` - visually integrated wrapper around existing settings.
- Create: `frontend/src/zorai/styles/zorai.css` - Quiet Orchestration visual system.
- Modify: `frontend/src/App.tsx` - replace old pane/mux shell with `ZoraiApp`.
- Modify: `frontend/src/main.tsx` - remove CDUI preference/bootstrap path and render `App` directly.
- Modify: `frontend/src/styles/global.css` - import Zorai CSS.
- Delete when unused: `frontend/src/CDUIApp.tsx`, `frontend/src/views/*.yaml`, `frontend/src/lib/cduiMode.ts`, `frontend/src/lib/cduiLoader.ts`, `frontend/src/lib/cduiVisibility.ts`, `frontend/src/renderers/*`, `frontend/src/registry/*`, `frontend/src/schemas/uiSchema.ts`, `frontend/src/components/ViewBuilderOverlay.tsx`, and `frontend/src/components/view-builder-overlay/*`.

## Task 1: Navigation Model And Bootstrap Test

**Files:**
- Create: `frontend/src/zorai/shell/navigation.ts`
- Create: `frontend/src/zorai/shell/navigation.test.ts`

- [ ] **Step 1: Write failing navigation tests**

```ts
import { describe, expect, it } from "vitest";
import { getDefaultZoraiView, zoraiNavItems } from "./navigation";

describe("Zorai navigation", () => {
  it("opens to threads by default", () => {
    expect(getDefaultZoraiView()).toBe("threads");
  });

  it("exposes the agent-centric top-level destinations", () => {
    expect(zoraiNavItems.map((item) => item.id)).toEqual([
      "threads",
      "goals",
      "workspaces",
      "tools",
      "activity",
      "settings",
    ]);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd frontend && npm run test:unit -- src/zorai/shell/navigation.test.ts`
Expected: FAIL because `./navigation` does not exist.

- [ ] **Step 3: Implement navigation model**

Create `navigation.ts` with the `ZoraiViewId`, `ZoraiNavItem`, `zoraiNavItems`, and `getDefaultZoraiView` exports.

- [ ] **Step 4: Run test to verify it passes**

Run: `cd frontend && npm run test:unit -- src/zorai/shell/navigation.test.ts`
Expected: PASS.

## Task 2: Zorai Shell And Thread Runtime

**Files:**
- Create: `frontend/src/zorai/ZoraiApp.tsx`
- Create: `frontend/src/zorai/shell/ZoraiShell.tsx`
- Create: `frontend/src/zorai/shell/ZoraiContextPanel.tsx`
- Create: `frontend/src/zorai/features/threads/ThreadsView.tsx`
- Create: `frontend/src/zorai/styles/zorai.css`
- Modify: `frontend/src/App.tsx`
- Modify: `frontend/src/styles/global.css`

- [ ] **Step 1: Extend navigation tests**

Extend `navigation.test.ts` with assertions that every visible destination has shell-facing metadata.

- [ ] **Step 2: Run test to verify it fails if metadata is missing**

Run: `cd frontend && npm run test:unit -- src/zorai/shell/navigation.test.ts`
Expected: FAIL until shell-facing metadata exists.

- [ ] **Step 3: Implement shell**

Implement a two-rail shell with Quiet Orchestration CSS. Wrap the shell in `AgentChatPanelProvider` so the existing runtime context remains available. Use the current runtime `ChatView`, `ThreadList`, `SpawnedAgentsPanel`, `TraceView`, `ContextView`, `TasksView`, and related surfaces where useful, but present them inside the new shell.

- [ ] **Step 4: Run focused tests**

Run: `cd frontend && npm run test:unit -- src/zorai/shell/navigation.test.ts src/components/agent-chat-panel/runtime/useAgentChatPanelProviderValue.test.ts`
Expected: PASS.

## Task 3: Secondary Destinations

**Files:**
- Create: `frontend/src/zorai/features/goals/GoalsView.tsx`
- Create: `frontend/src/zorai/features/workspaces/WorkspacesView.tsx`
- Create: `frontend/src/zorai/features/tools/ToolsView.tsx`
- Create: `frontend/src/zorai/features/activity/ActivityView.tsx`
- Create: `frontend/src/zorai/features/settings/SettingsView.tsx`

- [ ] **Step 1: Write failing navigation coverage**

Add a test that each nav item has a unique id, non-empty label, and contextual rail label.

- [ ] **Step 2: Run test to verify it fails if metadata is missing**

Run: `cd frontend && npm run test:unit -- src/zorai/shell/navigation.test.ts`
Expected: FAIL until metadata is complete.

- [ ] **Step 3: Implement destinations**

Create lightweight but functional destination views backed by existing stores/components: goals from mission/runtime data, workspaces from workspace state but board-oriented copy, tools wrapping existing tool panels, activity from mission/audit/notification stores, settings wrapping existing `SettingsPanel`.

- [ ] **Step 4: Run focused tests**

Run: `cd frontend && npm run test:unit -- src/zorai/shell/navigation.test.ts`
Expected: PASS.

## Task 4: Remove CDUI/YAML Bootstrap

**Files:**
- Modify: `frontend/src/main.tsx`
- Delete: `frontend/src/CDUIApp.tsx`
- Delete: CDUI/YAML-only files listed in File Structure when no imports remain.

- [ ] **Step 1: Remove CDUI bootstrap**

Update `main.tsx` to hydrate normal stores and render `<App />` directly. Remove `hydrateCDUIPreference`, `isCDUIEnabled`, `RuntimeModeBanner`, and `CDUIApp` imports from bootstrap.

- [ ] **Step 2: Delete CDUI/YAML files after import cleanup**

Use `rg "CDUI|cdui|views/|DynamicRenderer|ViewBuilder|registerBase|uiSchema"` to find remaining references, then delete only unused CDUI files.

- [ ] **Step 3: Verify no CDUI references remain in active bootstrap**

Run: `rg -n "CDUI|cdui|CDUIApp|views/" frontend/src/main.tsx frontend/src/App.tsx frontend/src/zorai`
Expected: no output.

## Task 5: Verification

**Files:**
- No new files unless fixing test/build failures.

- [ ] **Step 1: Run unit tests**

Run: `cd frontend && npm run test:unit`
Expected: PASS.

- [ ] **Step 2: Run lint**

Run: `cd frontend && npm run lint`
Expected: PASS or report pre-existing issues clearly.

- [ ] **Step 3: Run build**

Run: `cd frontend && npm run build`
Expected: PASS.

- [ ] **Step 4: Run dev server for manual inspection**

Run: `cd frontend && npm run dev -- --host 127.0.0.1 --port 5174`
Expected: Vite serves Zorai shell.

- [ ] **Step 5: Browser screenshot smoke**

Use Playwright to capture desktop and mobile screenshots of the default shell and verify it is nonblank, dark themed, thread-first, two-rail, and visually Zorai-branded.
