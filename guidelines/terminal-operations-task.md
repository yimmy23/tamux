---
name: terminal-operations-task
description: Use for shell commands, process inspection, local services, logs, scripts, or environment checks.
recommended_skills:
  - systematic-debugging
  - verification-before-completion
---

# Terminal Operations Task Guideline

Terminal work should be observable and reversible where possible.

## Workflow

1. State what the command is meant to prove or change.
2. Prefer read-only inspection before mutating the environment.
3. Use focused commands and preserve important output for the user.
4. Track long-running processes and stop or report them before finishing.
5. Avoid command chains that hide which step failed.
6. When changing local state, report the path, process, port, or config affected.

## Quality Gate

Do not leave required sessions running silently or claim a command succeeded without checking its exit/result.
