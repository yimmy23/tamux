---
name: incident-response-task
description: Use when something is down, broken in production, losing data, exposing secrets, or requiring urgent triage.
recommended_skills:
  - systematic-debugging
recommended_guidelines:
  - debugging-task
  - ci-failure-task
  - deployment-release-task
---
## Overview

Incident response prioritizes service restoration while preserving evidence for post-mortem analysis.

## Workflow

1. Assess severity and impact: how many users affected, revenue impact, data loss risk.
2. Stabilize: stop the bleeding. Rollback, feature flag, or fallback before deep investigation.
3. Communicate: update status page, notify stakeholders, log the incident timeline.
4. Investigate root cause systematically — don't guess.
5. Apply the fix once root cause is confirmed.
6. Verify resolution with monitoring and affected users.
7. Schedule a post-mortem: timeline, root cause, what worked, what didn't, action items.

## Quality Gate

Do not close an incident without a documented root cause and at least one preventive action item.