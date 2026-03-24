# Phase 9: Distribution - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-03-24
**Phase:** 09-distribution
**Areas discussed:** npm package strategy, Binary hosting & CI pipeline, Install script behavior, First-run bootstrapping

---

## npm package strategy

### How should `npx tamux` deliver the platform binary?

| Option | Description | Selected |
|--------|-------------|----------|
| Postinstall download | Thin npm package with postinstall script that detects OS/arch and downloads correct binary from release server. Used by esbuild, turbo, prisma. | ✓ |
| Platform-specific optional deps | Main package declares optionalDependencies like @tamux/linux-x64. npm auto-installs matching one. Used by SWC, Rollup. | |
| Fetch on first run | JS shim downloads binary on first invocation. Zero install-time side effects. | |

**User's choice:** Postinstall download
**Notes:** Standard pattern, small npm package footprint

### What binaries should the npm package include?

| Option | Description | Selected |
|--------|-------------|----------|
| Daemon + CLI only | Lean, server-friendly | |
| Daemon + CLI + TUI | Also bundle TUI for terminal-first users | |
| Everything | Daemon + CLI + TUI + Electron app | ✓ |

**User's choice:** Everything (all binaries including Electron app)

### Where should the postinstall script download binaries from?

| Option | Description | Selected |
|--------|-------------|----------|
| GitLab Releases | Download from project's release artifacts. Stays in existing GitLab infrastructure. | ✓ |
| Custom domain (tamux.dev) | Host at releases.tamux.dev or CDN. More control, requires infrastructure. | |
| GitHub Releases mirror | Mirror to GitHub for wider compatibility. | |

**User's choice:** GitLab Releases

### How should `npx tamux` work when run without prior install?

| Option | Description | Selected |
|--------|-------------|----------|
| Download + launch daemon + open TUI | Auto-start daemon, open TUI. Works everywhere including SSH. | ✓ |
| Download + launch daemon + open Electron | Opens full desktop app. Fails on headless. | |
| Download + prompt for client choice | Ask user which client. More flexible but more friction. | |

**User's choice:** Download + launch daemon + open TUI

---

## Binary hosting & CI pipeline

### How should cross-platform binaries be built in CI?

| Option | Description | Selected |
|--------|-------------|----------|
| GitLab CI matrix | Custom .gitlab-ci.yml with matrix jobs per platform. Stays in existing infrastructure. | ✓ |
| cargo-dist with GitLab adapter | Use cargo-dist config adapted to GitLab CI. Gets naming/checksums/installers. | |
| GitHub Actions (mirror repo) | Push to GitHub mirror for native cargo-dist support. | |

**User's choice:** GitLab CI matrix

### Which platform targets should be built?

| Option | Description | Selected |
|--------|-------------|----------|
| linux-x64 | Primary Linux target | ✓ |
| linux-arm64 | Raspberry Pi, ARM servers | ✓ |
| darwin-arm64 + darwin-x64 | macOS universal binary | ✓ |
| windows-x64 | Windows desktop | ✓ |

**User's choice:** All four targets

### How should release artifacts be organized and named?

| Option | Description | Selected |
|--------|-------------|----------|
| Flat tarball per platform | Single archive with everything per platform | |
| Split: binaries + Electron separate | Separate binary and desktop app downloads | ✓ |
| cargo-dist naming convention | Follow cargo-dist naming patterns | |

**User's choice:** Split artifacts (binaries separate from Electron)

### Should CI auto-publish npm on tagged releases?

| Option | Description | Selected |
|--------|-------------|----------|
| Yes, auto-publish on tag | Full automation: build → GitLab Releases → npm publish | ✓ |
| Manual npm publish | CI builds artifacts, npm publish is manual | |
| You decide | Claude's discretion | |

**User's choice:** Yes, auto-publish on tag

---

## Install script behavior

### Where should install.sh place binaries on Unix?

| Option | Description | Selected |
|--------|-------------|----------|
| ~/.tamux/bin | Consistent with data directory. No sudo. | |
| /usr/local/bin | Standard Unix. Requires sudo. | |
| ~/.local/bin | XDG convention. No sudo. Increasingly standard. | ✓ |

**User's choice:** ~/.local/bin

### Should install.sh also install the Electron desktop app?

| Option | Description | Selected |
|--------|-------------|----------|
| Binaries only, Electron optional | Downloads daemon+CLI+TUI. Offers optional Electron install prompt. | ✓ |
| Always install everything | Full download by default | |
| You decide | Claude's discretion | |

**User's choice:** Binaries only, Electron optional

### How should the PowerShell installer work on Windows?

| Option | Description | Selected |
|--------|-------------|----------|
| Download to AppData\tamux | $env:LOCALAPPDATA\tamux\bin. No admin required. | |
| Download to Program Files | C:\Program Files\tamux. Requires admin. Standard. | ✓ |
| Match Unix: ~/.tamux/bin | $HOME\.tamux\bin. Cross-platform consistency. | |

**User's choice:** Program Files (admin required)

### Should install scripts verify downloads with checksums?

| Option | Description | Selected |
|--------|-------------|----------|
| SHA256 verification | Download SHA256SUMS, verify before extracting. Warn if sha256sum unavailable. | ✓ |
| GPG signature verification | Full GPG check. More secure but requires key import. | |
| You decide | Claude's discretion | |

**User's choice:** SHA256 verification

---

## First-run bootstrapping

### What should happen the first time user runs `tamux`?

| Option | Description | Selected |
|--------|-------------|----------|
| Daemon + TUI + inline setup | Auto-start, show concierge welcome in chat | |
| Interactive setup wizard first | Guided terminal wizard before starting | ✓ |
| Just start, configure later | Start immediately, hint about missing config | |

**User's choice:** Interactive setup wizard first

### How much onboarding should Phase 9 implement?

| Option | Description | Selected |
|--------|-------------|----------|
| Minimal: daemon start + provider prompt only | Phase 9 handles setup wizard. Phase 10 handles tours/tiers. | ✓ |
| Medium: daemon + provider + feature intro | Also show a brief overview. Overlaps Phase 10. | |
| You decide | Claude's discretion | |

**User's choice:** Minimal scope (clean boundary with Phase 10)

### What should the setup wizard cover?

| Option | Description | Selected |
|--------|-------------|----------|
| LLM provider selection + API key | Pick provider, enter key, verify | ✓ |
| Default client preference | TUI vs Electron preference | ✓ |
| Data directory confirmation | Confirm ~/.tamux/ or customize | ✓ |
| Quick connectivity test | Send test message to verify LLM works | ✓ |

**User's choice:** All four steps

---

## Claude's Discretion

- Setup wizard visual design and terminal formatting
- Exact postinstall script error handling and fallback behavior
- CI pipeline job naming and stage organization
- Whether to generate shell completion scripts during install
- Electron app packaging details per platform

## Deferred Ideas

- Homebrew tap / formula for macOS
- Scoop/Chocolatey/winget manifests for Windows
- Docker image for containerized deployment
- Auto-update mechanism
- AUR package for Arch Linux
- Snap/Flatpak packages
