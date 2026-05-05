---
name: agent-coordination-task
description: Use when delegating, coordinating subagents, handing off work, or splitting parallel tasks.
recommended_skills:
  - dispatching-parallel-agents
  - subagent-driven-development
  - writing-plans
---

## Overview

Multi-agent coordination requires clear task boundaries, explicit handoffs, and shared context.

## Workflow

1. Decompose the work into independent, non-overlapping tasks before assigning to agents.
2. Give each agent a clear scope: what it owns, what it must produce, what it must not touch.
3. Specify the handoff format: what context is passed between agents.
4. Set expectations for quality and completeness on each sub-task.
5. Monitor progress across agents and integrate results.
6. If agents disagree, escalate to human-in-the-loop rather than letting one override.
7. Verify the integrated result holistically — each agent's output in isolation is not sufficient.

## Quality Gate

Multi-agent work is complete when all sub-tasks produce verifiable outputs that integrate cleanly.