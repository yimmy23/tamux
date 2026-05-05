---
name: deployment-release-task
description: Use for releases, packaging, deployment, CI publishing, versioning, or rollout planning.
recommended_skills:
  - testing
recommended_guidelines:
  - general-programming
  - ci-failure-task
  - coding-task
---

## Overview

Deployments carry inherent risk. This guideline ensures safe, traceable releases with rollback readiness.

## Workflow

1. Verify the build/artifact is deterministic and reproducible.
2. Run the full test suite against the target environment.
3. Apply migrations or schema changes before code deployment if backward-compatible.
4. Deploy in stages: canary or rolling, not all-at-once for critical services.
5. Monitor errors and latency after deployment.
6. If errors exceed threshold, rollback immediately — investigate after.
7. Tag the release in version control.

## Quality Gate

Do not deploy without a validated rollback plan and monitoring in place.