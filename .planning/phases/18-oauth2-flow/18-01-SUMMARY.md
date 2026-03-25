---
phase: 18-oauth2-flow
plan: 01
subsystem: auth
tags: [aes-gcm, encryption, oauth2, ipc, credentials, plugin-security]

# Dependency graph
requires:
  - phase: 14-plugin-manifest
    provides: "Plugin manifest schema, PluginPersistence, PluginInfo struct"
  - phase: 16-plugin-settings-ui
    provides: "Plugin settings persistence with base64 placeholder encoding"
  - phase: 17-api-proxy
    provides: "PluginApiError enum, API proxy pipeline"
provides:
  - "AES-256-GCM crypto module (encrypt/decrypt/key management)"
  - "Credential persistence (get/upsert/auth_status)"
  - "AuthExpired error variant on PluginApiError"
  - "auth_status field on PluginInfo (computed from credential state)"
  - "OAuth2 IPC messages (PluginOAuthStart/Url/Complete)"
  - "Sensitive key redaction for OAuth credential keys"
affects: [18-02-PLAN, 18-03-PLAN, 20-gmail-calendar-validation]

# Tech tracking
tech-stack:
  added: [aes-gcm 0.10.3, rand 0.8]
  patterns: [nonce-prefixed-blob encryption, credential-status-from-db, bincode-enum-append-only]

key-files:
  created:
    - crates/amux-daemon/src/plugin/crypto.rs
  modified:
    - crates/amux-daemon/src/plugin/persistence.rs
    - crates/amux-daemon/src/plugin/api_proxy.rs
    - crates/amux-daemon/src/plugin/mod.rs
    - crates/amux-daemon/src/agent/config.rs
    - crates/amux-daemon/Cargo.toml
    - crates/amux-protocol/src/messages.rs
    - crates/amux-daemon/src/server.rs

key-decisions:
  - "Nonce-prefixed blob format: encrypt returns nonce(12) || ciphertext for self-contained decryption"
  - "skip_serializing_if removed from bincode-serialized Option fields (bincode is not self-describing)"
  - "PluginOAuthStart stub in server.rs returns PluginOAuthComplete with error until Phase 18-02"
  - "auth_status computed per-plugin via SQL query on credential table (no decryption needed)"

patterns-established:
  - "Crypto blob format: 12-byte nonce || AES-256-GCM ciphertext for all credential storage"
  - "Bincode enum variants always appended at end for wire compatibility"
  - "Auth status computed from DB row existence and expiry (not from encrypted content)"

requirements-completed: [AUTH-03, AUTH-06, AUTH-07]

# Metrics
duration: 8min
completed: 2026-03-25
---

# Phase 18 Plan 01: OAuth2 Foundation Summary

**AES-256-GCM crypto module with credential persistence, OAuth2 IPC messages, AuthExpired error variant, and auth_status on PluginInfo**

## Performance

- **Duration:** 8 min
- **Started:** 2026-03-25T07:26:32Z
- **Completed:** 2026-03-25T07:35:22Z
- **Tasks:** 2
- **Files modified:** 9

## Accomplishments
- Created AES-256-GCM encryption module with key file management (0600 perms on Unix), 6 unit tests
- Extended credential persistence with get_credential, upsert_credential, get_auth_status
- Added AuthExpired variant to PluginApiError for token expiry handling
- Added auth_status field to PluginInfo, computed from credential state in SQLite
- Added PluginOAuthStart/PluginOAuthUrl/PluginOAuthComplete IPC messages with bincode roundtrip tests
- Extended sensitive key redaction for client_secret, access_token, refresh_token, oauth, credential

## Task Commits

Each task was committed atomically:

1. **Task 1: Crypto module, credential persistence, AuthExpired, auth_status** - `2f3017c` (feat)
2. **Task 2: IPC protocol messages for OAuth flow** - `607ae8d` (feat)

## Files Created/Modified
- `crates/amux-daemon/src/plugin/crypto.rs` - AES-256-GCM encrypt/decrypt/load_or_create_key with 6 unit tests
- `crates/amux-daemon/src/plugin/persistence.rs` - get_credential, upsert_credential, get_auth_status methods
- `crates/amux-daemon/src/plugin/api_proxy.rs` - AuthExpired variant on PluginApiError
- `crates/amux-daemon/src/plugin/mod.rs` - pub mod crypto, auth_status plumbing in to_plugin_info functions
- `crates/amux-daemon/src/agent/config.rs` - Extended is_sensitive_config_key for OAuth credential keys
- `crates/amux-daemon/Cargo.toml` - Added aes-gcm 0.10.3 and rand 0.8 dependencies
- `crates/amux-protocol/src/messages.rs` - auth_status on PluginInfo, OAuth IPC messages, roundtrip test
- `crates/amux-daemon/src/server.rs` - AuthExpired match arm, PluginOAuthStart handler stub
- `Cargo.lock` - Updated with new dependencies

## Decisions Made
- Used nonce-prefixed blob format (12-byte nonce || ciphertext) for self-contained decrypt without external nonce storage
- Removed `skip_serializing_if` from `PluginOAuthComplete.error` field because bincode is not self-describing and fails roundtrip with conditional serialization
- Added stub handler for `PluginOAuthStart` in server.rs that returns `PluginOAuthComplete` with error message, to be replaced in Phase 18-02
- auth_status computed by checking `access_token` credential existence and expiry in SQLite -- no decryption needed for status check

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Removed skip_serializing_if from bincode-serialized Option field**
- **Found during:** Task 2 (OAuth IPC messages)
- **Issue:** `#[serde(skip_serializing_if = "Option::is_none")]` on `PluginOAuthComplete.error` caused bincode roundtrip failure (UnexpectedEof) because bincode is not self-describing
- **Fix:** Removed the skip_serializing_if annotation, keeping only the Option type
- **Files modified:** `crates/amux-protocol/src/messages.rs`
- **Verification:** test_plugin_oauth_roundtrip passes
- **Committed in:** 607ae8d (Task 2 commit)

---

**Total deviations:** 1 auto-fixed (1 bug)
**Impact on plan:** Consistent with existing Phase 17 decision about bincode/serde incompatibility. No scope creep.

## Issues Encountered
None - compilation clean on first pass after deviation fix.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Crypto module ready for Phase 18-02 (OAuth2 flow engine) to use for token encryption
- Credential persistence methods ready for storing access/refresh tokens
- IPC messages ready for wiring OAuth flow between daemon and clients
- AuthExpired variant ready for API proxy token refresh logic

---
*Phase: 18-oauth2-flow*
*Completed: 2026-03-25*
