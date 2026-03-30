# Provider Outage Recovery And GitHub Copilot Design

## Summary

tamux should handle provider outages as a first-class runtime condition instead of surfacing misleading circuit-breaker errors. When a provider trips open, the daemon must tell clients which provider/model failed, why it is unavailable, and which alternatives are actually switchable right now. TUI and React should both render this as:

- a persistent degraded-provider banner/status surface while the outage remains active
- inline failure actions on the specific chat or heartbeat failure
- one-click provider+model switching using the existing config update path

This work also adds GitHub Copilot as a first-class provider with both interactive browser/device-flow login and token/env-based auth, with model availability gated by actual Copilot entitlement state.

## Goals

- Never suggest a provider that is not actually configured and switchable.
- Make provider outages visible as state, not just transient error text.
- Let the operator switch provider+model in one action from both TUI and React.
- Add GitHub Copilot as a supported provider with realistic auth and model handling.
- Keep the daemon as the single source of truth for provider health and switch targets.

## Non-Goals

- Automatic provider failover without operator approval.
- Reworking the full provider settings UX beyond what is needed for outage recovery.
- Multi-provider load balancing or traffic shaping.
- Deep Copilot feature parity beyond auth, validation, and model selection.

## Current Problems

### Misleading fallback suggestions

The daemon currently suggests “healthy” alternatives by scanning configured provider entries and checking only whether their circuit breaker is closed. Placeholder provider rows in the persisted config therefore appear eligible even when they have empty credentials, empty model, or empty base URL. This produced a false suggestion to switch to Hugging Face even though it was not logged in.

### Outage state is not actionable

Provider degradation is visible only as:

- circuit-breaker-related error text in a failed turn
- a degraded-provider count in the React status bar

There is no durable, operator-facing outage surface with direct switch actions.

### Provider switching is fragmented

Both TUI and React already support provider/model changes in settings, but neither exposes outage-aware one-click switching from the error path.

### GitHub Copilot is unsupported

There is no first-class GitHub Copilot provider despite demand for it as an outage fallback.

## Constraints And External Facts

GitHub Copilot integration should reflect current official constraints rather than assumptions:

- GitHub documents browser/device-flow authentication for Copilot CLI.
- GitHub documents that available Copilot models depend on the user’s plan and entitlements.
- GitHub Copilot plans differ in premium request allowance and model access.

Official sources:

- GitHub Copilot CLI authentication:
  https://docs.github.com/en/enterprise-cloud%40latest/copilot/how-tos/copilot-cli/set-up-copilot-cli/authenticate-copilot-cli
- Supported AI models in Copilot:
  https://docs.github.com/en/copilot/using-github-copilot/ai-models/supported-ai-models-in-copilot
- Copilot plans:
  https://docs.github.com/en/copilot/get-started/plans

Inference from these sources:

- model availability cannot be treated as static for all users
- daemon-side validation must gate Copilot switchability
- UI must not guess Copilot entitlement locally

## Recommended Architecture

The daemon remains the single source of truth for:

- provider health
- outage diagnostics
- alternative provider eligibility
- provider switch recommendations
- GitHub Copilot auth validity and model availability

TUI and React become consumers of a shared outage contract and reuse existing config update mechanisms for actual switching.

## Daemon Design

### 1. Provider eligibility helper

Add a daemon helper that answers: “is this provider a valid switch target right now?”

Eligibility rules:

- provider is not the currently failed provider
- provider circuit breaker can execute
- provider resolves to a non-empty effective model
- provider resolves to a non-empty effective base URL
- provider has required credentials for its auth mode
- provider-specific auth validation passes where needed

Credential/auth rules by class:

- API-key providers: required secret present and non-empty
- subscription/device-flow providers: persisted auth state valid and unexpired
- custom provider: base URL plus model plus whatever auth mode requires
- GitHub Copilot: either device-flow auth valid, or token/env auth valid, and at least one usable model available

Placeholder provider rows must never pass eligibility.

### 2. Structured outage event

Replace or extend the current `ProviderCircuitOpen` event so clients receive actionable data, not just `{ provider, trip_count }`.

Required fields:

- `provider`
- `model`
- `trip_count`
- `reason`
- `kind` or `failure_class` if available
- `alternatives`

Each alternative should include:

- `provider`
- `provider_display_name`
- `model`
- `model_display_name` when available
- `auth_mode`
- `switchable: true`

If there are no alternatives, the event should still say so explicitly.

### 3. Persistent provider health snapshot

The daemon status snapshot already exposes provider health. Extend it so clients can build a persistent banner without parsing chat text:

- active degraded providers
- latest outage reason per provider
- recommended alternatives per degraded provider
- whether each alternative is switchable vs auth-required

This allows banner rendering even if the original failure message is no longer visible.

### 4. Provider switch action

Switch actions should remain explicit operator actions. The daemon should support an atomic “switch active provider + model” flow using the same config persistence path already used by settings.

Switch semantics:

- set provider
- set model
- update effective base URL from canonical provider definition unless provider is `custom`
- persist config
- notify clients of config refresh

If validation fails at click time, return a structured error and keep the old config unchanged.

### 5. Honest alternative suggestion text

The human-readable suggestion string must be generated only from eligible alternatives. If none exist, the daemon should say so directly, for example:

“Provider `minimax-coding-plan` is temporarily unavailable. No authenticated fallback providers are currently available.”

### 6. GitHub Copilot provider

Add `github-copilot` as a provider definition with dedicated auth and validation behavior.

Supported auth paths in v1:

- browser/device-flow login
- token/env-based auth

Required daemon responsibilities:

- persist Copilot device-flow auth state separately from generic API-key storage
- validate Copilot auth state
- fetch or refresh available Copilot models from a Copilot-aware validation path when possible
- expose Copilot auth state through existing provider-auth UI surfaces

Copilot must not be advertised as switchable unless:

- auth is valid
- at least one model is available
- provider health is healthy enough to execute

## React Design

### Persistent banner

Add a provider-outage banner that appears while a degraded provider remains open.

Banner content:

- failed provider/model
- concise outage reason
- switch buttons for daemon-approved alternatives
- fallback action to open provider auth/settings

Behavior:

- banner is persistent while the outage remains active
- dismiss only hides the current instance locally; it does not clear outage state
- if the outage snapshot updates, the banner refreshes

### Status bar integration

Keep the degraded-provider status indicator, but make it open or focus the outage banner/panel instead of being informational only.

### Inline failure actions

Heartbeat failure cards and chat/system error surfaces should reuse the same daemon-supplied alternatives and action wiring so the operator can switch at the point of failure.

### Copilot auth

Extend existing provider-auth UI:

- show GitHub Copilot provider row
- support browser/device-flow login initiation
- support token/env auth path
- show validated available models only after successful auth validation

## TUI Design

### Persistent outage surface

Add a durable degraded-provider banner or panel to TUI rather than relying on transient status-line messages.

Minimum required content:

- failed provider/model
- concise reason
- top recommended alternative action
- action to open full provider/model picker if more alternatives exist

### Direct switch actions

Reuse the existing provider/model config path. The outage surface should trigger an atomic provider+model switch, not force the operator to manually navigate settings unless there is no switchable target.

### Copilot auth

Expose GitHub Copilot in the provider/auth flows alongside existing providers:

- browser/device-flow login initiation
- token/env auth path
- validated model selection once authenticated

## Data Flow

### Outage path

1. LLM provider call fails repeatedly and circuit breaker opens.
2. Daemon computes eligible alternatives.
3. Daemon emits structured outage event and updates provider-health snapshot.
4. TUI and React show persistent degraded-provider UI.
5. Operator clicks alternative provider/model.
6. Existing config update path persists the new provider and model.
7. Next turns use the new provider.

### Copilot path

1. Operator opens provider auth.
2. Device-flow or token/env auth is configured.
3. Daemon validates auth and resolves available models.
4. UI shows Copilot as authenticated only when validation succeeds.
5. Copilot becomes eligible for manual selection and outage fallback suggestions.

## Error Handling

If no alternatives are eligible:

- show outage banner without switch buttons
- offer settings/auth action only

If a switch action fails:

- do not mutate current provider/model
- surface exact daemon validation error

If Copilot auth is present but entitlement/model fetch fails:

- show Copilot as unavailable for switching
- route user to auth/provider details instead of exposing a broken switch button

## Migration

### Event compatibility

Because clients already consume `ProviderCircuitOpen`, this change should either:

- extend that event in a backward-compatible way, or
- add a new richer outage event while preserving old behavior during rollout

Recommended: extend if wire compatibility allows; otherwise introduce a new event and update both clients in the same change.

### Persisted placeholder providers

Existing placeholder rows in `agent_config_items` remain allowed, but the eligibility helper must exclude them from switch suggestions.

### Copilot persistence

Copilot auth state should not be stored as a fake API key if device-flow auth is used. It should have a dedicated persistence record similar to other subscription/browser-auth flows.

## Testing

### Daemon

- alternative suggestion excludes providers with empty API key/model/base URL
- alternative suggestion excludes providers whose breaker is open
- alternative suggestion includes properly configured providers
- outage event includes failed provider/model and alternatives
- atomic provider+model switch persists both fields together
- Copilot is eligible only when auth and model validation pass

### React

- degraded-provider banner renders from provider-health/outage state
- switch button updates active provider/model through existing config path
- no-switchable-alternative state routes to settings/auth
- Copilot auth row supports login state transitions

### TUI

- outage event projects to a durable UI surface
- switch action applies provider+model atomically
- no-switchable-alternative state does not offer bogus provider buttons

### Manual

- trip MiniMax circuit breaker while Alibaba/OpenRouter remain configured
- verify only authenticated alternatives appear
- verify placeholder Hugging Face does not appear
- switch from outage banner in React
- switch from outage surface in TUI
- authenticate GitHub Copilot via browser/device flow
- configure GitHub Copilot via token/env path
- confirm Copilot appears only when validated

## Recommended Delivery Order

1. Fix daemon alternative eligibility and structured outage payload
2. Add persistent outage banner/action surfaces in React and TUI
3. Reuse existing config flow for atomic provider+model switch buttons
4. Add GitHub Copilot provider auth, validation, and model handling
5. Wire Copilot into outage alternatives

## Open Decisions Resolved

- Outage actions should appear both inline and as a persistent banner/status surface.
- GitHub Copilot should support both browser/device-flow auth and token/env-based auth in v1.
