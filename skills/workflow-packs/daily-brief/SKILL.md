---
name: daily-brief
description: Canonical daily brief pack for day-start summaries across tasks, routines, approvals, notices, and optional Gmail/Calendar connectors.
tags: [daily, brief, morning, inbox, calendar, approvals, routines]
keywords:
  - daily
  - brief
  - morning
  - inbox
  - calendar
  - approvals
  - routines
triggers:
  - start of day
  - morning check-in
  - daily summary
  - executive summary
context_tags:
  - productivity
  - workflow
canonical_pack: true
delivery_modes:
  - manual
  - routine
  - chat-delivery
prerequisite_hints:
  - "Gmail connector optional: plugin `gmail` should be ready for inbox context."
  - "Calendar connector optional: plugin `calendar` should be ready for agenda context."
  - "Core daemon state must be available for tasks, routines, approvals, and notices."
source_links:
  - docs/operating/routines.md
  - skills/zorai-mcp/operating/tasks.md
  - skills/zorai-mcp/operating/observability.md
mobile_safe: true
approval_behavior: "Read-only by default; any follow-up write-back or message-sending action must be treated as a separate approval-governed step."
---

# Daily Brief

## User story

I want a concise, source-linked day-start brief that merges my local daemon state with optional inbox and calendar signal, so I can start work without manually cross-checking multiple surfaces.

## Pack contract

### Prerequisites and readiness

- Always available with daemon-only sources:
  - current workspace tasks
  - routines and recent routine outcomes
  - pending approvals
  - high-priority notices
- Optional connectors:
  - `gmail` for urgent unread / follow-up candidates
  - `calendar` for today’s agenda and meeting prep
- If optional connectors are unavailable, degrade gracefully and label the omitted section with the connector setup hint instead of failing.

### Inputs and configuration fields

- `mode`: `standard`, `quiet`, or `executive`
- `include_inbox`: boolean
- `include_calendar`: boolean
- `delivery_channel`: `in-app`, `slack`, `discord`, `telegram`, or `whatsapp`
- `time_window`: e.g. today / next 8h / next 24h

### Outputs and delivery targets

- concise summary with sections for:
  - top priorities
  - pending approvals
  - routine health
  - today’s agenda
  - urgent inbox items
  - connector health flags
- every connector-backed line should include an original source URL or stable ID when available
- output must remain readable in mobile chat clients; keep bullets short and avoid wide tables

## Manual run recipe

1. Read current workspace tasks/notices/routines.
2. If `gmail` is ready, fetch inbox shortlist.
3. If `calendar` is ready, fetch upcoming events.
4. Produce one merged brief with explicit `available` vs `omitted` sections.

## Example routine wiring

Use a task-backed routine scheduled for weekday mornings. Example `target_payload.description`:

`Prepare the Daily Brief pack with executive mode, optional Gmail/Calendar enrichment, and mobile-safe delivery summary.`

## Example prompt

`Run the Daily Brief pack in executive mode for today. Include pending approvals, routine failures, urgent inbox items, and the next meetings. If Gmail or Calendar is not ready, degrade gracefully with setup hints.`

## Failure and recovery behavior

- Missing Gmail/Calendar readiness -> report degraded section with setup hint.
- No tasks/routines/notices -> emit a short “quiet day” brief instead of an empty failure.
- Delivery-channel issues -> keep the brief available in-app and report the failed fan-out separately.

## Verification checklist

- [ ] Manual proof works with daemon-only data.
- [ ] Manual proof works with Gmail degraded.
- [ ] Manual proof works with Calendar degraded.
- [ ] Routine preview/run-now path is documented.
- [ ] Output stays mobile-safe and source-linked.
