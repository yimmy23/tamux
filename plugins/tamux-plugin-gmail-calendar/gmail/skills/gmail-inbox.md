---
name: gmail-inbox
description: >
  How to check Gmail inbox and search emails using the Gmail plugin.
  Covers the two-step retrieval pattern (list message IDs, then fetch details)
  and search queries via the Gmail REST API.
---

# Gmail Inbox & Search

You have access to the **Gmail plugin** which lets you read the user's inbox
and search their emails. Gmail uses a **two-step retrieval pattern**: first
list message IDs, then fetch details for each message.

## Checking the Inbox

**Step 1: List message IDs**

Call `plugin_api_call` to get a list of message IDs from the inbox:

```json
{
  "tool": "plugin_api_call",
  "params": {
    "plugin_name": "gmail",
    "endpoint_name": "list_inbox",
    "params": {}
  }
}
```

This returns message IDs and an estimated count. The response includes a
header like "## Inbox (N messages)" using the `resultSizeEstimate` field
from the Gmail API. Preserve this count when presenting results to the user.

You can also pass optional parameters:

```json
{
  "tool": "plugin_api_call",
  "params": {
    "plugin_name": "gmail",
    "endpoint_name": "list_inbox",
    "params": {
      "max_results": 5,
      "query": "is:unread"
    }
  }
}
```

**Step 2: Fetch message details**

For each message ID returned, call `plugin_api_call` with `get_message`:

```json
{
  "tool": "plugin_api_call",
  "params": {
    "plugin_name": "gmail",
    "endpoint_name": "get_message",
    "params": {
      "message_id": "18e1a2b3c4d5e6f7"
    }
  }
}
```

This returns the message snippet, headers (Subject, From, Date), and labels.
Extract the Subject, From, and Date from the headers list to build a summary.

**Step 3: Present results**

Compile results into a numbered summary for the user:

1. **Subject line** -- From: sender -- 2 hours ago
   Preview of the message snippet...

2. **Another subject** -- From: another sender -- yesterday
   Preview of the snippet...

## Searching Emails

To search, call `search_messages` with a Gmail search query:

```json
{
  "tool": "plugin_api_call",
  "params": {
    "plugin_name": "gmail",
    "endpoint_name": "search_messages",
    "params": {
      "query": "from:alice@example.com subject:meeting",
      "max_results": 10
    }
  }
}
```

The `query` parameter uses the same syntax as the Gmail search box:
- `from:user@example.com` -- messages from a specific sender
- `subject:keyword` -- messages with keyword in subject
- `is:unread` -- unread messages only
- `after:2026/03/01` -- messages after a date
- `has:attachment` -- messages with attachments
- `label:important` -- messages with a specific label

After getting search results (message IDs), fetch details for each using
`get_message` as described above, then present a numbered summary with the
"## Search Results (N messages)" header from the response.

## Error Handling

If the Gmail plugin is not connected, tell the user:

> To use Gmail, connect your Google account in **Settings > Plugins > Gmail**.
> You will need to set up a Google Cloud project with the Gmail API enabled
> and configure OAuth credentials.

If a request fails with an authentication error, the OAuth token may have
expired. The daemon will attempt to refresh the token automatically. If
refresh also fails, ask the user to reconnect in Settings.
