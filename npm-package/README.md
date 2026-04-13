# tamux

This npm package installs the tamux launcher and downloads the platform-specific tamux binaries on install or first run.

Full project documentation lives in the main repository README:

- https://github.com/mkurman/tamux/blob/main/README.md

Project homepage:

- https://tamux.app

Quick start:

```bash
npm install -g tamux
tamux --help
```

The npm installer downloads the platform release bundle and installs:

- launcher binaries under the package `bin/` directory used by `npm` and `npx`
- built-in skills into the canonical runtime root at `~/.tamux/skills` on Unix or `%LOCALAPPDATA%\\tamux\\skills` on Windows
