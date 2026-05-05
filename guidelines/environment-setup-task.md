---
name: environment-setup-task
description: Use for installing tools, configuring local development, onboarding, PATH issues, or machine-specific setup.
recommended_skills:
recommended_guidelines:
  - general-programming
  - configuration-task
  - deployment-release-task
---

## Overview

Environment setup must be reproducible — manual steps are a source of drift. This guideline ensures setups are scriptable and documented.

## Workflow

1. Check existing documentation and configuration before making changes.
2. Use package managers, dotfile repos, or container definitions — never manual-only steps.
3. Document every dependency with version constraints and installation source.
4. Test the setup from scratch in a clean environment.
5. If using secrets or credentials, document which environment variables need to be set without exposing the values.
6. Keep environment-specific configuration separate from code.
7. Add a validation script that verifies the environment is correctly configured.

## Quality Gate

An environment setup is complete when a new team member can reproduce it from documentation alone.