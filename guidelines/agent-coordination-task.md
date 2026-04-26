---
name: agent-coordination-task
description: Use when delegating, coordinating subagents, handing off work, or splitting parallel tasks.
recommended_skills:
  - dispatching-parallel-agents
  - subagent-driven-development
  - writing-plans
---

# Agent Coordination Task Guideline

Agent coordination should reduce risk and cycle time, not add confusion.

## Workflow

1. Identify independent workstreams and shared state before delegating.
2. Keep blocking work on the critical path local unless delegation truly helps.
3. Give each agent a bounded task, clear output, and disjoint write scope.
4. Tell agents when they are not alone in the codebase and must not revert others' work.
5. Integrate results by reviewing changed files and resolving overlaps.
6. Close or summarize background work before final completion.

## Quality Gate

Do not spawn parallel work without clear ownership and a plan for integration.
