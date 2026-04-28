---
name: install-plugins
description: Install zorai runtime plugins from npm, GitHub, or a local directory and verify they are registered with the daemon
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

# Install Plugins — add runtime extensions to zorai

## Agent Rules

- **Prefer `zorai plugin add <source>`** for current runtime plugin installs
- **Allow `zorai install plugin <package>` only as the legacy npm-compatible shortcut**
- **Verify the daemon sees the plugin** after installation by running `zorai plugin ls`
- **Use plugin source that is explicit** — npm package name, GitHub URL, or local directory path
- **Do not claim success on file copy alone** — plugin install is only complete once registration succeeds or the daemon-facing status is explained clearly

## Reference

### What this installs

Runtime plugins extend zorai with extra commands, tools, settings, and UI integrations. They are installed into the zorai data directory and then registered with the daemon so they become active.

### Preferred command

```bash
zorai plugin add <source>
```

Supported `source` forms:

- npm package name: `zorai plugin add zorai-plugin-example`
- GitHub URL: `zorai plugin add https://github.com/org/repo`
- local directory: `zorai plugin add /path/to/plugin-dir`

### Legacy compatibility command

For npm-style installs only, the older shorthand still works:

```bash
zorai install plugin <package>
```

Use this only when the user explicitly asks for the legacy install form or already uses it.

### Verify after install

Run:

```bash
zorai plugin ls
```

Expected outcome:

- the plugin appears in the installed list
- it shows as enabled if registration succeeded
- daemon registration errors are surfaced clearly if activation failed

### Useful follow-up commands

```bash
zorai plugin commands
zorai plugin disable <name>
zorai plugin enable <name>
zorai plugin remove <name>
```

## Gotchas

- `zorai plugin add` is the primary v2 flow; prefer it over the legacy install shortcut
- local installs must point to a plugin directory, not a single file
- registration can fail even if files were copied successfully; always verify with `zorai plugin ls`
- some plugins expose commands only after the daemon refreshes its plugin registry