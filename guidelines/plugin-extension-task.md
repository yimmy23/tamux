---
name: plugin-extension-task
description: Use for creating, installing, configuring, testing, or troubleshooting Tamux plugins and extensions.
recommended_skills:
  - plugin-creator
  - skill-creator
  - systematic-debugging
---

# Plugin And Extension Task Guideline

Plugin work should keep extension boundaries clear.

## Workflow

1. Identify whether the need is a plugin, skill, guideline, MCP server, app integration, or local script.
2. Follow existing manifest, cache, install, and enable/disable conventions.
3. Keep plugin-owned files separate from user-owned runtime data.
4. Validate commands, settings, secrets, and connection tests.
5. Handle install, update, disable, uninstall, and broken-plugin states.
6. Document how the agent discovers and invokes the extension.

## Quality Gate

Do not mix plugin runtime behavior into core code when the extension boundary can handle it cleanly.
