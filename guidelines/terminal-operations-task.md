---
name: terminal-operations-task
description: Use when running terminal commands, managing shell sessions, or executing CLI operations on files or systems.
recommended_skills:
recommended_guidelines:
  - automation-scripting-task
  - file-management-task
  - environment-setup-task
---


## Overview

Terminal operations should be deliberate, traceable, and safe. This guideline prevents destructive accidents and ensures reproducibility.

## Workflow

1. Preview commands before making changes: use dry-run flags, --list before rm/cp/mv.
2. Test loops and glob patterns on a sample before full execution.
3. Redirect script output to files for later inspection.
4. Use absolute paths or confirm cwd before destructive commands.
5. Background long-running tasks with nohup or tmux.
6. Log command history when investigating issues.

## Quality Gate

Do not run destructive commands without previewing affected paths first.