# Phase 9: Distribution - Context

**Gathered:** 2026-03-24
**Status:** Ready for planning

<domain>
## Phase Boundary

Anyone can install tamux with a single command on any platform (Linux x64/arm64, macOS arm64/x64, Windows x64) and be running within seconds. This phase delivers the npm package, standalone install scripts, CI release pipeline, and first-run setup wizard. No new runtime features — only packaging, distribution, and bootstrapping infrastructure.

</domain>

<decisions>
## Implementation Decisions

### npm Package Strategy
- **D-01:** Thin npm package with a postinstall script that detects OS/arch and downloads pre-built binaries from GitLab Releases. Pattern follows esbuild/turbo/prisma.
- **D-02:** The npm package delivers ALL binaries: daemon (`tamux-daemon`), CLI (`tamux`), TUI (`tamux-tui`), and Electron desktop app archive. Full experience from a single `npx tamux`.
- **D-03:** Download source is GitLab Releases (project release artifacts). URL pattern: `gitlab.com/{org}/tamux/-/releases/v{version}/downloads/tamux-{target}.tar.gz`
- **D-04:** `npx tamux` with no prior install downloads binaries, starts daemon, and opens TUI in the current terminal. TUI is the default surface for `npx` since it works everywhere including SSH.

### Binary Hosting & CI Pipeline
- **D-05:** GitLab CI matrix job for cross-platform builds. Linux runner for x64/arm64, macOS runner for universal binary, Windows runner for x64. Leverages existing `build-release.sh` with `--target` support.
- **D-06:** All four platform targets built: linux-x64, linux-arm64, darwin-arm64 + darwin-x64 (universal binary), windows-x64.
- **D-07:** Release artifacts split into two categories per platform: binaries tarball (daemon + CLI + TUI) and Electron app archive (AppImage/deb, dmg/zip, NSIS/portable). Users can grab just CLI tools without the desktop app.
- **D-08:** Naming convention: `tamux-binaries-v{version}-{platform}-{arch}.tar.gz` and `tamux-desktop-v{version}-{platform}-{arch}.{ext}`. SHA256SUMS file alongside.
- **D-09:** Automated release pipeline: pushing a `vX.Y.Z` tag triggers CI build, uploads artifacts to GitLab Releases, and publishes the npm wrapper package. Fully automated.

### Install Script Behavior
- **D-10:** Unix install script (`install.sh`) places binaries in `~/.local/bin`. XDG-conventional, no sudo required. Script adds to PATH via shell profile if not already present.
- **D-11:** Install script downloads daemon + CLI + TUI by default. Offers optional prompt to also install the Electron desktop app: "Install desktop app? [y/N]". Keeps default install lean for servers/containers.
- **D-12:** PowerShell installer places binaries in `C:\Program Files\tamux`. Requires admin elevation. Standard Windows install location.
- **D-13:** SHA256 checksum verification on all downloads. Download SHA256SUMS file, verify before extracting. Warn but continue if sha256sum utility not available.

### First-Run Bootstrapping
- **D-14:** First run triggers an interactive terminal setup wizard before starting the agent. Structured guided experience: provider selection, API key, preferences.
- **D-15:** Setup wizard covers four steps: (1) LLM provider selection + API key entry from 20+ supported providers, (2) default client preference (TUI vs Electron), (3) data directory confirmation (~/.tamux/ or custom), (4) quick connectivity test to verify LLM connection works.
- **D-16:** Scope boundary with Phase 10: Phase 9 handles daemon auto-start + setup wizard (provider, client, data dir, test). Phase 10 handles capability tiers, guided tours, progressive feature disclosure, and concierge onboarding.

### Claude's Discretion
- Setup wizard visual design and terminal formatting
- Exact postinstall script error handling and fallback behavior
- CI pipeline job naming and stage organization
- Whether to generate a shell completion script during install
- Electron app packaging details per platform (existing electron-builder config is a good starting point)

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Build Infrastructure
- `scripts/build-release.sh` — Primary build script with --target cross-compile support, signing, skip flags
- `scripts/build-production-releases.sh` — Multi-platform build orchestrator (Linux + Windows cross-compile)
- `scripts/build-release.bat` / `scripts/build-release.ps1` — Windows-native build scripts
- `scripts/build-release-macos.sh` — macOS wrapper (delegates to build-release.sh)
- `scripts/bump-version.sh` — Version management script
- `scripts/setup.sh` — Dependency checker with profiles (source, desktop)

### Packaging Configuration
- `frontend/package.json` §build — electron-builder config: AppImage/deb (Linux), NSIS/portable (Windows), dmg/zip (macOS), extraResources for daemon+CLI binaries
- `Cargo.toml` — Workspace root, version 0.1.10, 6 member crates
- `rust-toolchain.toml` — Pinned stable toolchain with rustfmt + clippy

### Crate Binaries
- `crates/amux-daemon/Cargo.toml` — tamux-daemon binary
- `crates/amux-cli/Cargo.toml` — tamux CLI binary
- `crates/amux-tui/Cargo.toml` — tamux-tui binary
- `crates/amux-gateway/Cargo.toml` — tamux-gateway binary
- `crates/amux-mcp/Cargo.toml` — tamux-mcp binary

### Agent Config & First-Run
- `crates/amux-daemon/src/agent/config.rs` — AgentConfig struct (provider, model, api_key, base_url fields needed for setup wizard)
- `crates/amux-cli/src/main.rs` — CLI entry point where setup wizard would be triggered

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `scripts/build-release.sh` — Already handles cross-compilation with `--target`, signing with `--sign`, selective skip flags. Core of the CI pipeline.
- `scripts/build-production-releases.sh` — Multi-platform orchestrator. Can be adapted for CI matrix job coordination.
- `frontend/package.json` build config — electron-builder already configured for all three platforms with correct extraResources paths.
- `scripts/setup.sh` — Dependency detection logic (Rust toolchain, Node, platform detection) reusable for install script validation.

### Established Patterns
- Workspace version managed centrally in `Cargo.toml` `[workspace.package]` — all crates inherit version 0.1.10.
- Electron bundles Rust binaries as `extraResources` in `dist/` → `bin/` mapping.
- Platform detection in build scripts uses `uname -s` / `uname -m` pattern.

### Integration Points
- `crates/amux-cli/src/main.rs` — CLI entry point where first-run detection and setup wizard would hook in.
- `crates/amux-daemon/src/agent/config.rs` — Config file that the setup wizard needs to write to (`~/.tamux/agent/config.json`).
- `crates/amux-daemon/src/main.rs` — Daemon entry point for auto-start logic.

</code_context>

<specifics>
## Specific Ideas

- npm postinstall pattern modeled after esbuild/turbo/prisma (thin wrapper + platform binary download)
- TUI as default `npx` surface since it works over SSH (Electron available separately)
- Split artifacts: users on headless servers can grab just the binaries tarball without the Electron app
- Setup wizard is a terminal TUI experience, not a web page — consistent with tamux's terminal-native identity

</specifics>

<deferred>
## Deferred Ideas

- Homebrew tap / formula for macOS — could be a follow-up after initial distribution works
- Scoop/Chocolatey/winget manifests for Windows package managers
- Docker image for containerized deployment
- Auto-update mechanism (check for new versions, offer to update)
- AUR package for Arch Linux
- Snap/Flatpak packages

None — discussion stayed within phase scope (all deferred ideas are natural extensions, not scope creep)

</deferred>

---

*Phase: 09-distribution*
*Context gathered: 2026-03-24*
