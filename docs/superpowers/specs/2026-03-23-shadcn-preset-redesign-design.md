# Shadcn Preset Redesign Design Spec

**Date:** 2026-03-23
**Status:** Approved
**Author:** Human + Copilot

## Problem

The current `frontend/` application uses a custom Electron + React + Zustand shell with a large CSS-variable design system and no React Router. The user wants the frontend to adopt the design direction represented by:

`pnpm dlx shadcn@latest init --preset b3QvsBDUY --base base --template react-router`

The design needs to replace the current visual system without breaking the existing workspace/surface runtime, Electron behavior, or the CDUI/YAML-driven rendering path.

## Solution

Use a **hybrid shell refactor**:

- keep the existing runtime/state model and Electron bridge behavior,
- introduce a new shared design layer built around shadcn-style primitives and Tailwind-backed theming,
- refactor the existing shared UI wrappers so both `App` and `CDUIApp` render through the same new visual system,
- migrate in phases rather than attempting a big-bang rewrite.

The preset is treated as a **design reference and starting point**, not as a literal scaffold to be dropped into the repo unchanged.

## Requirements

- Replace the current frontend design with the target preset's visual direction.
- Use a **phased migration**, not a big-bang rewrite.
- Maintain **CDUI visual parity in phase 1**, not as a later follow-up.
- Preserve the current core runtime model:
  - no regression in workspace/surface/pane behavior,
  - no regression in Zustand store contracts,
  - no regression in Electron-integrated flows.
- Structural cleanup is allowed when it produces a meaningfully cleaner end state, but the redesign should not force an unnecessary runtime rewrite.
- The result must be implementation-plan-ready for the existing repo conventions and tooling.

## Current Constraints

- `frontend/src/main.tsx` mounts either `App` or `CDUIApp`; both paths must remain supported.
- `frontend/src/App.tsx` is a state-driven shell, not a route-driven app.
- `frontend/src/styles/global.css` currently contains the bulk of the visual system and tokens.
- `frontend/src/components/BaseComponents.tsx` is the main shared primitive layer for builder/CDUI integration.
- There is currently no Tailwind, shadcn config, Radix dependency, or React Router dependency in `frontend/package.json`.

## Alternatives Considered

### 1. Full preset-led rewrite

Introduce React Router and reshape the frontend around the preset's template assumptions.

**Why not chosen:** This would create the cleanest visual reset, but it fights the current workspace/surface runtime, increases migration risk, and makes phase-1 CDUI parity harder.

### 2. Skin-only adaptation

Keep the current structure and mostly remap the new look onto the existing CSS/component stack.

**Why not chosen:** Lowest risk, but it would preserve too much of the old visual implementation and likely under-deliver on the requested redesign.

### 3. Hybrid shell refactor

Keep the runtime model but replace the design layer and shared primitives.

**Chosen because:** It balances fidelity to the new design with architecture safety, supports phased delivery, and gives CDUI a first-class path instead of a retrofit.

---

## Architecture

### Runtime boundary

The redesign keeps the existing behavior layer intact:

- Electron main/preload integrations remain unchanged
- Zustand stores remain the source of truth
- workspace/surface/pane navigation remains store-driven
- daemon event wiring and panel behavior remain intact

### New UI boundary

The redesign introduces a new shared UI layer with three responsibilities:

1. **Theme + token layer**
   - Defines the visual system that expresses the preset's direction
   - Acts as the single source of truth for colors, spacing, radii, borders, shadows, and typography

2. **Primitive layer**
   - New shadcn-style components under `frontend/src/components/ui/`
   - Covers buttons, inputs, dialogs, tabs, sheets, cards, menus, badges, and other reusable building blocks

3. **Adapter layer**
   - Refactors the existing shared wrappers used by `App` and CDUI
   - Preserves `ViewProps`, builder metadata, and registry contracts while rendering the new primitives underneath

### Key architectural rule

The redesign may restructure the **UI layer**, but it should not rewrite the **runtime layer** unless a later implementation-planning pass proves that a specific structural change is necessary and low-risk.

React Router is therefore **not** a required part of the chosen design. The preset's router template should inform layout and composition decisions, but not dictate the application's navigation model.

---

## Component Strategy

### Shared primitives

Create a new primitive system under `frontend/src/components/ui/` for the shadcn-style component set.

These primitives become the canonical building blocks for:

- the standard app shell
- panel/overlay surfaces
- CDUI-rendered views

### Adapter strategy

The existing `frontend/src/components/BaseComponents.tsx` should not remain a giant raw-HTML primitive layer. Instead, it should be split into focused modules or refactored behind adapters that:

- preserve the current `ViewProps` contract,
- keep builder metadata and edit-mode support intact,
- delegate rendering to the new primitive layer.

This is the key mechanism that lets CDUI and the standard app share one design language.

### Shell components

The following shell-facing components are expected to migrate onto the new primitives and layout patterns:

- `LayoutContainer`
- `TitleBar`
- `Sidebar`
- `SurfaceTabBar`
- `StatusBar`
- overlay surfaces such as settings, command palette, notifications, dialogs, and shared panels

---

## Styling and Token Strategy

### Canonical token source

Use **theme variables as the canonical token contract**, with Tailwind/shadcn consuming those variables rather than inventing a parallel theme source.

This preserves compatibility with:

- existing runtime theme application logic,
- Electron-safe styling behavior,
- CDUI wrappers that still need access to semantic tokens,
- gradual migration from the current `global.css`.

### Tailwind role

Tailwind is introduced as a composition and utility layer for the new primitives and layouts, not as a second competing design system.

### `global.css` role after migration begins

`frontend/src/styles/global.css` should gradually shrink to:

- token definitions,
- base element resets,
- temporary compatibility styles during migration.

It should no longer remain the long-term home of every component's final styling.

### Styling rule

The redesign should not leave the repo with two permanent visual systems. Legacy styles may coexist temporarily during migration, but the plan should drive toward one final shared system.

---

## CDUI Compatibility Contract

CDUI support is **in scope from phase 1**.

That means the redesign must:

- keep `CDUIApp` functional,
- preserve the renderer/registry path for YAML views,
- provide CDUI-compatible wrappers around the new primitives early,
- ensure phase-1 migrated surfaces do not look materially behind the standard app shell.

The design does **not** require CDUI to gain a separate styling system. Instead, CDUI should inherit the same primitives, tokens, and wrappers as the main app.

---

## Migration Sequence

### Phase A — Foundation

- Introduce Tailwind/shadcn infrastructure into `frontend/`
- Create the new theme/token bridge
- Add the core primitive set under `components/ui/`
- Build the first adapter pass for shared wrappers used by both `App` and `CDUIApp`

### Phase B — Shared wrapper conversion

- Refactor or split `BaseComponents.tsx`
- Move builder/CDUI primitives onto the new design layer
- Prove that both standard UI and CDUI can render through the same component contract

### Phase C — Shell migration

- Migrate the app shell: layout container, title bar, sidebar, tab bar, status bar
- Align shell spacing, hierarchy, elevation, and interaction styling with the approved design direction

### Phase D — Panels and overlays

- Migrate shared high-traffic panels and overlays
- Replace dialog/sheet/modal styling through the new primitive layer
- Keep behavior stable while progressively removing legacy visual dependencies

### Phase E — Cleanup

- Remove obsolete legacy component styling
- Trim compatibility CSS that is no longer needed
- Leave one coherent design system, not a permanent hybrid

---

## Behavior and Data Flow

The redesign is intentionally **visual-first**, not behavior-first.

The following should stay stable unless a later implementation plan explicitly proves otherwise:

- workspace creation and switching
- surface management
- panel open/close state
- agent/event overlays
- Electron bridge communication
- daemon message handling

This keeps the redesign focused on replacing the UX layer without accidentally broadening scope into unrelated runtime changes.

---

## Error Handling and Safety Rules

- No silent fallback to broken or half-migrated UI states.
- If a CDUI wrapper or shared primitive is missing, that should surface clearly during development rather than fail invisibly.
- Migration steps should keep a bounded temporary compatibility layer, not indefinite parallel implementations.
- Structural changes are allowed only where they reduce long-term complexity or improve design fidelity without destabilizing behavior.

---

## Validation

The redesign must be validated with the tooling already present in the repo:

- `cd frontend && npm run lint`
- `cd frontend && npm run build`

Manual validation is also required because there is no committed frontend test suite:

- standard app mode boot
- CDUI mode boot
- shell navigation and resizing
- major overlays/dialogs
- high-traffic panels
- Electron window chrome/title bar behavior

---

## Out of Scope

- daemon architecture changes
- TUI redesign work
- route-first rearchitecture of the app as a prerequisite
- unrelated refactors outside the surfaces touched by the redesign

## Planning Handoff

The implementation plan should assume:

- the chosen approach is the hybrid shell refactor,
- CDUI parity is required in phase 1,
- the preset is being adapted to this repo, not copied literally,
- the runtime/state model remains stable while the UI layer is replaced.
