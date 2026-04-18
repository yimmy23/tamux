# Honcho Chat Defaults And TUI Editor Design

## Goal

Align daemon, TUI, and React chat-related defaults so every boolean shown in the TUI Chat settings tab defaults to `true` except `enable_honcho_memory`, and change the TUI Chat tab so Honcho configuration is edited through an inline editor state instead of flat inline fields.

## Requested Behavior

### Default values

The following settings should default to `true`:

- `enable_streaming`
- `enable_conversation_memory`
- `anticipatory.enabled`
- `anticipatory.morning_brief`
- `anticipatory.predictive_hydration`
- `anticipatory.stuck_detection`
- `operator_model.enabled`
- `operator_model.allow_message_statistics`
- `operator_model.allow_approval_learning`
- `operator_model.allow_attention_tracking`
- `operator_model.allow_implicit_feedback`
- `collaboration.enabled`
- `compliance.sign_all_events`
- `tool_synthesis.enabled`
- `tool_synthesis.require_activation`

The following setting should default to `false`:

- `enable_honcho_memory`

Honcho text defaults remain unchanged:

- `honcho_api_key = ""`
- `honcho_base_url = ""`
- `honcho_workspace_id = "tamux"`

### TUI interaction

The Chat settings tab should keep a single `Honcho Memory` row in the main list.

- `Space` on that row toggles `enable_honcho_memory`
- `Enter` on that row opens an inline Honcho editor state
- the Honcho editor allows:
  - enable/disable Honcho memory
  - edit API key
  - edit base URL
  - edit workspace ID
  - save or cancel

This editor should behave like the existing subagent editor pattern:

- separate focused editor state
- dedicated render path while active
- dedicated keyboard handling while active
- save writes back to the shared config and syncs to the daemon
- cancel discards staged Honcho edits

## Affected Areas

### Daemon

Persisted defaults are defined in:

- `crates/amux-daemon/src/agent/types/runtime_config.rs`
- `crates/amux-daemon/src/agent/types/config_core.rs`

Changes:

- update `AgentConfig::default()` for top-level Honcho and chat booleans
- update nested `Default` implementations for:
  - `AnticipatoryConfig`
  - `OperatorModelConfig`
  - `CollaborationConfig`
  - `ComplianceConfig`
  - `ToolSynthesisConfig`

### TUI

Local startup defaults and config fallbacks are defined in:

- `crates/amux-tui/src/state/config.rs`
- `crates/amux-tui/src/app/config_io.rs`
- `crates/amux-tui/src/app/config_io_helpers.rs`

Settings navigation, rendering, and editing are defined in:

- `crates/amux-tui/src/state/settings.rs`
- `crates/amux-tui/src/widgets/settings/part5.rs`
- `crates/amux-tui/src/app/settings_handlers/impl_part1.rs`
- `crates/amux-tui/src/app/settings_handlers/impl_part2.rs`
- `crates/amux-tui/src/app/settings_handlers/impl_part3.rs`
- `crates/amux-tui/src/app/settings_handlers/impl_part4.rs`
- `crates/amux-tui/src/app/modal_handlers.rs`
- related tests under `crates/amux-tui/src/state/tests/`, `crates/amux-tui/src/widgets/settings/tests/`, and `crates/amux-tui/src/app/tests/`

Changes:

- remove Honcho text fields from the flat Chat-tab field list
- add a dedicated Honcho editor state owned by the TUI model/state layer
- keep the main Chat list cursor stable for top-level chat settings
- open the Honcho editor from the `Honcho Memory` row on `Enter`
- allow `Space` on the main row to keep toggling the boolean directly
- render the Honcho editor inline while active
- save/cancel back into normal Chat settings flow

### React

Frontend defaults are defined in:

- `frontend/src/lib/agentStore/settings.ts`

Changes:

- align `DEFAULT_AGENT_SETTINGS` with the daemon defaults listed above

## Architecture

### Why a dedicated Honcho editor

The current Chat tab is implemented as a fixed flat field index list. Honcho currently consumes one toggle plus three inline text fields. That makes the tab longer and mixes Honcho setup mechanics with unrelated chat capability toggles.

Creating a dedicated Honcho editor is the smallest clean change because it:

- matches the interaction model already used for subagents
- avoids adding more special cases to the Chat field index map
- keeps the Chat tab focused on top-level feature controls
- isolates Honcho-specific keyboard and rendering behavior

### Proposed TUI state shape

Add a Honcho editor state with staged values:

- `enabled: bool`
- `api_key: String`
- `base_url: String`
- `workspace_id: String`
- focused editor field enum:
  - `Enabled`
  - `ApiKey`
  - `BaseUrl`
  - `WorkspaceId`
  - `Save`
  - `Cancel`

The editor is opened by copying the current config values into staged state. Save copies staged values back into `ConfigState` and then calls the existing config-sync path. Cancel drops the staged editor state without mutating config.

## Navigation And Rendering

### Main Chat tab

The Chat tab remains a flat list of top-level items. Honcho contributes one row:

- `Honcho Memory`

The main Chat field count decreases because `honcho_api_key`, `honcho_base_url`, and `honcho_workspace_id` are no longer direct Chat fields.

### Honcho editor

While the Honcho editor is active:

- normal Chat cursor navigation is suspended
- arrow keys and tab move within Honcho editor fields
- `Enter` edits text fields or triggers save/cancel depending on the focused field
- `Esc` closes the editor without saving

Rendering should visually mirror the subagent editor style closely enough that it reads as the same class of interaction.

## Data Flow

### Startup and partial-config consistency

Defaults must align across:

- daemon persisted defaults
- TUI local initial defaults
- TUI deserialization fallback values for missing fields
- React local initial defaults

This avoids mismatches where a fresh config, a partial config, or a frontend-only initial render shows different values for the same setting.

### Sync semantics

No wire-format changes are required.

- the TUI still writes the same config keys and nested objects
- Honcho fields still persist as top-level config keys
- the editor changes only the TUI interaction model, not the stored config schema

## Testing

Add or update tests for:

### Daemon

- default `AgentConfig` values reflect the requested booleans
- nested config defaults reflect the requested booleans

### TUI state

- Chat tab field name mapping after removing inline Honcho text fields
- Chat tab field count after the field list changes

### TUI widgets

- main Chat tab renders a single `Honcho Memory` row and no inline Honcho text fields
- active Honcho editor renders its staged fields and actions

### TUI behavior

- `Enter` on `Honcho Memory` opens the editor
- `Space` on `Honcho Memory` still toggles the top-level boolean
- Honcho editor navigation wraps correctly
- saving staged Honcho values updates config and triggers sync
- cancel leaves config unchanged

### React

- frontend default settings reflect the new boolean defaults if covered by existing tests

## Non-Goals

- no schema redesign for Honcho settings
- no daemon-side Honcho behavior changes
- no broader settings-tab refactor beyond the Honcho editor extraction
- no attempt in this change to deduplicate all defaults into a single shared source
