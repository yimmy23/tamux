# Shadcn Preset Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the current frontend design with the approved shadcn-preset-inspired visual system while preserving the existing workspace runtime, Electron integrations, and CDUI parity from the first migration phase.

**Architecture:** Use a hybrid shell refactor. Keep the current React/Electron/Zustand runtime and the `ViewProps`/CDUI contract, but introduce a new Tailwind-backed primitive layer and progressively route shared wrappers, shell components, and high-traffic panels through it. Treat the preset as a local design reference, not a literal router template to import unchanged.

**Tech Stack:** React 19, TypeScript 5.6, Electron 33, Vite 6, Zustand 5, Tailwind CSS, shadcn-style primitives, Radix UI primitives where behavior is needed, existing YAML/CDUI renderer

**Spec:** `docs/superpowers/specs/2026-03-23-shadcn-preset-redesign-design.md`

---

## Repo-Specific Guardrails

- There is **no committed frontend unit-test harness**. Do not add Jest/Vitest/Cypress as part of this plan.
- Validate frontend work with the existing commands:
  - `cd frontend && npm run lint`
  - `cd frontend && npm run build`
  - manual Electron smoke checks with `cd frontend && npm run dev:electron`
- Keep the runtime model stable:
  - do **not** introduce React Router as part of this plan,
  - do **not** change Zustand store contracts unless a task explicitly calls for it,
  - do **not** change the `ViewProps` shape used by CDUI.
- Follow repo conventions:
  - PascalCase component files in `frontend/src/components/**`
  - camelCase util files in `frontend/src/lib/**`
  - named exports only
  - keep new files under 500 LOC

---

## File Map

### Modify

- `frontend/package.json` — add Tailwind/shadcn-related dependencies without introducing React Router
- `frontend/package-lock.json` — lockfile update from `npm install`
- `frontend/vite.config.ts` — add the Tailwind Vite plugin to the existing build pipeline
- `frontend/src/main.tsx` — import the Tailwind entrypoint alongside the existing global stylesheet
- `frontend/src/styles/global.css` — reduce to canonical tokens, resets, and temporary compatibility styles
- `frontend/src/lib/themes.ts` — make the app-shell theme writer emit the same token names the new primitives consume
- `frontend/src/CDUIApp.tsx` — migrate CDUI-specific loading/error chrome onto the new design layer
- `frontend/src/components/BaseComponents.tsx` — preserve export names, but delegate rendering to adapter-backed primitives
- `frontend/src/components/EditableShell.tsx` — migrate builder/edit-mode shell chrome
- `frontend/src/components/base-components/propUtils.tsx` — forward wrapper/component class/style data cleanly into the new adapter layer
- `frontend/src/components/base-components/MissionDeck.tsx` — migrate shared shell/dashboard mission chrome
- `frontend/src/components/editable-shell/EditableShellChrome.tsx`
- `frontend/src/components/editable-shell/useEditableShellState.ts`
- `frontend/src/components/TitleBar.tsx`
- `frontend/src/components/Sidebar.tsx`
- `frontend/src/components/sidebar/SidebarActions.tsx`
- `frontend/src/components/sidebar/SidebarHeader.tsx`
- `frontend/src/components/sidebar/SidebarResizeHandle.tsx`
- `frontend/src/components/sidebar/WorkspaceItem.tsx`
- `frontend/src/components/SurfaceTabBar.tsx`
- `frontend/src/components/surface-tab-bar/SurfaceCreateButton.tsx`
- `frontend/src/components/surface-tab-bar/SurfaceTabActions.tsx`
- `frontend/src/components/surface-tab-bar/SurfaceTabButton.tsx`
- `frontend/src/components/surface-tab-bar/SurfaceTabItem.tsx`
- `frontend/src/components/TerminalPane.tsx`
- `frontend/src/components/terminal-pane/TerminalContextMenu.tsx`
- `frontend/src/components/terminal-pane/TerminalPaneHeader.tsx`
- `frontend/src/components/terminal-pane/menuItems.ts`
- `frontend/src/components/terminal-pane/useTerminalClipboard.ts`
- `frontend/src/components/terminal-pane/useTerminalTranscript.ts`
- `frontend/src/components/terminal-pane/utils.ts`
- `frontend/src/components/LayoutContainer.tsx`
- `frontend/src/components/StatusBar.tsx`
- `frontend/src/components/status-bar/InlineSystemMonitor.tsx`
- `frontend/src/components/status-bar/StatusBarMissionStats.tsx`
- `frontend/src/components/status-bar/StatusPrimitives.tsx`
- `frontend/src/components/AppConfirmDialog.tsx`
- `frontend/src/components/AppPromptDialog.tsx`
- `frontend/src/components/LoadingState.tsx`
- `frontend/src/components/ViewBuilderOverlay.tsx`
- `frontend/src/components/InfiniteCanvasSurface.tsx`
- `frontend/src/components/CommandPalette.tsx`
- `frontend/src/components/CommandHistoryPicker.tsx`
- `frontend/src/components/CommandLogPanel.tsx`
- `frontend/src/components/ConciergeToast.tsx`
- `frontend/src/components/command-palette/CommandPaletteHeader.tsx`
- `frontend/src/components/command-palette/CommandPaletteResults.tsx`
- `frontend/src/components/command-log-panel/CommandLogFilters.tsx`
- `frontend/src/components/command-log-panel/CommandLogHeader.tsx`
- `frontend/src/components/command-log-panel/CommandLogTable.tsx`
- `frontend/src/components/NotificationPanel.tsx`
- `frontend/src/components/notification-panel/NotificationHeader.tsx`
- `frontend/src/components/notification-panel/NotificationList.tsx`
- `frontend/src/components/SetupOnboardingPanel.tsx`
- `frontend/src/components/AgentApprovalOverlay.tsx`
- `frontend/src/components/AgentChatPanel.tsx`
- `frontend/src/components/agent-chat-panel/AITrainingView.tsx`
- `frontend/src/components/agent-chat-panel/ChatView.tsx`
- `frontend/src/components/agent-chat-panel/CodingAgentsView.tsx`
- `frontend/src/components/agent-chat-panel/ContextView.tsx`
- `frontend/src/components/agent-chat-panel/SubagentsView.tsx`
- `frontend/src/components/agent-chat-panel/TasksView.tsx`
- `frontend/src/components/agent-chat-panel/ThreadList.tsx`
- `frontend/src/components/agent-chat-panel/TraceView.tsx`
- `frontend/src/components/agent-chat-panel/UsageView.tsx`
- `frontend/src/components/agent-chat-panel/runtime.tsx`
- `frontend/src/components/SettingsPanel.tsx`
- `frontend/src/components/settings-panel/AboutTab.tsx`
- `frontend/src/components/settings-panel/AgentTab.tsx`
- `frontend/src/components/settings-panel/AppearanceTab.tsx`
- `frontend/src/components/settings-panel/BehaviorTab.tsx`
- `frontend/src/components/settings-panel/ConciergeSection.tsx`
- `frontend/src/components/settings-panel/GatewaySettings.tsx`
- `frontend/src/components/settings-panel/GatewayTab.tsx`
- `frontend/src/components/settings-panel/KeyboardTab.tsx`
- `frontend/src/components/settings-panel/ProviderAuthTab.tsx`
- `frontend/src/components/settings-panel/SubAgentsTab.tsx`
- `frontend/src/components/settings-panel/TerminalTab.tsx`
- `frontend/src/components/FileManagerPanel.tsx`
- `frontend/src/components/file-manager-panel/PaneView.tsx`
- `frontend/src/components/file-manager-panel/SshProfilesPanel.tsx`
- `frontend/src/components/SnippetPicker.tsx`
- `frontend/src/components/snippet-picker/Overlay.tsx`
- `frontend/src/components/snippet-picker/SnippetForm.tsx`
- `frontend/src/components/snippet-picker/SnippetListView.tsx`
- `frontend/src/components/snippet-picker/SnippetResolverDialog.tsx`
- `frontend/src/components/SearchOverlay.tsx`
- `frontend/src/components/search-overlay/SearchOverlayControls.tsx`
- `frontend/src/components/search-overlay/SearchOverlayHeader.tsx`
- `frontend/src/components/search-overlay/SearchOverlayStatus.tsx`
- `frontend/src/components/SessionVaultPanel.tsx`
- `frontend/src/components/session-vault-panel/SessionVaultContent.tsx`
- `frontend/src/components/session-vault-panel/SessionVaultFilters.tsx`
- `frontend/src/components/session-vault-panel/SessionVaultHeader.tsx`
- `frontend/src/components/audit-panel/AuditDetailView.tsx`
- `frontend/src/components/audit-panel/AuditHeader.tsx`
- `frontend/src/components/audit-panel/AuditList.tsx`
- `frontend/src/components/audit-panel/AuditPanel.tsx`
- `frontend/src/components/audit-panel/AuditRow.tsx`
- `frontend/src/components/audit-panel/ConfidenceBadge.tsx`
- `frontend/src/components/audit-panel/EscalationBanner.tsx`
- `frontend/src/components/SystemMonitorPanel.tsx`
- `frontend/src/components/system-monitor-panel/SystemMonitorContent.tsx`
- `frontend/src/components/system-monitor-panel/SystemMonitorControls.tsx`
- `frontend/src/components/system-monitor-panel/SystemMonitorHeader.tsx`
- `frontend/src/components/TimeTravelSlider.tsx`
- `frontend/src/components/time-travel-slider/TimeTravelContent.tsx`
- `frontend/src/components/time-travel-slider/TimeTravelHeader.tsx`
- `frontend/src/components/ExecutionCanvas.tsx`
- `frontend/src/components/graph/DataFlowEdge.tsx`
- `frontend/src/components/graph/ToolNode.tsx`
- `frontend/src/components/WebBrowserPanel.tsx`
- `frontend/src/components/web-browser-panel/BrowserChrome.tsx`
- `frontend/src/components/web-browser-panel/CanvasBrowserPane.tsx`
- `frontend/src/components/web-browser-panel/WebviewFrame.tsx`
- `frontend/src/components/web-browser-panel/useWebBrowserController.ts`

### Create

- `frontend/tailwind.config.ts` — Tailwind token mapping to the app’s CSS variables
- `frontend/src/styles/tailwind.css` — Tailwind entrypoint
- `frontend/src/lib/classNameUtils.ts` — `cn()` helper that merges `clsx` with `tailwind-merge`
- `frontend/src/components/ui/shared.ts` — shared variant helpers used by the primitive layer
- `frontend/src/components/ui/index.ts` — named re-exports for the primitive layer
- `frontend/src/components/ui/Button.tsx`
- `frontend/src/components/ui/Badge.tsx`
- `frontend/src/components/ui/Card.tsx`
- `frontend/src/components/ui/Dialog.tsx`
- `frontend/src/components/ui/DropdownMenu.tsx`
- `frontend/src/components/ui/Input.tsx`
- `frontend/src/components/ui/ScrollArea.tsx`
- `frontend/src/components/ui/Select.tsx`
- `frontend/src/components/ui/Separator.tsx`
- `frontend/src/components/ui/Sheet.tsx`
- `frontend/src/components/ui/Tabs.tsx`
- `frontend/src/components/ui/TextArea.tsx`
- `frontend/src/components/base-components/adapters/index.ts`
- `frontend/src/components/base-components/adapters/ButtonAdapter.tsx`
- `frontend/src/components/base-components/adapters/ContainerAdapter.tsx`
- `frontend/src/components/base-components/adapters/DividerAdapter.tsx`
- `frontend/src/components/base-components/adapters/HeaderAdapter.tsx`
- `frontend/src/components/base-components/adapters/InputAdapter.tsx`
- `frontend/src/components/base-components/adapters/SelectAdapter.tsx`
- `frontend/src/components/base-components/adapters/SpacerAdapter.tsx`
- `frontend/src/components/base-components/adapters/TextAdapter.tsx`
- `frontend/src/components/base-components/adapters/TextAreaAdapter.tsx`

### Preserve Unless a Task Proves Otherwise

- `frontend/src/registry/registerBaseComponents.ts` — keep registration flow stable if export names do not change
- `frontend/src/views/*.yaml` — leave YAML documents untouched unless a migrated wrapper requires a specific prop/default cleanup

---

## Task 1: Capture the preset reference and wire the build foundation

**Files:**
- Modify: `frontend/package.json`
- Modify: `frontend/package-lock.json`
- Modify: `frontend/vite.config.ts`
- Modify: `frontend/src/main.tsx`
- Create: `frontend/tailwind.config.ts`
- Create: `frontend/src/styles/tailwind.css`
- Create: `frontend/src/lib/classNameUtils.ts`
- Local scratch only (do not commit): `tmp/shadcn-preset-reference/`

- [ ] **Step 1: Generate a local preset reference outside the app source tree**

Run:

```bash
mkdir -p tmp/shadcn-preset-reference
cd tmp/shadcn-preset-reference
pnpm dlx shadcn@latest init --preset b3QvsBDUY --base base --template react-router
```

Expected: a local reference project exists under `tmp/shadcn-preset-reference/` for inspecting tokens, primitive choices, and layout patterns. Do not copy its router structure into `frontend/`.

- [ ] **Step 2: Add the new UI/tooling dependencies to `frontend/package.json`**

Add only the packages needed by the approved design:

- `tailwindcss`
- `@tailwindcss/vite`
- `class-variance-authority`
- `clsx`
- `tailwind-merge`
- `@radix-ui/react-slot`
- `@radix-ui/react-dialog`
- `@radix-ui/react-dropdown-menu`
- `@radix-ui/react-tabs`
- `@radix-ui/react-scroll-area`
- `@radix-ui/react-select`
- `@radix-ui/react-separator`

Do **not** add `react-router` or `react-router-dom`.

- [ ] **Step 3: Install dependencies and update the lockfile**

Run:

```bash
cd frontend
npm install
```

Expected: `package-lock.json` is updated and the install completes without adding router packages.

- [ ] **Step 4: Create the Tailwind entry files and wire them into Vite and the app**

Create:

- `frontend/tailwind.config.ts`
- `frontend/src/styles/tailwind.css`
- `frontend/src/lib/classNameUtils.ts`

Use `classNameUtils.ts` for the shared class helper:

```ts
import { clsx, type ClassValue } from "clsx";
import { twMerge } from "tailwind-merge";

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}
```

Update:

- `frontend/vite.config.ts` to register the Tailwind Vite plugin in the existing `plugins` array
- `frontend/src/main.tsx` to import `./styles/tailwind.css` alongside `./styles/global.css`

If importing Tailwind base styles causes immediate visual regressions before any primitive migration lands, contain or disable preflight in the Tailwind setup before proceeding.

- [ ] **Step 5: Run the frontend checks before touching UI code**

Run:

```bash
cd frontend
npm run lint
npm run build
```

Expected: both commands exit `0`. At this point there should be no visible app redesign yet.

- [ ] **Step 6: Commit the foundation wiring**

```bash
git add frontend/package.json frontend/package-lock.json frontend/vite.config.ts frontend/tailwind.config.ts frontend/src/main.tsx frontend/src/styles/tailwind.css frontend/src/lib/classNameUtils.ts
git commit -m "feat(frontend): add shadcn redesign foundation"
```

### Task 2: Establish the canonical token bridge and primitive layer

**Files:**
- Modify: `frontend/src/styles/global.css`
- Modify: `frontend/src/lib/themes.ts`
- Create: `frontend/src/components/ui/shared.ts`
- Create: `frontend/src/components/ui/index.ts`
- Create: `frontend/src/components/ui/Button.tsx`
- Create: `frontend/src/components/ui/Badge.tsx`
- Create: `frontend/src/components/ui/Card.tsx`
- Create: `frontend/src/components/ui/Dialog.tsx`
- Create: `frontend/src/components/ui/DropdownMenu.tsx`
- Create: `frontend/src/components/ui/Input.tsx`
- Create: `frontend/src/components/ui/ScrollArea.tsx`
- Create: `frontend/src/components/ui/Select.tsx`
- Create: `frontend/src/components/ui/Separator.tsx`
- Create: `frontend/src/components/ui/Sheet.tsx`
- Create: `frontend/src/components/ui/Tabs.tsx`
- Create: `frontend/src/components/ui/TextArea.tsx`

- [ ] **Step 1: Turn `global.css` into the canonical token file**

Keep:

- semantic color variables (`--agent`, `--human`, `--approval`, `--reasoning`, `--mission`, `--timeline`)
- shell tokens (`--bg-*`, `--text-*`, `--space-*`, `--radius-*`, transition tokens)
- minimal global resets

Start moving component-specific visual rules out of `global.css`. Do not remove tokens that `App.tsx`, shell components, or existing runtime theme logic still depend on.

- [ ] **Step 2: Update `frontend/src/lib/themes.ts` so runtime theme application writes the same token names the primitives use**

The design system must have **one token source**. Extend the existing theme helper instead of creating a parallel token writer.

Expected result: app-shell theme changes still work, and the new Tailwind/UI layer reads the same CSS variables.

- [ ] **Step 3: Create the reusable primitive layer under `frontend/src/components/ui/`**

Implement the first-pass primitives needed by the shell and high-traffic panels:

- `Button`
- `Badge`
- `Card`
- `Dialog`
- `DropdownMenu`
- `Input`
- `ScrollArea`
- `Select`
- `Separator`
- `Sheet`
- `Tabs`
- `TextArea`

Each file should stay focused and use named exports.

- [ ] **Step 4: Re-export the primitives from `frontend/src/components/ui/index.ts`**

Expected: shell components and adapters can import from one stable surface instead of reaching into many files directly.

- [ ] **Step 5: Run the checks after the primitive layer lands**

Run:

```bash
cd frontend
npm run lint
npm run build
```

Expected: the primitive files compile, even if they are not widely used yet.

- [ ] **Step 6: Commit the token bridge and primitive layer**

```bash
git add frontend/src/styles/global.css frontend/src/lib/themes.ts frontend/src/components/ui/
git commit -m "feat(frontend): add shared redesign primitives"
```

### Task 3: Refactor `BaseComponents` onto adapter-backed primitives without breaking CDUI

**Files:**
- Modify: `frontend/src/CDUIApp.tsx`
- Modify: `frontend/src/components/BaseComponents.tsx`
- Modify: `frontend/src/components/EditableShell.tsx`
- Modify: `frontend/src/components/LoadingState.tsx`
- Modify: `frontend/src/components/ViewBuilderOverlay.tsx`
- Modify: `frontend/src/components/base-components/MissionDeck.tsx`
- Modify: `frontend/src/components/base-components/propUtils.tsx`
- Modify: `frontend/src/components/base-components/shared.ts`
- Modify: `frontend/src/components/editable-shell/EditableShellChrome.tsx`
- Modify: `frontend/src/components/editable-shell/useEditableShellState.ts`
- Create: `frontend/src/components/base-components/adapters/index.ts`
- Create: `frontend/src/components/base-components/adapters/ButtonAdapter.tsx`
- Create: `frontend/src/components/base-components/adapters/ContainerAdapter.tsx`
- Create: `frontend/src/components/base-components/adapters/DividerAdapter.tsx`
- Create: `frontend/src/components/base-components/adapters/HeaderAdapter.tsx`
- Create: `frontend/src/components/base-components/adapters/InputAdapter.tsx`
- Create: `frontend/src/components/base-components/adapters/SelectAdapter.tsx`
- Create: `frontend/src/components/base-components/adapters/SpacerAdapter.tsx`
- Create: `frontend/src/components/base-components/adapters/TextAdapter.tsx`
- Create: `frontend/src/components/base-components/adapters/TextAreaAdapter.tsx`

- [ ] **Step 1: Create the adapter files that map `ViewProps` to the new primitives**

Each adapter should:

- accept the current `ViewProps`/`componentProps` contract,
- preserve builder metadata and `EditableShell` behavior,
- render the new primitive layer instead of raw HTML elements.

- [ ] **Step 2: Update `BaseComponents.tsx` to re-export adapter-backed components without changing export names**

Keep exports like `Button`, `Input`, `Text`, `TextArea`, `Select`, `Divider`, `Header`, and `Container` stable so `registerBaseComponents()` and CDUI documents continue to work.

- [ ] **Step 3: Restyle the CDUI-only chrome and builder chrome that still render outside `BaseComponents`**

Update:

- `frontend/src/CDUIApp.tsx` loading/error presentation
- `frontend/src/components/LoadingState.tsx`
- `frontend/src/components/ViewBuilderOverlay.tsx`
- `frontend/src/components/EditableShell.tsx`
- `frontend/src/components/editable-shell/EditableShellChrome.tsx`
- `frontend/src/components/editable-shell/useEditableShellState.ts`

Goal: CDUI mode should not keep an old-only loading/builder visual path after the adapter migration.

- [ ] **Step 4: Migrate `MissionDeck.tsx` onto the same primitive layer**

`MissionDeck` is shared shell chrome used by both the standard app and CDUI dashboard views. Move it onto the new `Button`, `Badge`, `Card`, and `Separator` primitives so it does not remain a visible old-design island.

- [ ] **Step 5: Tighten `propUtils.tsx` and `shared.ts` only where needed to support the adapter layer**

Allowed:

- clarifying prop forwarding for wrapper/component class names and styles,
- documenting the `ViewProps` contract in code comments.

Not allowed:

- changing the `ViewProps` shape,
- introducing a second CDUI contract.

- [ ] **Step 6: Run the frontend checks**

Run:

```bash
cd frontend
npm run lint
npm run build
```

Expected: `BaseComponents` still compiles and `registerBaseComponents.ts` needs no structural rewrite.

- [ ] **Step 7: Run the CDUI smoke check**

Run:

```bash
cd frontend
npm run dev:electron
```

Manual verification:

- the app still boots in the current default mode,
- CDUI mode can still load registered views (if the app opens in standard mode by default, flip the existing CDUI preference and relaunch once for this check),
- no “No fallback component registered” error appears,
- builder/edit-mode chrome still wraps migrated primitives correctly.

- [ ] **Step 8: Commit the CDUI-safe adapter refactor**

```bash
git add frontend/src/CDUIApp.tsx frontend/src/components/BaseComponents.tsx frontend/src/components/EditableShell.tsx frontend/src/components/LoadingState.tsx frontend/src/components/ViewBuilderOverlay.tsx frontend/src/components/base-components/ frontend/src/components/editable-shell/ frontend/src/components/ui/ frontend/src/lib/themes.ts
git commit -m "refactor(frontend): route base components through redesign adapters"
```

### Task 4: Migrate the shell components to the new design layer

**Files:**
- Modify: `frontend/src/App.tsx`
- Modify: `frontend/src/components/TitleBar.tsx`
- Modify: `frontend/src/components/Sidebar.tsx`
- Modify: `frontend/src/components/sidebar/SidebarActions.tsx`
- Modify: `frontend/src/components/sidebar/SidebarHeader.tsx`
- Modify: `frontend/src/components/sidebar/SidebarResizeHandle.tsx`
- Modify: `frontend/src/components/sidebar/WorkspaceItem.tsx`
- Modify: `frontend/src/components/SurfaceTabBar.tsx`
- Modify: `frontend/src/components/surface-tab-bar/SurfaceCreateButton.tsx`
- Modify: `frontend/src/components/surface-tab-bar/SurfaceTabActions.tsx`
- Modify: `frontend/src/components/surface-tab-bar/SurfaceTabButton.tsx`
- Modify: `frontend/src/components/surface-tab-bar/SurfaceTabItem.tsx`
- Modify: `frontend/src/components/TerminalPane.tsx`
- Modify: `frontend/src/components/terminal-pane/TerminalContextMenu.tsx`
- Modify: `frontend/src/components/terminal-pane/TerminalPaneHeader.tsx`
- Modify: `frontend/src/components/terminal-pane/menuItems.ts`
- Modify: `frontend/src/components/terminal-pane/useTerminalClipboard.ts`
- Modify: `frontend/src/components/terminal-pane/useTerminalTranscript.ts`
- Modify: `frontend/src/components/terminal-pane/utils.ts`
- Modify: `frontend/src/components/InfiniteCanvasSurface.tsx`
- Modify: `frontend/src/components/LayoutContainer.tsx`
- Modify: `frontend/src/components/StatusBar.tsx`
- Modify: `frontend/src/components/status-bar/InlineSystemMonitor.tsx`
- Modify: `frontend/src/components/status-bar/StatusBarMissionStats.tsx`
- Modify: `frontend/src/components/status-bar/StatusPrimitives.tsx`

- [ ] **Step 1: Migrate `TitleBar.tsx` to the new primitives without changing its window/menu behavior**

Use the new `Button`, `DropdownMenu`, `Badge`, and `Separator` primitives where they fit. Preserve:

- Linux title menus
- notification counts
- window-state bridge wiring
- all hotkey-driven actions

- [ ] **Step 2: Migrate `Sidebar.tsx` and its subcomponents**

Move the tree, header, quick actions, and resize affordance onto the new design layer while preserving:

- workspace/surface selection
- rename flows
- context menus
- drag-to-resize
- pane duplication helpers

- [ ] **Step 3: Migrate `SurfaceTabBar.tsx` and its supporting files**

Use the new `Tabs`, `Button`, `Badge`, `DropdownMenu`, and `Separator` primitives where appropriate. Preserve:

- split actions
- duplicate terminal actions
- web browser/canvas actions
- close/rename/icon flows

- [ ] **Step 4: Restyle `LayoutContainer.tsx`, `TerminalPane.tsx`, `InfiniteCanvasSurface.tsx`, and `StatusBar.tsx` without touching layout/store logic**

Allowed:

- empty-state UI rewrite
- terminal-pane header/chrome migration
- canvas-surface visual migration
- pane frame styling
- split-handle styling
- status chip/button rewrites

Not allowed:

- BSP tree logic changes
- workspace-store behavior changes

- [ ] **Step 5: Run the frontend checks**

Run:

```bash
cd frontend
npm run lint
npm run build
```

Expected: the shell compiles cleanly after the migration.

- [ ] **Step 6: Run the shell smoke check in Electron**

Run:

```bash
cd frontend
npm run dev:electron
```

Manual verification:

- app launches with the redesigned shell
- title bar menus open and close correctly
- sidebar can resize
- workspace and surface switching still work
- pane split/zoom flows still work
- terminal pane headers, context menus, and transcript/clipboard affordances still work
- canvas surfaces still render through the migrated shell styling
- status bar metrics still update

- [ ] **Step 7: Commit the shell migration**

```bash
git add frontend/src/App.tsx frontend/src/components/TitleBar.tsx frontend/src/components/Sidebar.tsx frontend/src/components/sidebar/ frontend/src/components/SurfaceTabBar.tsx frontend/src/components/surface-tab-bar/ frontend/src/components/TerminalPane.tsx frontend/src/components/terminal-pane/ frontend/src/components/InfiniteCanvasSurface.tsx frontend/src/components/LayoutContainer.tsx frontend/src/components/StatusBar.tsx frontend/src/components/status-bar/
git commit -m "feat(frontend): migrate app shell to redesign primitives"
```

### Task 5: Migrate dialogs, high-traffic panels, and the remaining live surfaces

**Files:**
- Modify: `frontend/src/components/AppConfirmDialog.tsx`
- Modify: `frontend/src/components/AppPromptDialog.tsx`
- Modify: `frontend/src/components/CommandPalette.tsx`
- Modify: `frontend/src/components/CommandHistoryPicker.tsx`
- Modify: `frontend/src/components/CommandLogPanel.tsx`
- Modify: `frontend/src/components/ConciergeToast.tsx`
- Modify: `frontend/src/components/command-palette/CommandPaletteHeader.tsx`
- Modify: `frontend/src/components/command-palette/CommandPaletteResults.tsx`
- Modify: `frontend/src/components/command-log-panel/CommandLogFilters.tsx`
- Modify: `frontend/src/components/command-log-panel/CommandLogHeader.tsx`
- Modify: `frontend/src/components/command-log-panel/CommandLogTable.tsx`
- Modify: `frontend/src/components/NotificationPanel.tsx`
- Modify: `frontend/src/components/notification-panel/NotificationHeader.tsx`
- Modify: `frontend/src/components/notification-panel/NotificationList.tsx`
- Modify: `frontend/src/components/SetupOnboardingPanel.tsx`
- Modify: `frontend/src/components/AgentApprovalOverlay.tsx`
- Modify: `frontend/src/components/AgentChatPanel.tsx`
- Modify: `frontend/src/components/agent-chat-panel/AITrainingView.tsx`
- Modify: `frontend/src/components/agent-chat-panel/ChatView.tsx`
- Modify: `frontend/src/components/agent-chat-panel/CodingAgentsView.tsx`
- Modify: `frontend/src/components/agent-chat-panel/ContextView.tsx`
- Modify: `frontend/src/components/agent-chat-panel/SubagentsView.tsx`
- Modify: `frontend/src/components/agent-chat-panel/TasksView.tsx`
- Modify: `frontend/src/components/agent-chat-panel/ThreadList.tsx`
- Modify: `frontend/src/components/agent-chat-panel/TraceView.tsx`
- Modify: `frontend/src/components/agent-chat-panel/UsageView.tsx`
- Modify: `frontend/src/components/agent-chat-panel/runtime.tsx`
- Modify: `frontend/src/components/SettingsPanel.tsx`
- Modify: `frontend/src/components/settings-panel/AboutTab.tsx`
- Modify: `frontend/src/components/settings-panel/AgentTab.tsx`
- Modify: `frontend/src/components/settings-panel/AppearanceTab.tsx`
- Modify: `frontend/src/components/settings-panel/BehaviorTab.tsx`
- Modify: `frontend/src/components/settings-panel/ConciergeSection.tsx`
- Modify: `frontend/src/components/settings-panel/GatewaySettings.tsx`
- Modify: `frontend/src/components/settings-panel/GatewayTab.tsx`
- Modify: `frontend/src/components/settings-panel/KeyboardTab.tsx`
- Modify: `frontend/src/components/settings-panel/ProviderAuthTab.tsx`
- Modify: `frontend/src/components/settings-panel/SubAgentsTab.tsx`
- Modify: `frontend/src/components/settings-panel/TerminalTab.tsx`
- Modify: `frontend/src/components/FileManagerPanel.tsx`
- Modify: `frontend/src/components/file-manager-panel/PaneView.tsx`
- Modify: `frontend/src/components/file-manager-panel/SshProfilesPanel.tsx`
- Modify: `frontend/src/components/SnippetPicker.tsx`
- Modify: `frontend/src/components/snippet-picker/Overlay.tsx`
- Modify: `frontend/src/components/snippet-picker/SnippetForm.tsx`
- Modify: `frontend/src/components/snippet-picker/SnippetListView.tsx`
- Modify: `frontend/src/components/snippet-picker/SnippetResolverDialog.tsx`
- Modify: `frontend/src/components/SearchOverlay.tsx`
- Modify: `frontend/src/components/search-overlay/SearchOverlayControls.tsx`
- Modify: `frontend/src/components/search-overlay/SearchOverlayHeader.tsx`
- Modify: `frontend/src/components/search-overlay/SearchOverlayStatus.tsx`
- Modify: `frontend/src/components/SessionVaultPanel.tsx`
- Modify: `frontend/src/components/session-vault-panel/SessionVaultContent.tsx`
- Modify: `frontend/src/components/session-vault-panel/SessionVaultFilters.tsx`
- Modify: `frontend/src/components/session-vault-panel/SessionVaultHeader.tsx`
- Modify: `frontend/src/components/audit-panel/AuditDetailView.tsx`
- Modify: `frontend/src/components/audit-panel/AuditHeader.tsx`
- Modify: `frontend/src/components/audit-panel/AuditList.tsx`
- Modify: `frontend/src/components/audit-panel/AuditPanel.tsx`
- Modify: `frontend/src/components/audit-panel/AuditRow.tsx`
- Modify: `frontend/src/components/audit-panel/ConfidenceBadge.tsx`
- Modify: `frontend/src/components/audit-panel/EscalationBanner.tsx`
- Modify: `frontend/src/components/SystemMonitorPanel.tsx`
- Modify: `frontend/src/components/system-monitor-panel/SystemMonitorContent.tsx`
- Modify: `frontend/src/components/system-monitor-panel/SystemMonitorControls.tsx`
- Modify: `frontend/src/components/system-monitor-panel/SystemMonitorHeader.tsx`
- Modify: `frontend/src/components/TimeTravelSlider.tsx`
- Modify: `frontend/src/components/time-travel-slider/TimeTravelContent.tsx`
- Modify: `frontend/src/components/time-travel-slider/TimeTravelHeader.tsx`
- Modify: `frontend/src/components/ExecutionCanvas.tsx`
- Modify: `frontend/src/components/graph/DataFlowEdge.tsx`
- Modify: `frontend/src/components/graph/ToolNode.tsx`
- Modify: `frontend/src/components/WebBrowserPanel.tsx`
- Modify: `frontend/src/components/web-browser-panel/BrowserChrome.tsx`
- Modify: `frontend/src/components/web-browser-panel/CanvasBrowserPane.tsx`
- Modify: `frontend/src/components/web-browser-panel/WebviewFrame.tsx`
- Modify: `frontend/src/components/web-browser-panel/useWebBrowserController.ts`

- [ ] **Step 1: Convert the confirm/prompt dialogs to the new dialog primitives**

Update `AppConfirmDialog.tsx` and `AppPromptDialog.tsx` first. These files are the lowest-risk proof that the overlay model works with the new design layer.

- [ ] **Step 2: Migrate the command/search surfaces**

Update the command-palette and search-overlay files to use `Sheet`, `Input`, `ScrollArea`, `Card`, `Badge`, and `Separator` primitives while preserving:

- keyboard focus behavior
- result filtering
- overlay open/close behavior

- [ ] **Step 3: Migrate the notification, session-vault, file-manager, snippet, system-monitor, and time-travel surfaces**

Preserve all existing data flows and filters. Do not rewrite store logic while changing the visual layer.

- [ ] **Step 4: Migrate the remaining live overlays and work surfaces that `App.tsx` mounts**

This step must cover the rest of the mounted UI so the final cleanup task can truly remove the long-term hybrid:

- `AgentApprovalOverlay`
- `AgentChatPanel` and its subviews
- `ConciergeToast`
- `CommandHistoryPicker`
- `CommandLogPanel` and its subviews
- `SetupOnboardingPanel`
- `audit-panel/*`
- `ExecutionCanvas` and `graph/*`
- `WebBrowserPanel` and its browser chrome/frame helpers

Keep behavior stable; this is a visual migration, not a runtime rewrite.

- [ ] **Step 5: Migrate the settings panel and its tab components**

The new settings UI should share the same primitives and tokens as the shell while preserving:

- tab switching
- provider forms
- gateway settings
- keyboard settings
- appearance/theme controls

- [ ] **Step 6: Run the frontend checks**

Run:

```bash
cd frontend
npm run lint
npm run build
```

Expected: all migrated panels compile and type-check cleanly.

- [ ] **Step 7: Run the panel smoke check in Electron**

Run:

```bash
cd frontend
npm run dev:electron
```

Manual verification:

- confirm/prompt dialogs open and close correctly
- command palette still searches and selects actions
- command history and command log still render and filter correctly
- notifications panel still renders unread/read state
- settings tabs still save through existing stores
- file manager and session vault still render and filter correctly
- search overlay still updates status and results
- onboarding, concierge, and audit surfaces still render correctly
- agent chat, system monitor, snippet picker, time travel, execution canvas, and web browser surfaces still open and render correctly
- verify both the standard app path and CDUI path during the Electron smoke pass

- [ ] **Step 8: Commit the panel migration**

```bash
git add frontend/src/components/AppConfirmDialog.tsx frontend/src/components/AppPromptDialog.tsx frontend/src/components/CommandPalette.tsx frontend/src/components/CommandHistoryPicker.tsx frontend/src/components/CommandLogPanel.tsx frontend/src/components/ConciergeToast.tsx frontend/src/components/command-palette/ frontend/src/components/command-log-panel/ frontend/src/components/NotificationPanel.tsx frontend/src/components/notification-panel/ frontend/src/components/SettingsPanel.tsx frontend/src/components/settings-panel/ frontend/src/components/SetupOnboardingPanel.tsx frontend/src/components/AgentApprovalOverlay.tsx frontend/src/components/AgentChatPanel.tsx frontend/src/components/agent-chat-panel/ frontend/src/components/FileManagerPanel.tsx frontend/src/components/file-manager-panel/ frontend/src/components/SnippetPicker.tsx frontend/src/components/snippet-picker/ frontend/src/components/SearchOverlay.tsx frontend/src/components/search-overlay/ frontend/src/components/SessionVaultPanel.tsx frontend/src/components/session-vault-panel/ frontend/src/components/audit-panel/ frontend/src/components/SystemMonitorPanel.tsx frontend/src/components/system-monitor-panel/ frontend/src/components/TimeTravelSlider.tsx frontend/src/components/time-travel-slider/ frontend/src/components/ExecutionCanvas.tsx frontend/src/components/graph/ frontend/src/components/WebBrowserPanel.tsx frontend/src/components/web-browser-panel/
git commit -m "feat(frontend): migrate core panels to redesign primitives"
```

### Task 6: Remove legacy visual duplication and run the final validation pass

**Files:**
- Modify: `frontend/src/styles/global.css`
- Modify: `frontend/src/components/BaseComponents.tsx`
- Modify: any touched shell/panel file that still carries redundant inline visual styles after Tasks 1-5

- [ ] **Step 1: Remove obsolete component-specific CSS and redundant inline visual styles**

Keep only:

- the canonical token definitions
- minimal resets
- temporary compatibility rules that are still actively used

If a migrated component still has a large inline style block that duplicates the new primitive layer, remove it in this task.

- [ ] **Step 2: Audit the touched files for leftover dual-system styling**

Specifically check for:

- duplicated border/background/spacing tokens in both CSS and Tailwind classes
- legacy button/input/dialog styling that is no longer referenced
- old inline color constants that should now come from the token layer

- [ ] **Step 3: Run the full frontend verification**

Run:

```bash
cd frontend
npm run lint
npm run build
```

Expected: both commands exit `0` with the final migrated design in place.

- [ ] **Step 4: Run the final Electron smoke check for both standard and CDUI modes**

Run:

```bash
cd frontend
npm run dev:electron
```

Manual verification checklist:

- standard app mode boots cleanly
- CDUI mode boots cleanly
- shell navigation works
- dialogs and overlays work
- high-traffic panels render without obvious legacy styling bleed-through
- onboarding, concierge, and audit surfaces match the migrated design system
- the remaining live surfaces (`AgentChatPanel`, `CommandLogPanel`, `ExecutionCanvas`, `WebBrowserPanel`, `SystemMonitorPanel`, `TimeTravelSlider`, `SnippetPicker`) no longer rely on the old long-term visual layer
- Electron title bar/window affordances still behave correctly

- [ ] **Step 5: Commit the cleanup and final validation**

```bash
git add frontend/src/styles/global.css frontend/src/components/ frontend/src/lib/themes.ts
git commit -m "refactor(frontend): remove legacy styling after redesign migration"
```

---

## Implementation Notes for the Engineer

- Prefer modifying the existing shell files and subdirectories over creating a second shell abstraction.
- Keep `BaseComponents` export names stable. The point of the adapter layer is to avoid breaking CDUI registration and YAML documents.
- If a primitive grows past 500 LOC, split its behavior helpers into nearby files under `frontend/src/components/ui/`.
- If a task proves a file in `frontend/src/components/settings-panel/` or another panel directory needs to be split for readability, do the split in the same task instead of carrying a giant file forward.
- Treat the preset reference as an input for tokens/layout choices, not a source of router architecture or naming conventions that conflict with this repo.
