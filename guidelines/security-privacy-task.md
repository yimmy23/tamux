---
name: security-privacy-task
description: Use for security reviews, secret handling, privacy-sensitive data, permissions, auth, or abuse-risk questions.
recommended_skills:
  - security-best-practices
  - security-threat-model
  - security-ownership-map
recommended_guidelines:
  - general-programming
  - legal-compliance-task
  - deployment-release-task
---

## Overview

Security work must be intentional, verified, and documented. Assumptions about trust boundaries are the most common source of vulnerabilities.

## Workflow

1. Identify the trust boundary: what is trusted, what is untrusted, where validation matters.
2. Never roll your own cryptography — use standard libraries.
3. Validate and sanitize all input from untrusted sources.
4. Use parameterized queries to prevent injection attacks.
5. Store secrets in vaults or environment variables, never in code.
6. Apply least privilege: limit access to what's necessary.
7. Log security-relevant events for audit.

## Quality Gate

Security work is complete when threat models are documented and mitigations are verified.