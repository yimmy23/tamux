---
name: git-workflow-task
description: Use for branch inspection, commits, diffs, merges, rebases, PR prep, or repository hygiene.
recommended_skills:
  - using-git-worktrees
  - finishing-a-development-branch
  - requesting-code-review
---

# Git Workflow Task Guideline

Git work must preserve user changes and make history intentional.

## Workflow

1. Inspect status before changing files or history.
2. Distinguish your changes from pre-existing user or generated changes.
3. Avoid destructive commands unless the user explicitly asked for that exact operation.
4. For commits, stage only intended files and write a scoped, specific message.
5. For merges or rebases, understand both sides and verify the working tree afterward.
6. Before PRs or handoff, summarize changed files, tests run, and known risks.

## Quality Gate

Do not overwrite, revert, or include unrelated work just to make the branch look clean.
