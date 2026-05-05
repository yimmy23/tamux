---
name: automation-scripting-task
description: Use when creating scripts, scheduled jobs, one-off automation, batch operations, or developer utilities.
recommended_skills:
recommended_guidelines:
  - general-programming
  - coding-task
  - testing-task
---

## Overview

Automation scripts should be reliable, reproducible, and resilient to failures.

## Workflow

1. Define the automation scope: what input, what output, what failure modes.
2. Test the manual process first before writing automation.
3. Write scripts with error handling and informative error messages.
4. Scripts should be idempotent when possible.
5. Log progress: which step, what's next, what went wrong.
6. Handle edge cases: empty input, malformed data, network failures, permission issues.
7. Test on a small sample before running on the full dataset.

## Quality Gate

An automation script is ready when it handles expected inputs, error cases, and produces verified output.