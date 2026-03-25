---
name: calendar-today
description: >
  How to check today's Google Calendar events using the Calendar plugin.
  Covers RFC3339 date computation and the single-step events query.
---

# Calendar: Today's Events

You have access to the **Calendar plugin** which lets you list events from
the user's Google Calendar. Unlike Gmail, Calendar returns full event details
in a single API call.

## Listing Today's Events

**Step 1: Compute today's date boundaries**

Determine today's date boundaries in RFC3339 format. For example, if today
is 2026-03-25, compute:
- `time_min`: `2026-03-25T00:00:00Z` (start of day in UTC, or adjust for
  the user's timezone if known)
- `time_max`: `2026-03-26T00:00:00Z` (start of next day)

**Step 2: Call the calendar endpoint**

```json
{
  "tool": "plugin_api_call",
  "params": {
    "plugin_name": "calendar",
    "endpoint_name": "list_events_today",
    "params": {
      "time_min": "2026-03-25T00:00:00Z",
      "time_max": "2026-03-26T00:00:00Z"
    }
  }
}
```

You can optionally specify a different calendar:

```json
{
  "tool": "plugin_api_call",
  "params": {
    "plugin_name": "calendar",
    "endpoint_name": "list_events_today",
    "params": {
      "time_min": "2026-03-25T00:00:00Z",
      "time_max": "2026-03-26T00:00:00Z",
      "calendar_id": "work@group.calendar.google.com",
      "max_results": 50
    }
  }
}
```

**Step 3: Present results**

The API response template provides a "## Today's Calendar" header. Count
the events from the returned list and present with a count header:

## Today's Calendar (N events)

Format each event as a numbered list:

1. **9:00 AM - 10:00 AM** -- Team Standup
   Location: Conference Room A

2. **11:00 AM - 12:00 PM** -- Design Review
   Location: https://meet.google.com/abc-defg-hij

3. **2:00 PM - 3:30 PM** -- Sprint Planning
   Location: No location

For all-day events, the start/end fields use `start.date` and `end.date`
instead of `start.dateTime` and `end.dateTime`. The response template
handles this with the `default` helper, falling back from dateTime to date.

Include Google Meet or Zoom links when they appear in the location field.

If there are no events, tell the user their calendar is clear for the day.

## Error Handling

If the Calendar plugin is not connected, tell the user:

> To use Calendar, connect your Google account in **Settings > Plugins > Calendar**.
> You will need to set up a Google Cloud project with the Google Calendar API
> enabled and configure OAuth credentials.

If a request fails with an authentication error, the OAuth token may have
expired. The daemon will attempt to refresh the token automatically. If
refresh also fails, ask the user to reconnect in Settings.
