---
name: environment-setup-task
description: Use for installing tools, configuring local development, onboarding, PATH issues, or machine-specific setup.
recommended_skills:
  - systematic-debugging
  - verification-before-completion
---

# Environment Setup Task Guideline

Setup work should leave the machine in a known, repeatable state.

## Workflow

1. Identify OS, shell, architecture, package manager, and existing versions.
2. Prefer project-documented setup commands before inventing new ones.
3. Avoid global changes when local or user-scoped configuration is enough.
4. Verify PATH, environment variables, permissions, and service state.
5. Run a small smoke test that proves the installed tool works.
6. Document any manual step the user must perform outside the terminal.

## Quality Gate

Do not claim setup is complete because installation succeeded; verify the workflow the user needs.
