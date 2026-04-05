# Memory And Security

This document describes the memory and provenance model that tamux ships today.

For the broader daemon/runtime architecture, see [how-tamux-works.md](./how-tamux-works.md). For operator shortcuts and configuration reference, see [reference.md](./reference.md).

## Memory Layers

tamux uses multiple memory layers instead of relying on chat history alone.

- `SOUL.md`: stable agent identity and principles
- `MEMORY.md`: durable project facts, conventions, and environment knowledge
- `USER.md`: operator profile summary derived from structured daemon-owned profile state
- SQLite history: threads, tasks, goal runs, audit events, provenance rows, and other durable runtime state

The editable markdown files are curated memory artifacts. They are intentionally bounded and are not meant to become an unstructured event log.

## How Memory Writes Work

The daemon writes memory through curated update modes:

- `append`: add a new durable fact
- `replace`: replace the current memory content for the target file
- `remove`: explicitly remove an existing fact or block

Before append or replace, tamux extracts fact candidates from the incoming content and checks for contradictions against existing durable facts. Conflicting writes are rejected until the old fact is removed first.

For `USER.md`, append-style writes are staged through the operator-profile reconciliation path rather than treated as final freeform-only file edits.

## Provenance Model

Every durable memory write is also recorded into SQLite as a memory provenance entry. Each entry stores:

- target file (`SOUL.md`, `MEMORY.md`, or `USER.md`)
- update mode
- source kind
- raw content written or removed
- extracted fact keys
- optional thread, task, and goal-run ids
- creation time
- optional confirmation time
- optional retraction time

Memory provenance reports classify entries into operator-facing statuses:

- `active`: a current fact with normal confidence
- `uncertain`: an older fact whose age has reduced confidence below the uncertainty threshold
- `confirmed`: an entry explicitly confirmed by the operator
- `retracted`: an entry explicitly retracted by the operator, or a remove-mode provenance row that represents a removal action

## Relationship Storage

Deep storage now keeps explicit relationships between provenance entries instead of leaving fact replacement history implicit in markdown diffs.

Today, tamux persists `retracts` relationships when a remove-mode memory provenance entry targets an older fact entry with overlapping fact keys. Those relationships are stored in a dedicated SQLite table and surfaced in the memory report.

## Operator Surfaces

### Desktop

The desktop Session Vault exposes a memory provenance mode with:

- status summary counters
- per-entry provenance details
- explicit `Confirm` action for uncertain entries
- explicit `Retract` action for active or uncertain entries
- rendered provenance relationships, including persisted `retracts` links

The desktop audit panel also exposes memory provenance summary counts alongside execution provenance verification.

### TUI

The TUI does not yet have a dedicated memory provenance panel or direct confirm/retract controls. Current deep-storage operator actions are desktop-first.

## Security And Integrity

tamux keeps execution provenance and memory provenance separate but complementary.

- Memory provenance records track where durable facts came from and how they changed over time.
- Execution provenance records track goal, task, approval, tool, and causal-trace events in the signed or hash-linked audit trail.

Trusted execution provenance uses hash-chained log entries, and the current signing path supports Ed25519-backed verification. Memory provenance is not itself a signed ledger, but it is daemon-owned durable state with explicit operator overrides and reportable status.

## Current Limits

The current implementation is intentionally narrower than the original moat draft.

- Relationship storage is currently focused on persisted `retracts` edges.
- The desktop UI is the only operator surface with direct memory confirm/retract actions.
- There is still no full graph exploration UI for memory relationships.

Those are product limits, not hidden behavior. The current shipped model is provenance-backed curated memory with explicit confirmation, explicit retraction, and persisted remove-to-fact linkage.