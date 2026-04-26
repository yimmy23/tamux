---
name: code-review
description: Use when reviewing local changes, pull requests, diffs, or proposed patches.
recommended_skills:
  - receiving-code-review
  - requesting-code-review
  - security-best-practices
---

# Code Review Guideline

Code review should prioritize correctness and risk over style commentary.

## Workflow

1. Identify the intended behavior from the request, commit message, issue, or surrounding code.
2. Inspect changed files and nearby callers that depend on the changed contract.
3. Look first for bugs, regressions, missing tests, data loss, race conditions, security issues, and user-facing breakage.
4. Validate claims against code and tests; do not assume a change is safe because it is small.
5. Report findings first, ordered by severity, with file and line references.
6. Include open questions only when they change the review outcome.

## Quality Gate

If no issues are found, say that directly and mention remaining verification gaps or residual risk.
