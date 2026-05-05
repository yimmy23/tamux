---
name: file-management-task
description: Use for creating, moving, renaming, cleaning, summarizing, or organizing local files and folders.
recommended_skills:
recommended_guidelines:
  - terminal-operations-task
  - automation-scripting-task
---
## Overview

File management tasks are prone to irreversible mistakes. This guideline enforces safe workflows before any destructive or bulk operation.

## Workflow

1. Inspect the current layout and naming scheme before making changes.
2. Plan the target layout explicitly — don't approximate.
3. Test any pattern-based operation on a small sample first.
4. Prefer `mv` over copy-then-delete for moves. Keep copies of originals until verified.
5. For bulk operations, generate a preview or dry-run output before executing.
6. Check disk space before large copies or downloads.
7. After the operation, verify a representative sample of results.

## Quality Gate

Do not run destructive operations without listing the affected files first. Confirm the list matches expectations.