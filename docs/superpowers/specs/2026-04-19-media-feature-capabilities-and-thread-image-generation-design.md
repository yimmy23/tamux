# Media Feature Capabilities And Thread Image Generation Design

## Summary

Add a derived feature-capability layer on top of the existing model modality metadata, and use it to support:

- stable user-facing feature flags: `vision`, `stt`, `tts`, and `image_generation`
- provider-agnostic filtering and gating for media-capable models
- OpenRouter model-list query filtering where upstream supports it
- a new TUI `/image <prompt>` command that generates an image into the active thread, or creates a new main-agent thread first when no thread is open
- durable thread-scoped media persistence so generated images show up both as touched files and as inline renderable thread history entries

The design deliberately preserves the current modality fields and current STT/TTS behavior. Existing speech flows are treated as working contracts that must not regress.

## Problem

The repo already has most of the media primitives:

- fetched model metadata preserves `input_modalities`, `output_modalities`, `modality`, pricing, and raw upstream metadata
- TUI and frontend already derive directional audio support for STT and TTS from fetched model metadata
- the daemon already exposes `generate_image`, `speech_to_text`, and `text_to_speech`
- thread messages already support image and audio content blocks

The gaps are architectural rather than foundational:

- media capability decisions are still split across ad hoc modality checks, pricing hints, and provider-level assumptions
- image generation remains too coarse at the provider/tool layer
- OpenRouter model discovery does not yet use upstream modality filters when those can reduce noise
- user-forced image generation from TUI does not yet exist as a first-class thread-pinned action
- generated media currently lands in temp paths rather than thread-owned durable storage

Without a derived feature layer, adding image generation, keeping STT/TTS stable, and extending model filtering all risk duplicating capability logic across daemon, TUI, and frontend.

## Goals

- Keep the current raw modality metadata and pricing fields unchanged
- Add a second, smaller derived feature schema for user-facing media capabilities
- Use that schema consistently across daemon, TUI, and frontend
- Preserve current STT/TTS behavior and current audio settings UX
- Improve OpenRouter model discovery with optional upstream query filtering
- Add `/image <prompt>` in TUI
- Pin user-forced image generation to the active thread, or create a new main-agent thread when no thread is open
- Persist generated images into thread-scoped tamux storage
- Record generated images as thread-touched files/artifacts and also render them inline in thread history

## Non-Goals

- Replacing or renaming existing modality fields such as `input_modalities`, `output_modalities`, or `modality`
- Rewriting working STT/TTS execution flows
- Introducing a brand-new top-level media config tree
- Creating a cross-language generated capability registry in this task
- Building a generic media-job framework for every future media action
- Changing the current speech hotkeys or STT/TTS settings semantics unless required for compatibility preservation

## Recommended Approach

Use a derived feature-capability layer that sits above the existing modality metadata.

This is the smallest design that solves the product problem cleanly:

- raw upstream metadata remains intact
- current directional audio logic remains valid
- image generation becomes a first-class feature without inventing new modality semantics
- providers can opt into better fetch-time filtering without changing the app-wide contract

The source of truth for product-facing media behavior becomes the derived feature layer, not scattered direct modality checks.

## Feature Capability Contract

### Raw Data

The following existing fields remain untouched:

- `input_modalities`
- `output_modalities`
- `modality`
- `pricing`
- raw `metadata`

### New Derived Features

Add derived feature flags for:

- `vision`
- `stt`
- `tts`
- `image_generation`

These are computed from existing modality metadata, pricing hints, provider/model heuristics, and where necessary explicit compatibility rules for known providers.

### Derivation Rules

- `vision`
  - true when the model is known to accept image input for analysis/chat use
- `stt`
  - true when the model is known to accept audio input for transcription-oriented behavior
- `tts`
  - true when the model is known to emit audio output for speech synthesis
- `image_generation`
  - true when the model is known to support image generation through the provider’s image-generation path

The current STT/TTS directional rules stay in place. The new layer must be additive and derived from those rules, not a replacement for them.

## Capability Sources

### Built-In Model Catalogs

Built-in provider/model definitions should gain derived feature evaluation from the same helper layer used for fetched models.

Where the static catalog already encodes enough modality information, derived features should be computed from that data rather than duplicated manually.

### Fetched Remote Models

Fetched remote models already preserve upstream metadata and pricing. The feature layer should derive:

- `vision` from image input support
- `stt` from directional audio-input support
- `tts` from directional audio-output support
- `image_generation` from provider/model-specific generation support rules

This keeps current fetched-model behavior intact while giving the rest of the product a smaller, safer interface to consume.

## OpenRouter Fetch Integration

OpenRouter should remain supported through the normal fetched-model path, but model fetch may add provider-specific query filtering when useful.

### Default Fetch

General model fetch stays broad so the current model picker behavior does not regress.

### Targeted Fetch

When the caller needs image-generation candidates specifically, the OpenRouter fetch layer may add upstream query parameters such as `output_modalities=image` to reduce irrelevant results.

This optimization must be scoped narrowly:

- use it only for targeted media-capability pickers or lookups
- do not change the general-purpose fetch behavior unless the caller asked for a filtered media slice
- still run local derived-feature evaluation after fetch, even when upstream filtering is used

This preserves behavior if:

- OpenRouter changes response details
- the provider ignores a filter
- another provider later gains similar filtering support

## Architecture

### Shared Logic Shape

Do not mutate the raw model shape in incompatible ways.

Instead, add focused helpers that:

- inspect built-in or fetched model metadata
- derive the feature-capability view
- expose compact predicates used by daemon, TUI, and frontend

The implementation can be language-local per surface, but the rules must stay aligned.

### Daemon

The daemon should consume derived features for:

- `generate_image` availability and validation
- model/tool gating for image generation
- any thread-persistence path that needs to know whether a selected model is allowed to generate images

The daemon remains the owner of thread-scoped persistence and artifact recording.

### TUI

The TUI should consume derived features for:

- audio model picker filtering
- new image-generation model picker filtering if exposed
- `/image <prompt>` command routing
- status and error messages that explain capability mismatches

Existing STT/TTS settings, hotkeys, and modal flows must keep their current behavior unless a compatibility fix is strictly required.

### Frontend

The frontend can optionally use the same derived features for:

- consistent picker filtering
- feature badges or labels
- future image-generation picker affordances

This is additive. The immediate requirement is consistency, not a new frontend workflow.

## TUI `/image` Command Design

### Command Semantics

Add `/image <prompt>` as a user-forced media action.

Behavior:

- if a thread is open, generate into the current thread
- if no thread is open, create a new main-agent thread first, then generate into that thread

This is not normal chat text submission. It is a direct user-invoked image-generation action associated with a thread.

### Thread Ownership

The action is always pinned to a thread:

- active thread when present
- newly created main-agent thread when absent

This guarantees artifact ownership and deterministic history placement.

### User Intent

The thread history should reflect that this generation was forced by the operator rather than chosen by the model.

The history entry may still reuse tool-style rendering semantics, but metadata should make the source explicit.

## Persistence And Thread History

### Durable Storage

Generated images must not remain temp-only artifacts.

After generation succeeds, the daemon should persist the image into the thread’s files area under the target tamux directory. The persistence step should return:

- durable path
- mime type
- byte count where available
- thread/artifact metadata for history and work context

### Touched Files / Artifacts

The persisted image should appear in the thread’s touched-files or artifact history the same way file-producing tools such as `create_file` contribute thread-scoped outputs.

This is important for:

- thread work context
- later retrieval
- consistency with other thread-scoped outputs

### Inline Rendering

The same generation should also appear inline in the thread at the point where it happened.

Reuse the existing thread message/content-block rendering path rather than inventing a separate image-only timeline object. A synthetic result-style message with an image content block is sufficient if it preserves:

- ordering
- thread ownership
- renderability
- artifact linkage

### Message Shape

The generated-image history item should be able to carry:

- prompt or short generation description
- image content block with durable file or data-backed reference
- metadata that marks the event as user-forced

This mirrors the current inline handling used for content-rich thread events such as compactions and tool-like outputs.

## Daemon Media Persistence Refactor

`execute_generate_image` currently returns a temp-path-oriented result. Refactor the flow so the daemon can optionally persist generated media into a thread-owned destination.

Two layers are needed:

1. upstream generation
   - call the provider endpoint
   - parse `b64_json` or `url`
   - build normalized media result metadata

2. thread persistence
   - write or copy into thread storage
   - register the artifact with the thread/work-context machinery
   - emit a renderable thread event

This allows both:

- user-forced `/image`
- model-triggered `generate_image`

to converge on the same durable persistence path over time.

## Error Handling

### No Active Thread

If no thread is open:

- create a new main-agent thread
- continue image generation in that thread
- fail only if thread creation itself fails

### Missing Image-Generation Capability

If the selected or resolved model does not support `image_generation`, return a capability-specific error.

The error should explain that the provider may be reachable but the chosen model is not known to support image generation.

### URL-Only Upstream Results

If the upstream provider returns only a URL:

- preferred: download and persist into thread storage when allowed
- fallback: keep the URL and surface a clear warning that the artifact is not yet durably owned by the thread

### Persistence Failure After Successful Generation

If upstream generation succeeds but thread persistence fails:

- surface the generation result
- report that thread append/persistence failed
- avoid silently dropping the image

## STT/TTS Compatibility Boundary

Current STT/TTS functionality is explicitly treated as a non-regression boundary.

That means:

- do not replace the current directional audio detection model unless tests prove parity
- do not change working `speech_to_text` and `text_to_speech` transport routing without necessity
- do not change existing audio hotkeys or settings behavior as part of this task unless required to preserve correctness
- do not let image-generation filtering leak into STT/TTS pickers
- do not let coarse audio pricing or generic media hints break the current precise STT/TTS filtering rules

Any shared helper introduced by this design must preserve the current audio behavior first and only then layer `image_generation` alongside it.

## File Impact

Expected write scope:

- daemon media capability helpers
- daemon provider/model fetch helpers for OpenRouter filtering
- daemon media generation persistence path
- daemon thread/artifact append path for generated images
- TUI slash-command handling for `/image`
- TUI model filtering helpers where image-generation choices are exposed
- focused tests for derived features, thread persistence, and non-regression coverage

Likely touch points include:

- `crates/amux-daemon/src/agent/llm_client/helpers.rs`
- `crates/amux-daemon/src/agent/tool_executor/media_tools.rs`
- `crates/amux-daemon/src/agent/tool_executor/catalog/part_c.rs`
- thread/history persistence files in daemon
- `crates/amux-tui/src/app/commands.rs`
- `crates/amux-tui/src/app/modal_handlers_enter.rs`
- `crates/amux-tui/src/widgets/model_picker.rs`
- frontend feature-display or filtering helpers where needed

## Testing

Add or update focused tests for:

- derived feature evaluation for built-in models
- derived feature evaluation for fetched remote models
- preservation of current directional STT/TTS filtering behavior
- OpenRouter targeted model-fetch query behavior
- image-generation model filtering based on derived features
- `/image` against an active thread
- `/image` with no active thread, creating a new main-agent thread first
- durable persistence of generated images into thread storage
- thread touched-file/artifact recording for generated images
- inline renderable history entry creation for generated images
- fallback handling for URL-only upstream image responses

Verification must explicitly include current STT/TTS functionality so the feature refactor cannot silently break already-working speech behavior.

## Risks

- capability derivation may drift between Rust and TypeScript if helper rules are not kept aligned
- OpenRouter-specific filtering may accidentally narrow general model fetch if the call sites are not separated carefully
- thread persistence changes could create partial-success states if generation succeeds before artifact registration fails
- inline image rendering can regress if durable-path references are not wired compatibly with existing content-block rendering

## Open Questions Resolved

- Existing modality fields stay unchanged
- The new schema is feature-level rather than low-level directional capability replacement
- `/image` is pinned to the currently opened thread
- If no thread is open, tamux creates a new main-agent thread first
- Generated images should appear both as thread-touched files/artifacts and inline in thread history
- Preserving current STT/TTS behavior is a hard requirement, not an optional cleanup goal
