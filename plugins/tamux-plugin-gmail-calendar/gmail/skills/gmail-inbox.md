---
name: gmail
description: >
  Full Gmail integration — read inbox, search, send, trash, manage labels,
  view threads. Covers the two-step retrieval pattern and all write operations.
---

# Gmail Plugin

You have access to the **Gmail plugin** with these endpoints:

| Endpoint | Method | What it does |
|----------|--------|-------------|
| `list_inbox` | GET | List message IDs (two-step: list then get_message) |
| `get_message` | GET | Get message headers, snippet, labels |
| `get_message_full` | GET | Get full message with body parts |
| `search_messages` | GET | Search with Gmail query syntax |
| `send_message` | POST | Send email (raw_base64 RFC 2822) |
| `trash_message` | POST | Move message to trash |
| `untrash_message` | POST | Restore from trash |
| `modify_labels` | POST | Add/remove labels (mark read/unread, star, etc.) |
| `list_labels` | GET | List all Gmail labels |
| `get_thread` | GET | View full email thread |

## Reading Inbox (Two-Step Pattern)

Gmail's `list_inbox` returns only IDs. Fetch details with `get_message`:

```json
{"plugin_name": "gmail", "endpoint_name": "list_inbox", "params": {"max_results": 5}}
```
Then for each ID:
```json
{"plugin_name": "gmail", "endpoint_name": "get_message", "params": {"message_id": "ID_HERE"}}
```

Present as: **Subject** — From: sender — time ago / Preview: snippet...

## Searching

```json
{"plugin_name": "gmail", "endpoint_name": "search_messages", "params": {"query": "from:alice subject:meeting", "max_results": 10}}
```

Query syntax: `from:`, `subject:`, `is:unread`, `after:2026/03/01`, `has:attachment`, `label:important`

## Sending Email

The `send_message` endpoint requires a base64url-encoded RFC 2822 message in `raw_base64`. Build the message:

```
From: user@gmail.com
To: recipient@example.com
Subject: Hello
Content-Type: text/plain; charset=utf-8

Message body here
```

Base64url-encode it (no padding, URL-safe alphabet) and pass as `raw_base64`.

## Managing Messages

**Trash:** `{"plugin_name": "gmail", "endpoint_name": "trash_message", "params": {"message_id": "ID"}}`

**Mark as read:** `{"plugin_name": "gmail", "endpoint_name": "modify_labels", "params": {"message_id": "ID", "remove_labels": ["UNREAD"]}}`

**Star:** `{"plugin_name": "gmail", "endpoint_name": "modify_labels", "params": {"message_id": "ID", "add_labels": ["STARRED"]}}`

## Error Handling

If not connected: direct user to **Settings > Plugins > Gmail** to configure OAuth.
If auth error: token may have expired — daemon auto-refreshes, or ask user to reconnect.
