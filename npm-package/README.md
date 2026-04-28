# zorai

This npm package installs the zorai launcher and downloads the platform-specific zorai binaries on install or first run.

Full project documentation lives in the main repository README:

- https://github.com/mkurman/zorai/blob/main/README.md

Project homepage:

- https://zorai.app

Quick start:

```bash
npm install -g zor-ai
zorai --help
```

The npm installer downloads the platform release bundle and installs:

- launcher binaries under the package `bin/` directory used by `npm` and `npx`
- built-in skills into the canonical runtime root at `~/.zorai/skills` on Unix or `%LOCALAPPDATA%\\zorai\\skills` on Windows
