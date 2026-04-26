---
name: automation-scripting-task
description: Use when creating scripts, scheduled jobs, one-off automation, batch operations, or developer utilities.
recommended_skills:
  - test-driven-development
  - systematic-debugging
  - verification-before-completion
---

# Automation Scripting Task Guideline

Automation should be safe to rerun and clear when it fails.

## Workflow

1. Define inputs, outputs, side effects, and idempotency requirements.
2. Add dry-run or confirmation behavior for destructive or broad operations.
3. Handle paths, spaces, missing tools, permissions, and partial failure.
4. Use structured APIs or parsers when available.
5. Test with small representative fixtures before running broadly.
6. Print useful progress and final summaries without leaking secrets.

## Quality Gate

Do not automate a destructive action without a bounded target, verification, and recovery story.
