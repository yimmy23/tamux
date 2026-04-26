---
name: api-integration-task
description: Use for integrating HTTP APIs, SDKs, webhooks, auth flows, or external services.
recommended_skills:
  - openai-docs
  - test-driven-development
  - security-best-practices
---

# API Integration Task Guideline

API work should make external contracts explicit.

## Workflow

1. Verify current official docs, endpoint versions, auth requirements, limits, and error shapes.
2. Define request and response types, retries, timeouts, pagination, rate limits, and idempotency.
3. Keep secrets out of logs, tests, and committed files.
4. Handle partial failures and degraded service behavior.
5. Use realistic tests with mocked boundaries only where live calls are inappropriate.
6. Log enough metadata to debug without exposing credentials or personal data.

## Quality Gate

Do not ship an API integration that only handles the ideal 200 response.
