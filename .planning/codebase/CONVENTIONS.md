# Coding Conventions

**Analysis Date:** 2026-03-22

---

## Rust Conventions

### Naming Patterns

**Files/modules:**
- `snake_case` throughout — e.g., `circuit_breaker.rs`, `tool_filter.rs`, `session_recall.rs`
- Submodule directories use the same convention: `crates/amux-daemon/src/agent/metacognitive/`, `crates/amux-daemon/src/agent/subagent/`

**Types (structs, enums, traits):**
- `PascalCase` — e.g., `CircuitBreaker`, `CircuitState`, `TokenBucket`, `MemoryTarget`, `AgentThread`

**Functions and methods:**
- `snake_case` — e.g., `record_failure`, `try_acquire`, `build_causal_guidance_summary`
- Boolean query methods use the predicate pattern: `is_allowed`, `has_restrictions`, `can_execute`
- Builder helpers in tests use `make_` prefix: `make_tool`, `make_item`, `make_msg`, `make_input`
- Factory helpers use `sample_` prefix: `sample_goal_run`, `sample_task`, `sample_provider_config`

**Constants:**
- `UPPER_SNAKE_CASE` — e.g., `SOUL_LIMIT_CHARS`, `ONECONTEXT_TOOL_OUTPUT_MAX_CHARS`, `APPROX_CHARS_PER_TOKEN`
- Constants with numeric suffixes: `CONCIERGE_THREAD_ID`, `MIN_CONTEXT_TARGET_TOKENS`

**Visibility:**
- Internal module helpers: `pub(super)` — e.g., in `crates/amux-daemon/src/agent/memory.rs`
- Cross-crate internals: `pub(crate)`
- Public API: `pub`
- Default (private): no modifier; all internal implementation details

### Module Documentation

Every public module starts with a `//!` doc comment on line 1, short and descriptive:
```rust
//! Circuit breaker — protect against cascading failures from LLM API outages.
//!
//! Not yet wired into the LLM call path — infrastructure ready for integration.
```
Examples: `crates/amux-daemon/src/agent/circuit_breaker.rs`, `crates/amux-daemon/src/agent/rate_limiter.rs`, `crates/amux-daemon/src/agent/compaction.rs`

Public structs, enums, and impl methods use `///` doc comments:
```rust
/// Create a new circuit breaker with the given thresholds.
///
/// * `failure_threshold` — consecutive failures before tripping to Open.
pub fn new(failure_threshold: u32, ...) -> Self {
```

Internal helpers use `// ---- section label ----` banners to visually group functions.

### Type Definitions

**Structs:**
- Use `#[derive(Debug, Clone)]` as the minimum for domain types
- Types meant to be copied cheaply add `Copy, PartialEq, Eq`: `#[derive(Debug, Clone, Copy, PartialEq, Eq)]`
- Wire types (protocol messages) use: `#[derive(Debug, Clone, Serialize, Deserialize)]`
- Serde enums always carry `#[serde(rename_all = "snake_case")]` unless field-level overrides are needed
- Optional wire fields use: `#[serde(default, skip_serializing_if = "Option::is_none")]`

Examples from `crates/amux-daemon/src/agent/types.rs` and `crates/amux-protocol/src/messages.rs`.

### Error Handling

**Production code uses `anyhow::Result<T>` everywhere:**
- `anyhow` is the primary error crate across all Rust crates (declared in `Cargo.toml`)
- `thiserror` is in workspace deps but reserved for typed errors; most errors use `anyhow::anyhow!(...)`

**Pattern for returning errors:**
```rust
use anyhow::{Context, Result, bail};

// Contextual errors:
thing.operation().context("failed to do thing")?;

// Early exits:
bail!("daemon closed connection while spawning session");

// Error construction:
Err(anyhow::anyhow!("invalid memory target `{other}`; expected soul, memory, or user"))
```

**`unwrap()` and `expect()` are only acceptable inside `#[cfg(test)]` blocks.** Production code propagates errors via `?`. Examples in `crates/amux-daemon/src/agent/subagent/lifecycle.rs` (test block only).

**`tracing` crate is used for all structured logging** (not `println!`/`eprintln!`):
```rust
tracing::info!(session_id = %id, "daemon session spawned for gateway");
tracing::warn!(message, "managed command rejected");
tracing::error!(error = %e, "failed to send command to daemon");
tracing::debug!(?other, "unhandled daemon message");
```
Named fields use `%` for Display formatting and `?` for Debug formatting.

### Import Organization

**Standard structure within a file:**
1. External crates (`use anyhow`, `use serde`, `use tokio`, etc.)
2. Internal crate imports (`use crate::...`)
3. Sibling imports (`use super::...`)

**Within the agent module**, submodules use `use super::*` to access the entire parent namespace (because `mod.rs` re-exports everything with `use anticipatory::*; use behavioral_events::*;` etc.):
```rust
// In any agent submodule file:
use super::*;
```
This is intentional — `crates/amux-daemon/src/agent/mod.rs` explicitly re-exports everything via glob use statements.

### Code Organization

**Module splitting strategy:** Large domains get broken into small single-responsibility files. E.g., the agent module at `crates/amux-daemon/src/agent/` has 30+ submodules each under ~1500 lines, though `tool_executor.rs` is notably large at 5032 lines.

**Sections within long files** use divider comments:
```rust
// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
```

### Default Implementations

`Default` is consistently implemented for configuration and state structs. Production initialization uses `Default::default()` or custom `new()` constructors with explicit parameters.

---

## TypeScript / React Conventions

### Naming Patterns

**Files:**
- React components: `PascalCase.tsx` — e.g., `ChatView.tsx`, `AgentTab.tsx`, `TitleBar.tsx`
- Stores and lib utilities: `camelCase.ts` — e.g., `agentStore.ts`, `workspaceStore.ts`, `agentClient.ts`
- Component subdirectories: `kebab-case/` — e.g., `agent-chat-panel/`, `settings-panel/`, `base-components/`

**React components:**
- Named exports only (no default exports from component files, except the root `App`)
- `function ComponentName(...)` syntax (not arrow functions for top-level components)
- Props passed as inline destructured object parameter with explicit TypeScript type annotation

**Interfaces and types:**
- `PascalCase` for all type names: `AgentThread`, `ChatChunk`, `ProviderDefinition`
- `type` for unions and aliases, `interface` for object shapes — both are used interchangeably
- Provider IDs are string union types: `export type AgentProviderId = "featherless" | "openai" | ...`

**Variables and functions:**
- `camelCase` — e.g., `createWorkspace`, `toggleSidebar`, `normalizeAgentProviderId`
- Constants: `UPPER_SNAKE_CASE` — e.g., `AGENT_PROVIDER_IDS`, `PROVIDER_DEFINITIONS`, `APPROX_CHARS_PER_TOKEN`
- Counter helpers: prefixed with `_` when private module-level: `let _wsId = 0`

**Zustand stores:**
- Store hook: `use<Name>Store` — e.g., `useAgentStore`, `useWorkspaceStore`, `useSettingsStore`
- Selector pattern: `const thing = useStore((s) => s.thing)` — granular subscriptions, not full store access
- Actions defined inline within `create(...)` call

### TypeScript Settings

`frontend/tsconfig.json` enforces strict mode:
- `"strict": true`
- `"noUnusedLocals": true`
- `"noUnusedParameters": true`
- `"noFallthroughCasesInSwitch": true`
- `"forceConsistentCasingInFileNames": true`

Path alias `@/*` maps to `src/*` (defined in both `tsconfig.json` and `vite.config.ts`).

### Import Organization

```typescript
// 1. React and framework imports
import { useMemo, useState } from "react";
import type React from "react";

// 2. Third-party libraries
import ReactMarkdown from "react-markdown";
import type { Components } from "react-markdown";

// 3. Internal lib imports (relative)
import type { AgentMessage, AgentThread } from "../../lib/agentStore";
import { inputStyle } from "./shared";
```

`import type` is used consistently for type-only imports.

### Component Design

**Props interface:** Defined inline as an object type in the function signature:
```typescript
export function ChatView({
    messages,
    todos,
    ...
}: {
    messages: AgentMessage[];
    todos: AgentTodoItem[];
    ...
}) {
```

**Hooks:** Only from `react` or local `hooks/` directory. Custom hooks follow `use<Name>` naming: `useHotkeys`.

**Lazy loading:** Heavy panels and overlays are lazy-loaded via `React.lazy` + named module re-export:
```typescript
const SettingsPanel = lazy(() => import("./components/SettingsPanel").then((m) => ({ default: m.SettingsPanel })));
```

### Error Handling

**Frontend uses `console.warn/error` for non-critical errors** — no unified error boundary framework detected. `void` is used to discard floating promises explicitly:
```typescript
void amux.openAICodexAuthStatus({ refresh: true }).then(...).catch(...);
```

**`try/catch`** wraps async electron bridge calls and plugin loading.

### Logging

`console.log/warn/error` is used directly. Prefixes in brackets identify the subsystem:
```typescript
console.log("[concierge] setting up agent event listener in App.tsx");
console.warn("[concierge] no onAgentEvent bridge available");
```

### Comments

**Block comments** above sections of related logic use `// ---------------------------------------------------------------------------`.

**Inline comments** explain non-obvious logic with `//`.

No JSDoc usage detected — TypeScript types serve as the primary documentation.

---

## Shared Across Both Languages

**Separator comments** group sections within large files: `// ---- label ----` (Rust) and `// ----` (TypeScript).

**Naming guards:** Sensitive config fields are protected by pattern-matching (`is_sensitive_config_key` in `crates/amux-daemon/src/agent/config.rs`).

**Numeric constants** use `_` for readability: `1_000_000`, `30_000`, `128_000`.

---

*Convention analysis: 2026-03-22*
