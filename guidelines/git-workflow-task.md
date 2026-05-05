---
name: git-workflow-task
description: Use for branch inspection, commits, diffs, merges, rebases, PR prep, or repository hygiene.
recommended_skills:
recommended_guidelines:
  - general-programming
  - coding-task
  - code-review
---

## Overview

Clean git history supports auditing, debugging, and collaboration.

## Workflow

1. Keep commits focused on a single logical change — no mixing concerns.
2. Write descriptive commit messages: what changed and why, not just what.
3. Rebase feature branches onto the target branch before merging for linear history.
4. Resolve merge conflicts with understanding, not brute force — know both sides.
5. Squash fixup commits before merging to main.
6. Never force-push to shared branches without coordination.
7. Tag releases after merging.

## Quality Gate

A branch is ready to merge when commits are focused, messages are descriptive, and history is linear.