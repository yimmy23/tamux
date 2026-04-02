import type { ToolDefinition } from "./types";

export const GATEWAY_TOOLS: ToolDefinition[] = [
  {
    type: "function",
    function: {
      name: "send_slack_message",
      description: "Send a message to a Slack channel via the amux gateway.",
      parameters: {
        type: "object",
        properties: {
          channel: { type: "string", description: "Slack channel name or ID (e.g. 'general', 'C01234ABCDE')" },
          message: { type: "string", description: "The message text to send" },
        },
        required: ["channel", "message"],
      },
    },
  },
  {
    type: "function",
    function: {
      name: "send_discord_message",
      description: "Send a message to a Discord channel or user. If channel_id and user_id are both omitted, sends to the first channel configured in settings.",
      parameters: {
        type: "object",
        properties: {
          channel_id: { type: "string", description: "Discord channel ID to send to. Optional and falls back to the first configured channel." },
          user_id: { type: "string", description: "Discord user ID to DM. Optional and falls back to the first allowed user." },
          message: { type: "string", description: "The message text to send" },
        },
        required: ["message"],
      },
    },
  },
  {
    type: "function",
    function: {
      name: "send_telegram_message",
      description: "Send a message to a Telegram chat via the amux gateway.",
      parameters: {
        type: "object",
        properties: {
          chat_id: { type: "string", description: "Telegram chat ID" },
          message: { type: "string", description: "The message text to send" },
        },
        required: ["chat_id", "message"],
      },
    },
  },
  {
    type: "function",
    function: {
      name: "send_whatsapp_message",
      description: "Send a message to a WhatsApp contact via the amux gateway.",
      parameters: {
        type: "object",
        properties: {
          phone: { type: "string", description: "Phone number in E.164 format (e.g. '+1234567890') or WhatsApp JID" },
          message: { type: "string", description: "The message text to send" },
        },
        required: ["phone", "message"],
      },
    },
  },
];

export const TERMINAL_TOOLS: ToolDefinition[] = [
  {
    type: "function",
    function: {
      name: "list_terminals",
      description: "List all open terminal panes with their IDs. Use this to discover which terminals are available before running commands.",
      parameters: { type: "object", properties: {} },
    },
  },
  {
    type: "function",
    function: {
      name: "read_active_terminal_content",
      description: "Read the current terminal buffer content or browser panel info. By default reads the active pane; optionally target a pane by ID or pane name. For browser panels, returns URL and title; use include_dom to get page text content.",
      parameters: {
        type: "object",
        properties: {
          pane: { type: "string", description: "Optional pane ID or pane name to read from. If omitted, uses the active pane." },
          include_dom: { type: "boolean", description: "For browser panels: include page DOM text content. Ignored for terminal panes." },
        },
      },
    },
  },
  {
    type: "function",
    function: {
      name: "run_terminal_command",
      description: "Execute a shell command in a terminal pane and return its output. If pane/pane_id are omitted, uses the currently active pane.",
      parameters: {
        type: "object",
        properties: {
          command: { type: "string", description: "The shell command to execute" },
          pane: { type: "string", description: "Terminal pane ID or pane name. Optional and preferred over pane_id; defaults to active pane." },
          pane_id: { type: "string", description: "Legacy alias for pane. Terminal pane ID to run in. Optional and defaults to the active pane." },
        },
        required: ["command"],
      },
    },
  },
];

export const SYSTEM_TOOLS: ToolDefinition[] = [
  {
    type: "function",
    function: {
      name: "get_system_info",
      description: "Get current system information including CPU usage, memory, GPU stats, and top processes.",
      parameters: { type: "object", properties: {} },
    },
  },
  {
    type: "function",
    function: {
      name: "agent_query_memory",
      description: "Query Honcho cross-session memory for long-term user, workspace, or assistant context.",
      parameters: {
        type: "object",
        properties: {
          query: { type: "string", description: "Question to ask Honcho memory, for example 'What coding conventions does this workspace prefer?'" },
        },
        required: ["query"],
      },
    },
  },
];
