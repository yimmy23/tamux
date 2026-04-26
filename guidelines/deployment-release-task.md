---
name: deployment-release-task
description: Use for releases, packaging, deployment, CI publishing, versioning, or rollout planning.
recommended_skills:
  - finishing-a-development-branch
  - verification-before-completion
  - github:gh-fix-ci
---

# Deployment And Release Task Guideline

Release work should make artifacts reproducible and rollback possible.

## Workflow

1. Identify version, target platforms, artifacts, changelog, and release channel.
2. Verify build inputs, packaging files, install scripts, and CI workflows.
3. Run the release-relevant test and build commands.
4. Confirm bundled assets, migrations, config defaults, and compatibility constraints.
5. Plan rollout, smoke checks, monitoring, and rollback.
6. Record exact commands and artifact locations.

## Quality Gate

Do not publish or declare release readiness without verifying the artifacts users will actually install.
