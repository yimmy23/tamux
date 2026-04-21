# tamux-plugin-gmail-calendar

Gmail and Google Calendar integration for tamux. Read, send, and manage emails. View, create, update, and delete calendar events -- all through natural conversation with your agent.

- **Gmail**: Inbox, search, send, trash, label management, thread view
- **Calendar**: List events, event details, create, update, delete, list calendars

## Prerequisites

- tamux v2.0 or later
- A Google account
- A Google Cloud project with OAuth credentials (see setup below)

## Google Cloud Console Setup

Follow these steps to create the OAuth credentials your plugin needs.

### 1. Create a Google Cloud Project

1. Go to [Google Cloud Console](https://console.cloud.google.com/)
2. Click the project selector at the top of the page
3. Click **New Project**
4. Enter a project name (e.g., "tamux-plugins") and click **Create**

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
3. Fill in the required fields:
   - **App name**: your choice (e.g., "tamux")
   - **User support email**: your email
   - **Developer contact email**: your email
4. Click **Save and Continue**
5. On the **Scopes** page, click **Add or Remove Scopes** and add:
   - `https://www.googleapis.com/auth/gmail.modify`
   - `https://www.googleapis.com/auth/gmail.send`
   - `https://www.googleapis.com/auth/calendar`
6. Click **Update**, then **Save and Continue**
7. On the **Test users** page, click **Add Users** and add your Google email address
8. Click **Save and Continue**, then **Back to Dashboard**

**Important:** The `gmail.modify` and `gmail.send` scopes are **restricted** Google scopes. While your OAuth consent screen is in **Testing** mode, only the test users you add (up to 100) can authorize the app. This is fine for personal and development use. Production distribution requires completing Google's OAuth verification review process.

### 5. Create OAuth Credentials

1. Go to **APIs & Services > Credentials**
2. Click **Create Credentials > OAuth client ID**
3. Select **Desktop app** as the application type
4. Enter a name (e.g., "tamux desktop")
5. Click **Create**
6. Note the **Client ID** and **Client Secret** -- you will need these in tamux

## Installation

```bash
tamux plugin add tamux-plugin-gmail-calendar
```

This installs both the Gmail and Calendar plugins. They appear as separate entries in your plugin list.

## Configuration

### In the Desktop App (Electron)

1. Open **Settings > Plugins**
2. You will see **Gmail** and **Calendar** listed
3. For each plugin:
   - Enter the **Client ID** and **Client Secret** from the OAuth credentials you created
   - Click **Connect** to authorize with your Google account
   - A browser window opens for Google's OAuth consent flow
   - After authorizing, the plugin status shows **Connected**

### In the TUI

1. Open Settings (press `S`) and navigate to the **Plugins** tab
2. Select **Gmail** or **Calendar**
3. Enter Client ID and Client Secret
4. Follow the authorization flow

## Usage

### Commands

| Command | Description |
|---------|-------------|
| `/gmail.inbox` | Show recent inbox messages |
| `/gmail.search` | Search emails by query |
| `/calendar.today` | Show today's calendar events |

### Natural Language

You can also ask naturally:

- "What's in my inbox?"
- "Show me unread emails"
- "Search for emails from alice@example.com about the project"
- "What's on my calendar today?"
- "Do I have any meetings this afternoon?"

The agent uses the plugin skills to translate your request into the appropriate API calls.

### Gmail Search Syntax

The search query supports the same syntax as the Gmail search box:

| Query | Description |
|-------|-------------|
| `from:user@example.com` | Messages from a sender |
| `subject:meeting` | Messages with keyword in subject |
| `is:unread` | Unread messages |
| `after:2026/03/01` | Messages after a date |
| `has:attachment` | Messages with attachments |
| `label:important` | Messages with a label |

You can combine queries: `from:alice subject:meeting after:2026/03/01`

## Scopes

This plugin requests **read-only** access:

| Scope | Access |
|-------|--------|
| `gmail.readonly` | Read messages and metadata (restricted scope) |
| `calendar.readonly` | Read calendar events |

No write access is requested -- the plugin cannot send emails, modify messages, or create calendar events.

The `gmail.readonly` scope is classified as **restricted** by Google. This means:
- In **Testing** mode, up to 100 test users can authorize the app
- For broader distribution, you must complete Google's [OAuth verification review](https://support.google.com/cloud/answer/9110914)
- The review process requires a privacy policy and may take several weeks

For personal or team use, Testing mode is sufficient.

## Creating Your Own Plugin

This plugin serves as a reference implementation for the tamux plugin system. To create your own plugin:

1. Create a directory with a `plugin.json` manifest declaring your API endpoints, auth, settings, and commands
   - API-backed commands use `commands.<name>.action`
   - Python-backed commands use `commands.<name>.python.command`
   - Shared Python defaults such as `run_path`, `source`, `env`, and `dependencies` can live under a top-level `python` object
2. Add YAML skill files in a `skills/` subdirectory to teach the agent how to use your API
3. Add a `package.json` with a `files` array listing your plugin directories
4. Publish to npm: `npm publish`
5. Install in tamux: `tamux plugin add your-plugin-name`

See the [tamux plugin documentation](https://github.com/anthropic/tamux/wiki/plugins) for the full manifest schema and API reference.
