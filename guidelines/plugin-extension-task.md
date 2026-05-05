---
name: plugin-extension-task
description: Use for creating, installing, configuring, testing, or troubleshooting Zorai plugins and extensions.
recommended_skills:
recommended_guidelines:
  - general-programming
  - coding-task
  - testing-task
---

## Overview

Plugin development requires understanding the extension points, API contracts, and isolation boundaries.

## Workflow

1. Read the plugin system documentation: extension points, lifecycle hooks, and API surface.
2. Study existing plugins for patterns and conventions before implementing.
3. Keep plugins focused on a single responsibility.
4. Handle initialization, cleanup, and error states properly.
5. Test the plugin both in isolation and integrated with the host system.
6. Document version compatibility and dependencies.
7. Clean up resources on unload or deactivation.

## Quality Gate

A plugin is complete when it works with the target version, handles errors gracefully, and cleans up after itself.