---
name: security-privacy-task
description: Use for security reviews, secret handling, privacy-sensitive data, permissions, auth, or abuse-risk questions.
recommended_skills:
  - security-best-practices
  - security-threat-model
  - security-ownership-map
---

# Security And Privacy Task Guideline

Security work should be concrete about assets, actors, and controls.

## Workflow

1. Identify assets, trust boundaries, privileges, data sensitivity, and attacker capabilities.
2. Verify current behavior in code or configuration before recommending changes.
3. Minimize secret exposure in logs, prompts, tests, screenshots, and files.
4. Prefer defense-in-depth for auth, authorization, input validation, storage, and auditability.
5. Distinguish vulnerabilities from hardening opportunities.
6. Provide actionable mitigations and validation steps.

## Quality Gate

Do not give generic security advice when the user needs repository-grounded risk and fixes.
