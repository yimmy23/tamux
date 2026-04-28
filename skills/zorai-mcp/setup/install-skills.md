---
name: install-skills
description: Import community skills into zorai from the registry or a direct URL, then verify they appear in the local skill catalog
compatibility:
  - zorai
  - claude-code
allowed_tools:
  - bash
  - read_file
metadata:
  category: setup
  platform: linux,macos,windows
---

# Install Skills — import reusable community skills into zorai

## Agent Rules

- **Use `zorai skill import <source>` as the canonical install flow**
- **Search first when the user does not provide an exact skill name** using `zorai skill search <query>`
- **Verify the imported skill is present** with `zorai skill list` or `zorai skill inspect <name>`
- **Use `--force` only when the user explicitly accepts bypassing security warnings**
- **Treat direct URLs and registry names differently** — keep the source explicit in the command you run

## Reference

### Primary install command

```bash
zorai skill import <source>
```

Where `source` can be:

- a registry skill name
- a direct URL to a published skill archive

### Discover skills before import

```bash
zorai skill search <query>
```

Examples:

```bash
zorai skill search browser
zorai skill search git
zorai skill search debugging
```

### Verify after import

```bash
zorai skill list
zorai skill inspect <name-or-variant-id>
```

Expected outcome:

- the imported skill appears in the local catalog
- its maturity/status is visible in the list output
- inspect returns the variant metadata and content details

### Security override

If the import flow warns about findings and the user explicitly wants to proceed anyway:

```bash
zorai skill import <source> --force
```

Use this only with user consent.

### Related commands

```bash
zorai skill export <name>
zorai skill publish <name>
zorai skill reject <name>
zorai skill promote <name> --to active
```

## Gotchas

- `zorai skill import` is the install command; there is no separate `zorai install skill` CLI flow
- imported skills can be blocked or downgraded by scan results; read the terminal output instead of assuming success
- a skill can appear under a variant ID rather than only the friendly name; `zorai skill inspect` handles both
- `--force` bypasses warnings, not hard blocks imposed by the daemon