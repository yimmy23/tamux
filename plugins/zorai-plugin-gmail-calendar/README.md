# zorai-plugin-gmail-calendar

Gmail and Google Calendar connectors for zorai. They now expose readiness-first daily-loop surfaces instead of only raw read/write calls.

- **Gmail**: inbox/search/thread context, draft/reply/send, archive/trash/star/read state changes
- **Calendar**: schedule listing, event context, create/update/delete, RSVP/attendance, meeting prep

## Prerequisites

- zorai v2.0 or later
- A Google account
- A Google Cloud project with OAuth credentials (see setup below)

## Google Cloud Console Setup

Follow these steps to create the OAuth credentials your plugin needs.

### 1. Create a Google Cloud Project

1. Go to [Google Cloud Console](https://console.cloud.google.com/)
2. Click the project selector at the top of the page
3. Click **New Project**
4. Enter a project name (e.g., "zorai-plugins") and click **Create**

### 2. Enable the Gmail API

1. In the left sidebar, go to **APIs & Services > Library**
2. Search for **Gmail API**
3. Click on it and click **Enable**

### 3. Enable the Google Calendar API

1. In **APIs & Services > Library**, search for **Google Calendar API**
2. Click on it and click **Enable**

### 4. Configure the OAuth Consent Screen

1. Go to **APIs & Services > OAuth consent screen**
2. Select **External** user type and click **Create**
3. Fill in the required fields
4. On the **Scopes** page add:
   - `https://www.googleapis.com/auth/gmail.modify`
   - `https://www.googleapis.com/auth/gmail.send`
   - `https://www.googleapis.com/auth/gmail.compose`
   - `https://www.googleapis.com/auth/calendar`
5. Add your Google account as a test user while the app is in testing mode

**Important:** Gmail scopes remain restricted Google scopes. Testing mode is fine for personal/dev use; broader distribution requires Google OAuth verification.

### 5. Create OAuth Credentials

1. Go to **APIs & Services > Credentials**
2. Click **Create Credentials > OAuth client ID**
3. Select **Desktop app**
4. Record the **Client ID** and **Client Secret** for zorai plugin settings

## Installation

```bash
zorai plugin add zorai-plugin-gmail-calendar
```

This installs both the Gmail and Calendar plugins as separate connector entries.

## Readiness and setup model

Each connector now exposes:
- readiness probe (`check_health`)
- readiness state in TUI settings
- setup hint
- recovery hint
- docs path
- normalized workflow primitives
- read/write action inventory

### Gmail readiness expectations
- Settings: `client_id`, `client_secret`
- OAuth scopes: modify, send, compose
- Readiness probe confirms Gmail profile access

### Calendar readiness expectations
- Settings: `client_id`, `client_secret`, optional `default_calendar`
- OAuth scope: calendar
- Readiness probe confirms calendar list access

## Supported Gmail actions

### Read
- `/gmail.inbox`
- `/gmail.search`
- `/gmail.thread`
- thread/context primitives: `list_threads`, `fetch_thread_context`

### Write
- `/gmail.send`
- `/gmail.draft`
- `/gmail.reply`
- archive / trash / mark read/unread / star

### Gmail normalized primitives
- `list_threads`
- `fetch_thread_context`
- `draft_message`
- `reply_in_thread`
- `send_message`

## Supported Calendar actions

### Read
- `/calendar.today`
- `/calendar.week`
- `/calendar.prep`
- schedule/context primitives: `list_schedule_items`, `fetch_schedule_context`, `meeting_prep`

### Write
- `/calendar.create`
- `/calendar.update`
- `/calendar.delete`
- `/calendar.rsvp`

### Calendar normalized primitives
- `list_schedule_items`
- `fetch_schedule_context`
- `schedule_follow_up`
- `update_schedule_item`
- `update_attendance`
- `meeting_prep`

## Day-start triage example

1. Check Gmail and Calendar readiness in Settings → Plugins.
2. Use `/gmail.inbox` or `/gmail.search` to gather mail requiring action.
3. Use `/gmail.thread` or the normalized thread context primitive for full context.
4. Draft or reply using `/gmail.draft` and `/gmail.reply`.
5. Review today’s schedule with `/calendar.today`.
6. Use `/calendar.prep` for a meeting-prep summary.
7. Reschedule or RSVP with `/calendar.update` or `/calendar.rsvp`.

## Failure visibility

The readiness + enriched error layer now makes visible:
- missing setup / missing client credentials
- reconnect-needed or expired auth
- insufficient scopes
- rate limits
- unreachable Google service / timeout

Typical recoveries:
- missing setup → enter client credentials and connect
- insufficient scopes → reconnect and grant the listed scopes
- rate limit → retry later
- unreachable service → verify Google API availability and local network access

## Commands

| Command | Description |
|---------|-------------|
| `/gmail.inbox` | Show recent inbox messages |
| `/gmail.search` | Search emails by query |
| `/gmail.send` | Send an email |
| `/gmail.draft` | Create a Gmail draft |
| `/gmail.reply` | Reply in a Gmail thread |
| `/gmail.thread` | View a full email thread |
| `/calendar.today` | Show today's calendar events |
| `/calendar.week` | Show this week's calendar events |
| `/calendar.create` | Create a calendar event |
| `/calendar.update` | Update a calendar event |
| `/calendar.delete` | Delete a calendar event |
| `/calendar.rsvp` | Update attendance / response status |
| `/calendar.prep` | Show meeting-prep context |
