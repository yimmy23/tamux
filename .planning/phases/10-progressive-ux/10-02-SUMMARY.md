---
phase: 10-progressive-ux
plan: 02
subsystem: ui
tags: [typescript, electron, bridge, type-safety, refactoring]

# Dependency graph
requires: []
provides:
  - "Canonical getBridge() accessor in frontend/src/lib/bridge.ts"
  - "Type-safe AmuxBridge declarations covering all runtime IPC methods"
  - "Zero unsafe (window as any) casts for bridge access across frontend"
affects: [10-progressive-ux]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Single shared bridge accessor pattern via getBridge() import"
    - "AmuxBridge type declarations for all Electron IPC methods"

key-files:
  created:
    - "frontend/src/lib/bridge.ts"
  modified:
    - "frontend/src/types/amux-bridge.d.ts"
    - "frontend/src/lib/agentStore.ts"
    - "frontend/src/lib/agentWorkspace.ts"
    - "frontend/src/lib/agentWorkContext.ts"
    - "frontend/src/lib/agentTodos.ts"
    - "frontend/src/lib/goalRuns.ts"
    - "frontend/src/lib/persistence.ts"
    - "frontend/src/lib/agentClient.ts"
    - "frontend/src/lib/agentDaemonConfig.ts"
    - "frontend/src/lib/agentMissionStore.ts"
    - "frontend/src/lib/agentRuns.ts"
    - "frontend/src/lib/agentTaskQueue.ts"
    - "frontend/src/lib/agentTools.ts"
    - "frontend/src/lib/commandLogStore.ts"
    - "frontend/src/lib/paneDuplication.ts"
    - "frontend/src/lib/transcriptStore.ts"
    - "frontend/src/lib/workspaceStore.ts"
    - "frontend/src/App.tsx"
    - "frontend/src/CDUIApp.tsx"
    - "frontend/src/components/AgentApprovalOverlay.tsx"
    - "frontend/src/components/CommandPalette.tsx"
    - "frontend/src/components/NotificationPanel.tsx"
    - "frontend/src/components/SettingsPanel.tsx"
    - "frontend/src/components/StatusBar.tsx"
    - "frontend/src/components/SystemMonitorPanel.tsx"
    - "frontend/src/components/TaskTray.tsx"
    - "frontend/src/components/TerminalPane.tsx"
    - "frontend/src/components/TimeTravelSlider.tsx"
    - "frontend/src/components/TitleBar.tsx"
    - "frontend/src/components/agent-chat-panel/ContextView.tsx"
    - "frontend/src/components/agent-chat-panel/SubagentsView.tsx"
    - "frontend/src/components/agent-chat-panel/TasksView.tsx"
    - "frontend/src/components/agent-chat-panel/runtime.tsx"
    - "frontend/src/components/base-components/AppRuntimeBridge.tsx"
    - "frontend/src/components/settings-panel/AboutTab.tsx"
    - "frontend/src/components/settings-panel/AgentTab.tsx"
    - "frontend/src/components/settings-panel/BehaviorTab.tsx"
    - "frontend/src/components/settings-panel/GatewayTab.tsx"
    - "frontend/src/components/settings-panel/ProviderAuthTab.tsx"
    - "frontend/src/components/settings-panel/shared.tsx"
    - "frontend/src/components/status-bar/InlineSystemMonitor.tsx"
    - "frontend/src/components/terminal-pane/useTerminalClipboard.ts"

key-decisions:
  - "AmuxBridge type expanded with 30+ missing method declarations to replace any-typed casts"
  - "persistence.ts updated from undefined to null return without changing call site behavior (null?.method returns undefined)"
  - "getAgentBridge() in agentDaemonConfig.ts delegates to shared getBridge() rather than being eliminated"

patterns-established:
  - "Bridge access: always import getBridge from @/lib/bridge, never use (window as any)"
  - "Optional chaining for bridge methods: getBridge()?.methodName?.(args)"

requirements-completed: [PRUX-05]

# Metrics
duration: 13min
completed: 2026-03-24
---

# Phase 10 Plan 02: Shared getBridge() Helper Summary

**Canonical getBridge() accessor replacing all 85+ unsafe (window as any) bridge casts across 38 frontend files with type-safe imports and expanded AmuxBridge declarations**

## Performance

- **Duration:** 13 min
- **Started:** 2026-03-24T08:12:26Z
- **Completed:** 2026-03-24T08:25:26Z
- **Tasks:** 2
- **Files modified:** 38

## Accomplishments
- Created single canonical `getBridge(): AmuxBridge | null` in `frontend/src/lib/bridge.ts`
- Replaced 6 duplicate local `getBridge()` definitions with imports from shared module
- Eliminated all `(window as any).tamux` and `(window as any).amux` casts across 35 component and lib files
- Expanded `AmuxBridge` type in `amux-bridge.d.ts` with 30+ previously undeclared IPC methods
- TypeScript compilation passes with zero new errors (only pre-existing useSettingsStore errors remain)

## Task Commits

Each task was committed atomically:

1. **Task 1: Create shared bridge.ts and replace all duplicate getBridge() definitions** - `328b397` (feat)
2. **Task 2: Replace all remaining (window as any) unsafe casts with getBridge() imports** - `93865c6` (feat)

## Files Created/Modified
- `frontend/src/lib/bridge.ts` - Canonical getBridge() accessor, single source of truth
- `frontend/src/types/amux-bridge.d.ts` - Expanded with 30+ missing method declarations
- `frontend/src/lib/agentStore.ts` - Replaced local getBridge + 2 inline casts
- `frontend/src/lib/persistence.ts` - Updated from undefined to null bridge return
- `frontend/src/lib/agentTools.ts` - Replaced 7 inline casts, fixed type narrowing
- `frontend/src/components/TerminalPane.tsx` - Replaced 14 casts, fixed closure narrowing
- `frontend/src/App.tsx` - Replaced 8 inline casts
- `frontend/src/components/settings-panel/AgentTab.tsx` - Replaced 5 casts
- Plus 30 additional files with 1-3 replacements each

## Decisions Made
- AmuxBridge type was expanded with 30+ method declarations rather than using `[key: string]: any` index signature, preserving type safety for known methods while surfacing previously hidden type errors
- `getAgentBridge()` in `agentDaemonConfig.ts` was preserved as a public API that delegates to `getBridge()` since `runtime.tsx` and `SettingsPanel.tsx` import it
- `persistence.ts` changed from `AmuxBridge | undefined` to `AmuxBridge | null` -- no call site changes needed because `null?.method?.()` returns `undefined` just like `undefined?.method?.()`

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 2 - Missing Critical] Added 30+ missing AmuxBridge method declarations**
- **Found during:** Task 2 (replacing unsafe casts)
- **Issue:** Many IPC methods (readClipboardText, agentListTasks, sendDiscordMessage, whatsappSend, getSystemMonitorSnapshot, windowMinimize/Maximize/Close, onAgentEvent, etc.) were called via `(window as any)` but never declared in the AmuxBridge type
- **Fix:** Added all missing method declarations to `amux-bridge.d.ts` with correct signatures derived from usage patterns
- **Files modified:** `frontend/src/types/amux-bridge.d.ts`
- **Verification:** `npx tsc --noEmit` produces zero new errors
- **Committed in:** 93865c6 (Task 2 commit)

**2. [Rule 1 - Bug] Fixed type narrowing in closures and callbacks**
- **Found during:** Task 2 (replacing unsafe casts)
- **Issue:** Several files had TypeScript narrowing issues where bridge methods checked in outer scope weren't narrowed in inner closures/callbacks (InlineSystemMonitor, TerminalPane, TitleBar)
- **Fix:** Captured method references before closures (`const resizeFn = getBridge()?.resizeTerminalSession`), used optional chaining in JSX callbacks, added type casts where appropriate
- **Files modified:** `InlineSystemMonitor.tsx`, `TerminalPane.tsx`, `TitleBar.tsx`, `StatusBar.tsx`, `SettingsPanel.tsx`, `BehaviorTab.tsx`, `shared.tsx`, `TasksView.tsx`, `agentTools.ts`
- **Verification:** `npx tsc --noEmit` clean
- **Committed in:** 93865c6 (Task 2 commit)

**3. [Rule 1 - Bug] Fixed executeManagedCommand type signature**
- **Found during:** Task 2
- **Issue:** `executeManagedCommand` declared as `(paneId: string, ...) => Promise<boolean>` but called with `null` paneId and expected `{output: string}` result
- **Fix:** Updated signature to `(paneId: string | null, ...) => Promise<boolean | {output?: string}>`, added type guard at call site
- **Files modified:** `amux-bridge.d.ts`, `agentTools.ts`
- **Committed in:** 93865c6

---

**Total deviations:** 3 auto-fixed (2 bugs, 1 missing critical)
**Impact on plan:** All auto-fixes necessary for correctness. The plan's scope was accurate but the type declarations needed to be expanded when `any` was replaced with `AmuxBridge | null`. No scope creep.

## Issues Encountered
- Pre-existing TypeScript errors (`useSettingsStore` in `agentTools.ts` lines 1968-1979) exist on the base branch and are not caused by this plan. Build script `npm run build` fails due to `tsc -b` treating these as errors. This is a known pre-existing issue.

## Known Stubs
None - all bridge methods are wired to existing Electron IPC implementations.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- All 38 frontend files now use type-safe bridge access via `getBridge()` imports
- AmuxBridge type declarations are comprehensive, enabling future type checking
- Pattern established: any new bridge method must be declared in `amux-bridge.d.ts`

---
*Phase: 10-progressive-ux*
*Completed: 2026-03-24*
