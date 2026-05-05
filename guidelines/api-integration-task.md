---
name: api-integration-task
description: Use for integrating HTTP APIs, SDKs, webhooks, auth flows, or external services.
recommended_skills:
recommended_guidelines:
  - general-programming
  - coding-task
  - testing-task
---

## Overview

API integration requires understanding the contract, authentication, error handling, and rate limits before writing code.

## Workflow

1. Read the API documentation: endpoints, request/response schemas, auth method, rate limits, pagination.
2. Test the API with curl or a HTTP client first before writing integration code.
3. Handle HTTP errors explicitly: 4xx (client errors), 5xx (server errors), rate limiting (429), timeouts.
4. Implement retries with exponential backoff for transient failures.
5. Validate response schemas — don't assume the API always returns the expected shape.
6. Log API requests and responses during development for debugging.
7. Write integration tests against a test/staging endpoint, not production.

## Quality Gate

An API integration is complete when it handles success, error, timeout, and rate-limit cases.