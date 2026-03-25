---
phase: 20-gmail-calendar-validation
plan: 01
subsystem: plugins
tags: [gmail, calendar, google, oauth2, handlebars, yaml-skills, npm]

# Dependency graph
requires:
  - phase: 14-plugin-manifest
    provides: PluginManifest serde struct, JSON Schema v1, plugin loader
  - phase: 17-plugin-api-proxy
    provides: Handlebars template engine, API proxy, plugin_api_call tool
  - phase: 18-plugin-oauth
    provides: OAuth2 PKCE flow, encrypted credential storage
  - phase: 19-plugin-skills-commands
    provides: Skill bundling, command registration, skill auto-discovery
provides:
  - Gmail plugin manifest with 3 API endpoints (list_inbox, get_message, search_messages)
  - Calendar plugin manifest with 1 API endpoint (list_events_today)
  - Gmail YAML skill teaching agent two-step inbox retrieval pattern
  - Calendar YAML skill teaching agent RFC3339 date-based event queries
  - npm-publishable package with correct files array
  - Google Cloud Console setup README with restricted scope documentation
affects: [20-02, 20-03]

# Tech tracking
tech-stack:
  added: []
  patterns:
    - Two-step API retrieval pattern (list IDs then fetch details) for Gmail
    - Single-step query with agent-computed date parameters for Calendar
    - Handlebars response templates using only available helpers (default, urlencode, join, each)
    - Nested plugin package structure (gmail/ and calendar/ subdirectories in single npm package)

key-files:
  created:
    - plugins/tamux-plugin-gmail-calendar/gmail/plugin.json
    - plugins/tamux-plugin-gmail-calendar/calendar/plugin.json
    - plugins/tamux-plugin-gmail-calendar/gmail/skills/gmail-inbox.yaml
    - plugins/tamux-plugin-gmail-calendar/calendar/skills/calendar-today.yaml
    - plugins/tamux-plugin-gmail-calendar/package.json
    - plugins/tamux-plugin-gmail-calendar/README.md
  modified: []

key-decisions:
  - "Response templates avoid eq helper (not registered) by iterating all headers and letting agent extract Subject/From/Date"
  - "Calendar template uses static header instead of items.length (not available in Handlebars); agent counts events when presenting"
  - "No Authorization headers in endpoint definitions; OAuth token injected automatically by API proxy layer"
  - "Gmail uses format=METADATA with metadataHeaders for efficient metadata-only retrieval"

patterns-established:
  - "Plugin manifest pattern: complete example of declaring endpoints, auth, settings, commands, and skills"
  - "Two-step retrieval skill: agent chains list+get calls guided by YAML skill instructions"
  - "Date parameter pattern: agent computes dynamic values (RFC3339 dates) and passes as endpoint params"

requirements-completed: [GMAI-01, GMAI-02, GMAI-03, GMAI-04, GMAI-05, GMAI-06, GMAI-08, GMAI-09]

# Metrics
duration: 3min
completed: 2026-03-25
---

# Phase 20 Plan 01: Gmail/Calendar Plugin Package Summary

**Declarative Gmail and Calendar plugin manifests with Handlebars response templates, YAML skills for agent two-step retrieval and date-based queries, and npm-publishable package with Google Cloud Console setup guide**

## Performance

- **Duration:** 3 min
- **Started:** 2026-03-25T08:20:51Z
- **Completed:** 2026-03-25T08:24:22Z
- **Tasks:** 2
- **Files created:** 6

## Accomplishments
- Gmail manifest with 3 endpoints (list_inbox, get_message, search_messages) covering inbox reading and email search via Gmail REST API v1
- Calendar manifest with 1 endpoint (list_events_today) covering today's events via Google Calendar REST API v3
- Both manifests declare OAuth2 auth with PKCE, Google authorization/token URLs, and read-only scopes
- YAML skills instruct agent on Gmail two-step retrieval pattern and Calendar date-based queries with example tool call JSON
- npm package.json with correct files array for publishing; README with step-by-step Google Cloud Console setup including restricted scope documentation

## Task Commits

Each task was committed atomically:

1. **Task 1: Create Gmail and Calendar plugin manifests** - `a3c7b18` (feat)
2. **Task 2: Create YAML skills, npm package.json, and README** - `1ddca29` (feat)

## Files Created/Modified
- `plugins/tamux-plugin-gmail-calendar/gmail/plugin.json` - Gmail manifest with 3 endpoints, OAuth2, settings, commands
- `plugins/tamux-plugin-gmail-calendar/calendar/plugin.json` - Calendar manifest with 1 endpoint, OAuth2, settings, commands
- `plugins/tamux-plugin-gmail-calendar/gmail/skills/gmail-inbox.yaml` - Agent skill for two-step inbox retrieval and search
- `plugins/tamux-plugin-gmail-calendar/calendar/skills/calendar-today.yaml` - Agent skill for calendar event listing with RFC3339 dates
- `plugins/tamux-plugin-gmail-calendar/package.json` - npm package metadata with files array
- `plugins/tamux-plugin-gmail-calendar/README.md` - Google Cloud Console setup guide with restricted scope docs

## Decisions Made
- Response templates iterate all headers with `{{#each}}` rather than using conditional `eq` (helper not registered in Handlebars registry)
- Calendar response template uses static "## Today's Calendar" header; agent counts events when presenting to user (Handlebars lacks `.length` for arrays)
- No `Authorization` headers in endpoint definitions since the API proxy layer from Phase 17/18 automatically injects OAuth tokens
- Gmail `get_message` uses `format=METADATA&metadataHeaders=Subject&metadataHeaders=From&metadataHeaders=Date` for efficient metadata-only retrieval
- `gmail.readonly` restricted scope documented prominently with Testing mode guidance and OAuth verification review note

## Deviations from Plan

None - plan executed exactly as written.

## Known Stubs

None - all files contain complete, functional content.

## Issues Encountered

None

## User Setup Required

None - no external service configuration required for this plan. Google Cloud Console setup is documented in the plugin README for end users who install the plugin.

## Next Phase Readiness
- Plugin manifests ready for nested install detection enhancement (Plan 20-02)
- Skills and manifests ready for end-to-end validation (Plan 20-03)
- All 6 files exist and pass structural validation

## Self-Check: PASSED

All 6 created files verified on disk. Both task commits (a3c7b18, 1ddca29) verified in git log.

---
*Phase: 20-gmail-calendar-validation*
*Completed: 2026-03-25*
