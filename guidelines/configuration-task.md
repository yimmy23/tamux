---
name: configuration-task
description: Use for settings, environment variables, config files, feature flags, secrets, or runtime defaults.
recommended_skills:
  - security-best-practices
recommended_guidelines:
  - general-programming
  - environment-setup-task
  - testing-task
---

## Overview

Configuration changes are a common source of subtle bugs. This guideline ensures changes are intentional, tested, and reversible.

## Workflow

1. Read the current configuration before changing it.
2. Understand what each setting does — never copy-paste without comprehension.
3. Make one change at a time and verify the effect.
4. Document why a non-default value was chosen.
5. Keep secrets out of configuration files — use environment variables or vaults.
6. Validate configuration syntax before deployment.
7. Have a rollback plan for configuration changes in production.

## Quality Gate

Do not apply configuration changes in production without testing them in a matching environment first.