---
name: configuration-task
description: Use for settings, environment variables, config files, feature flags, secrets, or runtime defaults.
recommended_skills:
  - security-best-practices
  - verification-before-completion
---

# Configuration Task Guideline

Configuration changes should be explicit, reversible, and discoverable.

## Workflow

1. Identify the config source, precedence order, defaults, and runtime reload behavior.
2. Preserve user-owned config and avoid overwriting local customizations.
3. Validate values and document accepted formats.
4. Keep secrets out of examples, logs, and generated output.
5. Update install, boot, and packaging paths when defaults change.
6. Verify both default and customized configurations.

## Quality Gate

Do not add a setting without defining its default, validation, and interaction with existing settings.
