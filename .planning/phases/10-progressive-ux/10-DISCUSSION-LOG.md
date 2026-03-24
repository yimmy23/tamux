# Phase 10: Progressive UX - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-03-24
**Phase:** 10-progressive-ux
**Areas discussed:** Capability tier system, Concierge onboarding flow, Typed bridge + status parity, Feature disclosure strategy

---

## Capability tier system

### How should tier transitions be triggered?

| Option | Description | Selected |
|--------|-------------|----------|
| Operator model signals | Use existing OperatorModel data: session count, tool diversity, goal run usage. Automatic promotion. | |
| Usage milestones | Simple counter-based: 5 conversations → Familiar, first goal run → Power User. | |
| User self-selects | Let user choose tier in settings. | |

**User's choice:** Hybrid — operator model signals + user self-selection in wizard
**Notes:** Both inputs feed the tier assignment. Wizard provides initial self-assessment, operator model drives ongoing promotion.

### Should users be able to override their tier?

| Option | Description | Selected |
|--------|-------------|----------|
| Yes, both directions | Settings toggle to lock tier or manually promote/demote. | ✓ |
| Only upward override | Can skip ahead but not go back. | |
| No override — fully automatic | Trust operator model completely. | |

**User's choice:** Yes, both directions

### What features should be hidden at the Newcomer tier?

| Option | Description | Selected |
|--------|-------------|----------|
| Sub-agent management | Show only at Power User+. | ✓ |
| Goal runs & task queue | Show at Familiar+. | ✓ |
| Memory & learning controls | Show at Expert. | ✓ |
| Gateway & automation config | Show at Familiar+. | ✓ |

**User's choice:** All four categories hidden at Newcomer

### How should hidden features appear in the UI?

| Option | Description | Selected |
|--------|-------------|----------|
| Completely invisible | Don't appear at all. | |
| Visible but locked | Grayed-out with 'Unlock at X tier' label. | |
| Collapsed sections | Exist but collapsed with 'Advanced' label. | ✓ |

**User's choice:** Collapsed sections

### How should the setup wizard ask about experience level?

| Option | Description | Selected |
|--------|-------------|----------|
| Natural language question | 'How familiar are you with AI agents?' → 4 friendly options | ✓ |
| Feature checklist | Show features, ask which they've used. | |
| Skip — always start as Newcomer | Everyone starts at Newcomer. | |

**User's choice:** Natural language question

---

## Concierge onboarding flow

### What should the concierge onboarding look like?

| Option | Description | Selected |
|--------|-------------|----------|
| Guided first goal run | Walk through a hands-on example. 'Give me a small task.' | ✓ |
| Feature tour with action buttons | Series of feature cards with 'Try it' buttons. | |
| Chat-native introduction | Natural welcome message, then wait. | |
| You decide | Claude's discretion. | |

**User's choice:** Guided first goal run

### How should onboarding differ by tier?

| Option | Description | Selected |
|--------|-------------|----------|
| Tier-adapted content | Different onboarding per tier (detailed for Newcomer, minimal for Expert). | ✓ |
| Same for everyone, skip option | Universal onboarding with skip button. | |
| You decide | Claude's discretion. | |

**User's choice:** Tier-adapted content

### Should the concierge onboarding be skippable?

| Option | Description | Selected |
|--------|-------------|----------|
| Skippable with soft nudge | 'Skip for now' + re-offer after 3 sessions. | |
| Not skippable for Newcomers | Must complete at least welcome + first task. | |
| Always skippable, no follow-up | One-shot offer. Never comes back. | ✓ |

**User's choice:** Always skippable, no follow-up

---

## Typed bridge + status parity

### How should the typed bridge helper work?

| Option | Description | Selected |
|--------|-------------|----------|
| getBridge() accessor | Single function replacing all 39 unsafe casts. Returns null outside Electron. | ✓ |
| Zustand bridge store | Bridge in Zustand store, reactive. | |
| You decide | Claude's discretion. | |

**User's choice:** getBridge() accessor

### What status information must be consistent across all clients?

| Option | Description | Selected |
|--------|-------------|----------|
| Agent activity state | idle, thinking, executing tool, etc. | ✓ |
| Current task context | Active thread/task/goal, step, progress. | ✓ |
| Resource indicators | Provider health, gateway connections, memory. | ✓ |
| Recent action summary | Last 3-5 autonomous actions with explanations. | ✓ |

**User's choice:** All four categories

---

## Feature disclosure strategy

### How should tier transitions be announced?

| Option | Description | Selected |
|--------|-------------|----------|
| In-chat concierge message | Natural message in agent thread. | ✓ |
| Notification panel card | Card in notification area. | |
| Settings highlight | New sections glow/pulse briefly. | |
| You decide | Claude's discretion. | |

**User's choice:** In-chat concierge message

### How many features disclosed at once during promotion?

| Option | Description | Selected |
|--------|-------------|----------|
| One at a time, spread over days | One per session over several days. | ✓ |
| All at once with summary | Single message listing everything new. | |
| You decide | Claude's discretion. | |

**User's choice:** One at a time, spread over days

---

## Claude's Discretion

- Exact operator model thresholds for tier promotion
- Concierge welcome message content and tone per tier
- Status widget layout details per client
- Specific feature → collapsed section mappings per tier
- getBridge() edge case handling
- "One feature per session" disclosure queue implementation

## Deferred Ideas

- Per-task safety mode (advise/plan/execute) — good_ux.md gap #4
- Scope policies + dry-run + rollback UX — good_ux.md gap #5
- "Time-travel" UI for error recovery
- Enterprise observability (GenAI semconv) — good_ux.md gap #9
- Agent regression test package + dashboards — good_ux.md gap #10
- Template library for goal runs
- Privacy UI for memory management
- Token budget / cost-limiting telemetry
