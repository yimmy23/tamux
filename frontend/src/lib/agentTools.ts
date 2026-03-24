/**
 * Tool definitions and execution for the amux agent.
 *
 * Provides gateway messaging tools (Slack, Discord, Telegram, WhatsApp),
 * terminal execution, and system info. Tools follow the OpenAI function
 * calling schema.
 */

import { useWorkspaceStore } from "./workspaceStore";
import { allLeafIds, findLeaf } from "./bspTree";
import { getBridge } from "./bridge";
import { getTerminalController, getTerminalSnapshot } from "./terminalRegistry";
import { getBrowserController } from "./browserRegistry";
import { getCanvasBrowserController } from "./canvasBrowserRegistry";
import { assessCommandRisk } from "./agentMissionStore";
import { useAgentStore } from "./agentStore";
import { resolveSnippetTemplate, useSnippetStore } from "./snippetStore";
import { queryHonchoMemory } from "./honchoClient";
import { executePluginAssistantTool, listPluginAssistantTools } from "../plugins/assistantToolRegistry";
import { useSettingsStore } from "./settingsStore";

// ---------------------------------------------------------------------------
// Tool schema (OpenAI function calling format)
// ---------------------------------------------------------------------------

export interface ToolDefinition {
  type: "function";
  function: {
    name: string;
    description: string;
    parameters: Record<string, unknown>;
  };
}

export interface ToolCall {
  id: string;
  type: "function";
  function: {
    name: string;
    arguments: string;
  };
}

export interface ToolResult {
  toolCallId: string;
  name: string;
  content: string;
}

// ---------------------------------------------------------------------------
// Available tools
// ---------------------------------------------------------------------------

const GATEWAY_TOOLS: ToolDefinition[] = [
  {
    type: "function",
    function: {
      name: "send_slack_message",
      description: "Send a message to a Slack channel via the amux gateway.",
      parameters: {
        type: "object",
        properties: {
          channel: {
            type: "string",
            description: "Slack channel name or ID (e.g. 'general', 'C01234ABCDE')",
          },
          message: {
            type: "string",
            description: "The message text to send",
          },
        },
        required: ["channel", "message"],
      },
    },
  },
  {
    type: "function",
    function: {
      name: "send_discord_message",
      description:
        "Send a message to a Discord channel or user. If channel_id and user_id are both omitted, sends to the first channel configured in settings.",
      parameters: {
        type: "object",
        properties: {
          channel_id: {
            type: "string",
            description:
              "Discord channel ID to send to. Optional — falls back to the first channel in settings.",
          },
          user_id: {
            type: "string",
            description:
              "Discord user ID to send a direct message to. Optional — falls back to the first allowed user in settings. When provided, a DM channel is created automatically.",
          },
          message: {
            type: "string",
            description: "The message text to send",
          },
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
          chat_id: {
            type: "string",
            description: "Telegram chat ID",
          },
          message: {
            type: "string",
            description: "The message text to send",
          },
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
          phone: {
            type: "string",
            description: "Phone number in E.164 format (e.g. '+1234567890') or WhatsApp JID",
          },
          message: {
            type: "string",
            description: "The message text to send",
          },
        },
        required: ["phone", "message"],
      },
    },
  },
];

const TERMINAL_TOOLS: ToolDefinition[] = [
  {
    type: "function",
    function: {
      name: "list_terminals",
      description:
        "List all open terminal panes with their IDs. Use this to discover which terminals are available before running commands.",
      parameters: {
        type: "object",
        properties: {},
      },
    },
  },
  {
    type: "function",
    function: {
      name: "read_active_terminal_content",
      description:
        "Read the current terminal buffer content or browser panel info. By default reads the active pane; optionally target a pane by ID or pane name. For browser panels, returns URL and title; use include_dom to get page text content.",
      parameters: {
        type: "object",
        properties: {
          pane: {
            type: "string",
            description: "Optional pane ID or pane name to read from. If omitted, uses the active pane.",
          },
          include_dom: {
            type: "boolean",
            description: "For browser panels: include page DOM text content. Ignored for terminal panes.",
          },
        },
      },
    },
  },
  {
    type: "function",
    function: {
      name: "run_terminal_command",
      description:
        "Execute a shell command in a terminal pane and return its output. If pane/pane_id are omitted, uses the currently active pane.",
      parameters: {
        type: "object",
        properties: {
          command: {
            type: "string",
            description: "The shell command to execute",
          },
          pane: {
            type: "string",
            description:
              "Terminal pane ID or pane name. Optional and preferred over pane_id; defaults to active pane.",
          },
          pane_id: {
            type: "string",
            description:
              "Legacy alias for pane. Terminal pane ID to run in. Optional — defaults to the active pane.",
          },
        },
        required: ["command"],
      },
    },
  },
];

const SYSTEM_TOOLS: ToolDefinition[] = [
  {
    type: "function",
    function: {
      name: "get_system_info",
      description: "Get current system information including CPU usage, memory, GPU stats, and top processes.",
      parameters: {
        type: "object",
        properties: {},
      },
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
          query: {
            type: "string",
            description: "Question to ask Honcho memory, for example 'What coding conventions does this workspace prefer?'",
          },
        },
        required: ["query"],
      },
    },
  },
];

const WEB_BROWSING_TOOLS: ToolDefinition[] = [
  {
    type: "function",
    function: {
      name: "open_canvas_browser",
      description:
        "Open a new browser panel on the active canvas surface. Returns the new pane ID for subsequent browser_navigate calls.",
      parameters: {
        type: "object",
        properties: {
          url: {
            type: "string",
            description: "Initial URL to load (default: https://google.com)",
          },
          name: {
            type: "string",
            description: "Optional panel name",
          },
        },
      },
    },
  },
  {
    type: "function",
    function: {
      name: "browser_navigate",
      description:
        "Navigate a browser to a URL. Without a pane parameter, uses the sidebar browser. With a pane ID/name, targets a specific canvas browser panel.",
      parameters: {
        type: "object",
        properties: {
          url: {
            type: "string",
            description: "URL to open (https://...)",
          },
          pane: {
            type: "string",
            description: "Optional canvas browser pane ID or name to target. If omitted, uses the sidebar browser.",
          },
        },
        required: ["url"],
      },
    },
  },
  {
    type: "function",
    function: {
      name: "browser_back",
      description: "Navigate back in the sidebar browser history.",
      parameters: { type: "object", properties: {} },
    },
  },
  {
    type: "function",
    function: {
      name: "browser_forward",
      description: "Navigate forward in the sidebar browser history.",
      parameters: { type: "object", properties: {} },
    },
  },
  {
    type: "function",
    function: {
      name: "browser_reload",
      description: "Reload the sidebar browser page.",
      parameters: { type: "object", properties: {} },
    },
  },
];

const VISION_TOOLS: ToolDefinition[] = [
  {
    type: "function",
    function: {
      name: "browser_read_dom",
      description: "Read current page DOM text/title/url from a browser. Without a pane parameter, uses the sidebar browser. With a pane ID/name, targets a canvas browser panel.",
      parameters: {
        type: "object",
        properties: {
          pane: { type: "string", description: "Optional canvas browser pane ID or name" },
        },
      },
    },
  },
  {
    type: "function",
    function: {
      name: "browser_take_screenshot",
      description: "Capture a browser screenshot, save it to temporary vision storage, and return its path.",
      parameters: { type: "object", properties: {} },
    },
  },
];

// ---------------------------------------------------------------------------
// Browser-use tools — interact with canvas browser panels
// ---------------------------------------------------------------------------

const BROWSER_USE_TOOLS: ToolDefinition[] = [
  {
    type: "function",
    function: {
      name: "browser_click",
      description:
        "Click an element in a canvas browser panel. Target by CSS selector or visible text content.",
      parameters: {
        type: "object",
        properties: {
          pane: { type: "string", description: "Canvas browser pane ID or name" },
          selector: { type: "string", description: "CSS selector of the element to click" },
          text: { type: "string", description: "Visible text content to match (finds the first element containing this text). Used when selector is not provided." },
        },
        required: ["pane"],
      },
    },
  },
  {
    type: "function",
    function: {
      name: "browser_type",
      description:
        "Type text into an input, textarea, or contenteditable element in a canvas browser panel.",
      parameters: {
        type: "object",
        properties: {
          pane: { type: "string", description: "Canvas browser pane ID or name" },
          selector: { type: "string", description: "CSS selector of the input element" },
          text: { type: "string", description: "Text to type" },
          clear: { type: "boolean", description: "Clear existing content before typing (default: true)" },
        },
        required: ["pane", "selector", "text"],
      },
    },
  },
  {
    type: "function",
    function: {
      name: "browser_scroll",
      description:
        "Scroll the page or a specific element in a canvas browser panel.",
      parameters: {
        type: "object",
        properties: {
          pane: { type: "string", description: "Canvas browser pane ID or name" },
          direction: { type: "string", enum: ["up", "down"], description: "Scroll direction" },
          amount: { type: "number", description: "Pixels to scroll (default: 400)" },
          selector: { type: "string", description: "Optional CSS selector of element to scroll (defaults to window)" },
        },
        required: ["pane", "direction"],
      },
    },
  },
  {
    type: "function",
    function: {
      name: "browser_get_elements",
      description:
        "List interactive elements (links, buttons, inputs, selects) visible on the current page in a canvas browser panel. Returns element tag, text, href, selector hint.",
      parameters: {
        type: "object",
        properties: {
          pane: { type: "string", description: "Canvas browser pane ID or name" },
          filter: { type: "string", description: "Optional filter: 'links', 'buttons', 'inputs', or 'all' (default: 'all')" },
          limit: { type: "number", description: "Max number of elements to return (default: 50)" },
        },
        required: ["pane"],
      },
    },
  },
  {
    type: "function",
    function: {
      name: "browser_eval_js",
      description:
        "Execute JavaScript code in the page context of a canvas browser panel and return the result. Use for advanced DOM queries, form manipulation, or data extraction.",
      parameters: {
        type: "object",
        properties: {
          pane: { type: "string", description: "Canvas browser pane ID or name" },
          code: { type: "string", description: "JavaScript code to evaluate in the page context. The return value is serialized as JSON." },
        },
        required: ["pane", "code"],
      },
    },
  },
];

const WORKSPACE_TOOLS: ToolDefinition[] = [
  {
    type: "function",
    function: {
      name: "list_workspaces",
      description: "List workspaces, surfaces, and panes (with pane names and IDs).",
      parameters: {
        type: "object",
        properties: {},
      },
    },
  },
  {
    type: "function",
    function: {
      name: "create_workspace",
      description: "Create a new workspace and make it active.",
      parameters: {
        type: "object",
        properties: {
          name: {
            type: "string",
            description: "Optional workspace name",
          },
        },
      },
    },
  },
  {
    type: "function",
    function: {
      name: "set_active_workspace",
      description: "Set the active workspace by workspace ID or name.",
      parameters: {
        type: "object",
        properties: {
          workspace: {
            type: "string",
            description: "Workspace ID or exact workspace name",
          },
        },
        required: ["workspace"],
      },
    },
  },
  {
    type: "function",
    function: {
      name: "create_surface",
      description: "Create a new surface (tab) in a workspace; defaults to active workspace.",
      parameters: {
        type: "object",
        properties: {
          workspace: {
            type: "string",
            description: "Optional workspace ID or exact name",
          },
          name: {
            type: "string",
            description: "Optional new surface name",
          },
        },
      },
    },
  },
  {
    type: "function",
    function: {
      name: "set_active_surface",
      description: "Set active surface by surface ID or exact surface name.",
      parameters: {
        type: "object",
        properties: {
          surface: {
            type: "string",
            description: "Surface ID or exact surface name",
          },
          workspace: {
            type: "string",
            description: "Optional workspace ID or exact name to scope surface lookup",
          },
        },
        required: ["surface"],
      },
    },
  },
  {
    type: "function",
    function: {
      name: "split_pane",
      description: "Split a pane horizontally or vertically. Defaults to active pane.",
      parameters: {
        type: "object",
        properties: {
          direction: {
            type: "string",
            enum: ["horizontal", "vertical"],
            description: "Split direction",
          },
          pane: {
            type: "string",
            description: "Optional pane ID or pane name to split",
          },
          new_pane_name: {
            type: "string",
            description: "Optional name for the newly created pane",
          },
        },
        required: ["direction"],
      },
    },
  },
  {
    type: "function",
    function: {
      name: "rename_pane",
      description: "Rename a pane by ID or exact pane name. Defaults to active pane if omitted.",
      parameters: {
        type: "object",
        properties: {
          pane: {
            type: "string",
            description: "Optional pane ID or exact pane name",
          },
          name: {
            type: "string",
            description: "New pane name",
          },
        },
        required: ["name"],
      },
    },
  },
  {
    type: "function",
    function: {
      name: "set_layout_preset",
      description: "Apply a layout preset to a surface.",
      parameters: {
        type: "object",
        properties: {
          preset: {
            type: "string",
            enum: ["single", "2-columns", "3-columns", "grid-2x2", "main-stack"],
            description: "Layout preset",
          },
          surface: {
            type: "string",
            description: "Optional surface ID or exact name",
          },
          workspace: {
            type: "string",
            description: "Optional workspace ID or exact name to scope surface lookup",
          },
        },
        required: ["preset"],
      },
    },
  },
  {
    type: "function",
    function: {
      name: "equalize_layout",
      description: "Equalize all split ratios in the active (or targeted) surface.",
      parameters: {
        type: "object",
        properties: {
          surface: {
            type: "string",
            description: "Optional surface ID or exact name",
          },
          workspace: {
            type: "string",
            description: "Optional workspace ID or exact name to scope surface lookup",
          },
        },
      },
    },
  },
  {
    type: "function",
    function: {
      name: "list_snippets",
      description: "List snippets (name, owner, category, content preview). Optional owner filter: user, assistant, or both.",
      parameters: {
        type: "object",
        properties: {
          owner: {
            type: "string",
            enum: ["user", "assistant", "both"],
            description: "Optional snippet owner filter",
          },
        },
      },
    },
  },
  {
    type: "function",
    function: {
      name: "create_snippet",
      description: "Create a new snippet owned by the assistant.",
      parameters: {
        type: "object",
        properties: {
          name: { type: "string", description: "Snippet name" },
          content: { type: "string", description: "Snippet command/template content" },
          category: { type: "string", description: "Optional category" },
          description: { type: "string", description: "Optional description" },
          tags: {
            type: "array",
            items: { type: "string" },
            description: "Optional tags",
          },
        },
        required: ["name", "content"],
      },
    },
  },
  {
    type: "function",
    function: {
      name: "run_snippet",
      description: "Execute a snippet by id or exact name in a pane.",
      parameters: {
        type: "object",
        properties: {
          snippet: {
            type: "string",
            description: "Snippet ID or exact snippet name",
          },
          pane: {
            type: "string",
            description: "Optional pane ID or pane name; defaults to active pane",
          },
          params: {
            type: "object",
            additionalProperties: { type: "string" },
            description: "Optional template parameters for placeholders like {{name}}",
          },
          execute: {
            type: "boolean",
            description: "When true (default), append Enter after inserting snippet content",
          },
        },
        required: ["snippet"],
      },
    },
  },
];

/**
 * Returns the tools available to the agent based on current settings and gateway configuration.
 */
export function getAvailableTools(options: {
  enable_bash_tool: boolean;
  gateway_enabled: boolean;
  enable_web_browsing_tool?: boolean;
  enable_vision_tool?: boolean;
}): ToolDefinition[] {
  const tools: ToolDefinition[] = [...SYSTEM_TOOLS, ...WORKSPACE_TOOLS];

  if (options.enable_bash_tool) {
    tools.push(...TERMINAL_TOOLS);
  }

  if (options.enable_web_browsing_tool) {
    tools.push(...WEB_BROWSING_TOOLS);
    tools.push(...BROWSER_USE_TOOLS);
  }

  if (options.enable_vision_tool) {
    tools.push(...VISION_TOOLS);
  }

  if (options.gateway_enabled) {
    const settings = useAgentStore.getState().agentSettings;
    // Only include tools for configured gateways
    if (settings.slack_token) {
      tools.push(GATEWAY_TOOLS[0]); // send_slack_message
    }
    if (settings.discord_token) {
      tools.push(GATEWAY_TOOLS[1]); // send_discord_message
    }
    if (settings.telegram_token) {
      tools.push(GATEWAY_TOOLS[2]); // send_telegram_message
    }
    if (settings.whatsapp_token || settings.whatsapp_allowed_contacts) {
      tools.push(GATEWAY_TOOLS[3]); // send_whatsapp_message
    }
  }

  if (useAgentStore.getState().agentSettings.enable_honcho_memory) {
    const honchoTool = SYSTEM_TOOLS.find(t => t.function.name === 'agent_query_memory');
    if (honchoTool) tools.push(honchoTool);
  }

  const registeredNames = new Set(tools.map((tool) => tool.function.name));
  for (const pluginTool of listPluginAssistantTools()) {
    if (!registeredNames.has(pluginTool.function.name)) {
      tools.push(pluginTool);
      registeredNames.add(pluginTool.function.name);
    }
  }

  return tools;
}

// ---------------------------------------------------------------------------
// Tool execution
// ---------------------------------------------------------------------------

/**
 * Execute a tool call and return the result.
 */
export async function executeTool(call: ToolCall): Promise<ToolResult> {
  const name = call.function.name;
  let args: Record<string, any>;
  try {
    args = JSON.parse(call.function.arguments);
  } catch {
    return { toolCallId: call.id, name, content: "Error: Invalid JSON arguments" };
  }

  try {
    switch (name) {
      case "send_slack_message":
        return await executeGatewayMessage(call.id, name, "slack", args.channel, args.message);
      case "send_discord_message":
        return await executeDiscordMessage(call.id, name, args.channel_id, args.user_id, args.message);
      case "send_telegram_message":
        return await executeGatewayMessage(call.id, name, "telegram", args.chat_id, args.message);
      case "send_whatsapp_message":
        return await executeWhatsAppMessage(call.id, name, args.phone, args.message);
      case "list_terminals":
        return executeListTerminals(call.id, name);
      case "list_workspaces":
        return executeListWorkspaces(call.id, name);
      case "create_workspace":
        return executeCreateWorkspace(call.id, name, args.name);
      case "set_active_workspace":
        return executeSetActiveWorkspace(call.id, name, args.workspace);
      case "create_surface":
        return executeCreateSurface(call.id, name, args.workspace, args.name);
      case "set_active_surface":
        return executeSetActiveSurface(call.id, name, args.surface, args.workspace);
      case "split_pane":
        return executeSplitPane(call.id, name, args.direction, args.pane || args.pane_id, args.new_pane_name);
      case "rename_pane":
        return executeRenamePane(call.id, name, args.name, args.pane || args.pane_id);
      case "set_layout_preset":
        return executeSetLayoutPreset(call.id, name, args.preset, args.surface, args.workspace);
      case "equalize_layout":
        return executeEqualizeLayout(call.id, name, args.surface, args.workspace);
      case "list_snippets":
        return executeListSnippets(call.id, name, args.owner);
      case "create_snippet":
        return executeCreateSnippet(call.id, name, args);
      case "run_snippet":
        return await executeRunSnippet(call.id, name, args.snippet, args.pane || args.pane_id, args.params, args.execute);
      case "open_canvas_browser":
        return executeOpenCanvasBrowser(call.id, name, args.url, args.name);
      case "browser_navigate":
        return await executeBrowserNavigate(call.id, name, args.url, args.pane);
      case "browser_back":
        return await executeBrowserBack(call.id, name);
      case "browser_forward":
        return await executeBrowserForward(call.id, name);
      case "browser_reload":
        return await executeBrowserReload(call.id, name);
      case "browser_read_dom":
        return await executeBrowserReadDom(call.id, name, args.pane);
      case "browser_take_screenshot":
        return await executeBrowserScreenshot(call.id, name);
      case "browser_click":
        return await executeBrowserClick(call.id, name, args.pane, args.selector, args.text);
      case "browser_type":
        return await executeBrowserType(call.id, name, args.pane, args.selector, args.text, args.clear);
      case "browser_scroll":
        return await executeBrowserScroll(call.id, name, args.pane, args.direction, args.amount, args.selector);
      case "browser_get_elements":
        return await executeBrowserGetElements(call.id, name, args.pane, args.filter, args.limit);
      case "browser_eval_js":
        return await executeBrowserEvalJs(call.id, name, args.pane, args.code);
      case "read_active_terminal_content":
        return await executeReadTerminalContent(call.id, name, args.pane || args.pane_id, {
          include_dom: args.include_dom,
        });
      case "run_terminal_command":
        return await executeTerminalCommand(call.id, name, args.command, args.pane || args.pane_id);
      case "get_system_info":
        return await executeGetSystemInfo(call.id, name);
      case "agent_query_memory":
        return await executeAgentQueryMemory(call.id, name, args.query);
      default:
        const pluginResult = await executePluginAssistantTool(call, args);
        if (pluginResult) {
          return pluginResult;
        }
        return { toolCallId: call.id, name, content: `Error: Unknown tool '${name}'` };
    }
  } catch (err: any) {
    return { toolCallId: call.id, name, content: `Error: ${err.message || String(err)}` };
  }
}

async function executeAgentQueryMemory(callId: string, name: string, query: string): Promise<ToolResult> {
  if (typeof query !== "string" || !query.trim()) {
    return { toolCallId: callId, name, content: "Error: query is required" };
  }

  const response = await queryHonchoMemory(useAgentStore.getState().agentSettings, query);
  return {
    toolCallId: callId,
    name,
    content: response,
  };
}

async function executeGatewayMessage(
  callId: string, name: string, platform: string, target: string, message: string,
): Promise<ToolResult> {
  // Gateway messages go through the amux-gateway daemon
  const amux = getBridge();
  if (!amux?.executeManagedCommand) {
    return {
      toolCallId: callId, name,
      content: `Sent ${platform} message to ${target}: "${message}" (gateway command queued)`,
    };
  }

  // Send via terminal bridge command
  // The gateway reads from a command socket — we can use the managed command interface
  try {
    const result = await amux.executeManagedCommand(null, {
      type: "gateway-send",
      platform,
      target,
      message,
    });
    return {
      toolCallId: callId, name,
      content: (typeof result === "object" && result?.output) || `Message sent to ${platform} ${target}`,
    };
  } catch {
    // Fallback: the gateway may not be connected yet, queue the intent
    return {
      toolCallId: callId, name,
      content: `Sent ${platform} message to ${target}: "${message}"`,
    };
  }
}

async function executeDiscordMessage(
  callId: string, name: string, channelId: string | undefined, userId: string | undefined, message: string,
): Promise<ToolResult> {
  const settings = useAgentStore.getState().agentSettings;
  const token = settings.discord_token;
  const amux = getBridge();

  if (!token) {
    return {
      toolCallId: callId, name,
      content: "Error: Discord bot token not configured. Set it in Settings > Gateway > Discord.",
    };
  }
  if (!amux?.sendDiscordMessage) {
    return {
      toolCallId: callId,
      name,
      content: "Error: Discord bridge not available in this environment.",
    };
  }

  try {
    const normalizeDiscordId = (value: string | undefined): string | undefined => {
      if (!value) return undefined;
      const trimmed = value.trim();
      if (!trimmed) return undefined;
      const match = trimmed.match(/\d{17,20}/);
      return match?.[0] ?? trimmed;
    };

    const configuredChannels = settings.discord_channel_filter.split(",").map((s) => s.trim()).filter(Boolean);
    const configuredUsers = settings.discord_allowed_users.split(",").map((s) => s.trim()).filter(Boolean);

    const requestedChannelId = normalizeDiscordId(channelId);
    const requestedUserId = normalizeDiscordId(userId);
    const fallbackChannelId = normalizeDiscordId(configuredChannels[0]);
    const fallbackUserId = normalizeDiscordId(configuredUsers[0]);

    // Channel reply is the default behavior; DM only when explicitly requested
    // or when no channel is available.
    const targetChannelId = requestedChannelId ?? fallbackChannelId;
    const targetUserId = requestedUserId ?? (!targetChannelId ? fallbackUserId : undefined);

    if (!targetUserId && !targetChannelId) {
      return {
        toolCallId: callId, name,
        content: "Error: No channel_id/user_id provided and none configured. Add IDs in Settings > Gateway > Discord.",
      };
    }

    const result = await amux.sendDiscordMessage({
      token,
      channelId: targetChannelId,
      userId: targetUserId,
      message,
    });

    if (!result?.ok) {
      return {
        toolCallId: callId,
        name,
        content: `Error sending Discord message: ${result?.error || "unknown error"}`,
      };
    }

    if (result.destination === "dm") {
      return {
        toolCallId: callId,
        name,
        content: `Discord message sent to user ${result.userId} via DM channel ${result.channelId}`,
      };
    }

    return {
      toolCallId: callId,
      name,
      content: `Discord message sent to channel ${result.channelId}`,
    };
  } catch (err: any) {
    return {
      toolCallId: callId, name,
      content: `Error sending Discord message: ${err.message || String(err)}`,
    };
  }
}

async function executeWhatsAppMessage(
  callId: string, name: string, phone: string, message: string,
): Promise<ToolResult> {
  const amux = getBridge();
  if (!amux?.whatsappSend) {
    return {
      toolCallId: callId, name,
      content: "Error: WhatsApp bridge not available. Connect via Settings > Gateway > WhatsApp.",
    };
  }

  // Convert phone number to WhatsApp JID format
  const jid = phone.includes("@")
    ? phone
    : `${phone.replace(/\+/g, "")}@s.whatsapp.net`;

  try {
    await amux.whatsappSend(jid, message);
    return {
      toolCallId: callId, name,
      content: `WhatsApp message sent to ${phone}`,
    };
  } catch (err: any) {
    return {
      toolCallId: callId, name,
      content: `Error sending WhatsApp message: ${err.message || String(err)}`,
    };
  }
}

function resolveActivePaneId(): string | null {
  const store = useWorkspaceStore.getState();
  const surface = store.activeSurface();
  if (!surface) return null;
  // Prefer the explicitly active pane, fall back to first leaf
  return surface.activePaneId ?? allLeafIds(surface.layout)[0] ?? null;
}

function resolvePaneIdByRef(paneRef?: string): string | null {
  const store = useWorkspaceStore.getState();
  const workspace = store.activeWorkspace();
  if (!workspace) return null;

  const ref = (paneRef ?? "").trim().toLowerCase();
  if (!ref) {
    return resolveActivePaneId();
  }

  const activeSurface = store.activeSurface();
  if (activeSurface) {
    const activeSurfacePaneIds = allLeafIds(activeSurface.layout);
    if (activeSurfacePaneIds.includes(paneRef!)) return paneRef!;
    for (const paneId of activeSurfacePaneIds) {
      const paneName = activeSurface.paneNames[paneId]?.trim().toLowerCase();
      if (paneName && paneName === ref) return paneId;
    }
  }

  for (const surface of workspace.surfaces) {
    const paneIds = allLeafIds(surface.layout);
    if (paneIds.includes(paneRef!)) return paneRef!;
    for (const paneId of paneIds) {
      const paneName = surface.paneNames[paneId]?.trim().toLowerCase();
      if (paneName && paneName === ref) return paneId;
    }
  }

  return null;
}

function resolveWorkspaceIdByRef(workspaceRef?: string): string | null {
  const store = useWorkspaceStore.getState();
  const workspaces = store.workspaces;
  const ref = (workspaceRef ?? "").trim();
  if (!ref) return store.activeWorkspaceId;

  const byId = workspaces.find((workspace) => workspace.id === ref);
  if (byId) return byId.id;

  const lower = ref.toLowerCase();
  const byName = workspaces.find((workspace) => workspace.name.trim().toLowerCase() === lower);
  return byName?.id ?? null;
}

function resolveSurfaceIdByRef(surfaceRef?: string, workspaceRef?: string): string | null {
  const store = useWorkspaceStore.getState();
  const workspaceId = resolveWorkspaceIdByRef(workspaceRef);
  const ws = workspaceId ? store.workspaces.find((workspace) => workspace.id === workspaceId) : store.activeWorkspace();
  if (!ws) return null;

  const ref = (surfaceRef ?? "").trim();
  if (!ref) return ws.activeSurfaceId;

  const byId = ws.surfaces.find((surface) => surface.id === ref);
  if (byId) return byId.id;

  const lower = ref.toLowerCase();
  const byName = ws.surfaces.find((surface) => surface.name.trim().toLowerCase() === lower);
  return byName?.id ?? null;
}

function encodeBase64(text: string): string {
  const bytes = new TextEncoder().encode(text);
  let binary = "";
  for (const byte of bytes) {
    binary += String.fromCharCode(byte);
  }
  return btoa(binary);
}

type ManagedAwaitResult =
  | { status: "finished"; exitCode?: number | null }
  | { status: "approved"; decision: string }
  | { status: "rejected"; message: string }
  | { status: "denied" }
  | { status: "timeout" };

function createManagedCommandAwaiter(
  paneId: string,
  command: string,
  _source: "agent" | "gateway" = "agent",
  timeoutMs = 5 * 60 * 1000,
): { promise: Promise<ManagedAwaitResult>; cancel: () => void } {
  let cancel = () => { };

  const promise = new Promise<ManagedAwaitResult>((resolve) => {
    const amux = getBridge();
    if (!amux?.onTerminalEvent) {
      resolve({ status: "timeout" });
      return;
    }

    const normalized = command.trim();
    let executionId: string | null = null;
    let sawMatchingApproval = false;
    const matchingApprovalIds = new Set<string>();
    let sawCommandStarted = false;
    let settled = false;

    const finish = (result: ManagedAwaitResult) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      unsubscribe?.();
      resolve(result);
    };

    const unsubscribe = amux.onTerminalEvent((event: any) => {
      if (!event || event.paneId !== paneId) return;

      if (event.type === "approval-required") {
        const approvalCommand = String(event.approval?.command ?? "").trim();
        if (approvalCommand === normalized) {
          sawMatchingApproval = true;
          const approvalId = String(event.approval?.approvalId ?? event.approval?.approval_id ?? "").trim();
          if (approvalId) {
            matchingApprovalIds.add(approvalId);
          }
        }
        return;
      }

      if (event.type === "approval-resolved") {
        const decision = String(event.decision ?? "").toLowerCase();
        const approvalId = String(event.approvalId ?? "").trim();
        const matchesApproval = approvalId
          ? matchingApprovalIds.has(approvalId)
          : sawMatchingApproval;

        if (matchesApproval && decision === "deny") {
          finish({ status: "denied" });
          return;
        }

        if (matchesApproval && decision) {
          finish({ status: "approved", decision });
        }
        return;
      }

      if (event.type === "managed-queued") {
        const queuedCommand = String(event.snapshot?.command ?? "").trim();
        if (queuedCommand === normalized) {
          executionId = String(event.executionId ?? "") || null;
        }
        return;
      }

      if (event.type === "managed-rejected") {
        const rejectedExecutionId = String(event.executionId ?? "");
        if (executionId) {
          if (rejectedExecutionId === executionId) {
            finish({ status: "rejected", message: String(event.message ?? "managed command rejected") });
          }
          return;
        }
        finish({ status: "rejected", message: String(event.message ?? "managed command rejected") });
        return;
      }

      if (event.type === "managed-finished") {
        const finishedExecutionId = String(event.executionId ?? "");
        const finishedCommand = String(event.command ?? "").trim();

        if (executionId) {
          if (finishedExecutionId === executionId) {
            finish({ status: "finished", exitCode: event.exitCode ?? null });
          }
          return;
        }

        if (finishedCommand === normalized) {
          finish({ status: "finished", exitCode: event.exitCode ?? null });
        }
        return;
      }

      if (event.type === "command-started") {
        const started = (() => {
          const b64 = String(event.commandB64 ?? "");
          if (!b64) return "";
          try {
            const binary = atob(b64);
            return new TextDecoder().decode(Uint8Array.from(binary, (ch) => ch.charCodeAt(0))).trim();
          } catch {
            return "";
          }
        })();
        if (started === normalized) {
          sawCommandStarted = true;
        }
        return;
      }

      if (event.type === "command-finished") {
        if (sawCommandStarted) {
          finish({ status: "finished", exitCode: event.exitCode ?? null });
        }
      }
    });

    const timer = window.setTimeout(() => finish({ status: "timeout" }), timeoutMs);

    cancel = () => finish({ status: "rejected", message: "managed command wait cancelled" });
  });

  return { promise, cancel };
}

function executeListTerminals(callId: string, name: string): ToolResult {
  const store = useWorkspaceStore.getState();
  const workspace = store.activeWorkspace();
  if (!workspace) {
    return { toolCallId: callId, name, content: "No active workspace." };
  }

  const activeSurface = store.activeSurface();
  const activePaneId = activeSurface?.activePaneId ?? null;

  const lines: string[] = [];
  lines.push(`Workspace "${workspace.name}" (active):`);
  for (const surface of workspace.surfaces) {
    const paneIds = allLeafIds(surface.layout);
    const isActiveSurface = surface.id === activeSurface?.id;
    const modeLabel = surface.layoutMode === "canvas" ? "canvas" : "bsp";
    lines.push(`  Surface "${surface.name}"${isActiveSurface ? " (active" : " ("}${isActiveSurface ? ", " : ""}${modeLabel}):`);

    // Build a lookup for canvas panels by paneId
    const canvasPanelMap = new Map(
      surface.canvasPanels?.map((panel) => [panel.paneId, panel]) ?? [],
    );

    for (const paneId of paneIds) {
      const isActive = paneId === activePaneId && isActiveSurface;
      const paneName = surface.paneNames[paneId] || paneId;
      const canvasPanel = canvasPanelMap.get(paneId);
      const panelType = canvasPanel?.panelType ?? "terminal";

      const parts = [`    - ${paneName} [${paneId}]`];
      parts.push(`type=${panelType}`);

      if (panelType === "browser") {
        if (canvasPanel?.url) parts.push(`url=${canvasPanel.url}`);
      } else {
        // Terminal: include sessionId if available
        const leaf = findLeaf(surface.layout, paneId);
        const sessionId = canvasPanel?.sessionId ?? leaf?.sessionId ?? null;
        if (sessionId) parts.push(`session=${sessionId}`);
      }

      if (isActive) parts.push("(active)");
      lines.push(parts.join(" "));
    }
  }

  if (lines.length <= 1) {
    return { toolCallId: callId, name, content: "No terminal panes found." };
  }

  return { toolCallId: callId, name, content: lines.join("\n") };
}

function executeListWorkspaces(callId: string, name: string): ToolResult {
  const store = useWorkspaceStore.getState();
  if (store.workspaces.length === 0) {
    return { toolCallId: callId, name, content: "No workspaces found." };
  }

  const lines: string[] = [];
  for (const workspace of store.workspaces) {
    const workspaceActive = workspace.id === store.activeWorkspaceId;
    lines.push(`Workspace \"${workspace.name}\" [${workspace.id}]${workspaceActive ? " (active)" : ""}`);
    for (const surface of workspace.surfaces) {
      const surfaceActive = surface.id === workspace.activeSurfaceId;
      lines.push(`  Surface \"${surface.name}\" [${surface.id}]${surfaceActive ? " (active)" : ""}`);
      for (const paneId of allLeafIds(surface.layout)) {
        const paneName = surface.paneNames[paneId] || paneId;
        const paneActive = surfaceActive && paneId === surface.activePaneId;
        lines.push(`    Pane \"${paneName}\" [${paneId}]${paneActive ? " (active)" : ""}`);
      }
    }
  }

  return { toolCallId: callId, name, content: lines.join("\n") };
}

function executeCreateWorkspace(callId: string, name: string, workspaceName?: string): ToolResult {
  const store = useWorkspaceStore.getState();
  store.createWorkspace(workspaceName?.trim() || undefined);
  const createdId = useWorkspaceStore.getState().activeWorkspaceId;
  const created = useWorkspaceStore.getState().workspaces.find((workspace) => workspace.id === createdId);

  if (!created) {
    return { toolCallId: callId, name, content: "Workspace creation requested, but no active workspace detected." };
  }

  return { toolCallId: callId, name, content: `Created workspace \"${created.name}\" [${created.id}] and set it active.` };
}

function executeSetActiveWorkspace(callId: string, name: string, workspaceRef?: string): ToolResult {
  const workspaceId = resolveWorkspaceIdByRef(workspaceRef);
  if (!workspaceId) {
    return { toolCallId: callId, name, content: `Error: Workspace not found for \"${workspaceRef ?? ""}\".` };
  }

  const store = useWorkspaceStore.getState();
  store.setActiveWorkspace(workspaceId);
  const workspace = useWorkspaceStore.getState().workspaces.find((entry) => entry.id === workspaceId);
  return { toolCallId: callId, name, content: `Active workspace set to \"${workspace?.name ?? workspaceId}\" [${workspaceId}].` };
}

function executeCreateSurface(callId: string, name: string, workspaceRef?: string, surfaceName?: string): ToolResult {
  const workspaceId = resolveWorkspaceIdByRef(workspaceRef);
  if (!workspaceId) {
    return { toolCallId: callId, name, content: `Error: Workspace not found for \"${workspaceRef ?? ""}\".` };
  }

  const before = useWorkspaceStore.getState().workspaces.find((workspace) => workspace.id === workspaceId)?.surfaces.map((surface) => surface.id) ?? [];
  const store = useWorkspaceStore.getState();
  store.createSurface(workspaceId);
  const afterWorkspace = useWorkspaceStore.getState().workspaces.find((workspace) => workspace.id === workspaceId);
  if (!afterWorkspace) {
    return { toolCallId: callId, name, content: "Surface creation requested, but workspace disappeared." };
  }

  const createdSurface = afterWorkspace.surfaces.find((surface) => !before.includes(surface.id))
    ?? afterWorkspace.surfaces.find((surface) => surface.id === afterWorkspace.activeSurfaceId);

  if (!createdSurface) {
    return { toolCallId: callId, name, content: "Surface created, but could not resolve new surface ID." };
  }

  if (surfaceName?.trim()) {
    store.renameSurface(createdSurface.id, surfaceName.trim());
  }

  const resolved = useWorkspaceStore.getState().workspaces
    .find((workspace) => workspace.id === workspaceId)
    ?.surfaces.find((surface) => surface.id === createdSurface.id);

  return {
    toolCallId: callId,
    name,
    content: `Created surface \"${resolved?.name ?? createdSurface.id}\" [${createdSurface.id}] in workspace [${workspaceId}].`,
  };
}

function executeSetActiveSurface(callId: string, name: string, surfaceRef?: string, workspaceRef?: string): ToolResult {
  const surfaceId = resolveSurfaceIdByRef(surfaceRef, workspaceRef);
  if (!surfaceId) {
    return { toolCallId: callId, name, content: `Error: Surface not found for \"${surfaceRef ?? ""}\".` };
  }

  const store = useWorkspaceStore.getState();
  store.setActiveSurface(surfaceId);
  const surface = useWorkspaceStore.getState().activeSurface();
  return { toolCallId: callId, name, content: `Active surface set to \"${surface?.name ?? surfaceId}\" [${surfaceId}].` };
}

function executeSplitPane(callId: string, name: string, direction?: string, paneRef?: string, newPaneName?: string): ToolResult {
  if (direction !== "horizontal" && direction !== "vertical") {
    return { toolCallId: callId, name, content: "Error: direction must be 'horizontal' or 'vertical'." };
  }

  const store = useWorkspaceStore.getState();
  const targetPaneId = paneRef ? resolvePaneIdByRef(paneRef) : store.activePaneId();
  if (!targetPaneId) {
    return { toolCallId: callId, name, content: "Error: No pane available to split." };
  }

  store.setActivePaneId(targetPaneId);
  store.splitActive(direction, typeof newPaneName === "string" ? newPaneName : undefined);
  const activePaneId = useWorkspaceStore.getState().activePaneId();
  return {
    toolCallId: callId,
    name,
    content: `Split pane [${targetPaneId}] ${direction}. New active pane is [${activePaneId ?? "unknown"}].`,
  };
}

function executeRenamePane(callId: string, name: string, paneName?: string, paneRef?: string): ToolResult {
  const nextName = String(paneName ?? "").trim();
  if (!nextName) {
    return { toolCallId: callId, name, content: "Error: name is required." };
  }
  const store = useWorkspaceStore.getState();
  const targetPaneId = paneRef ? resolvePaneIdByRef(paneRef) : store.activePaneId();
  if (!targetPaneId) {
    return { toolCallId: callId, name, content: "Error: No pane found to rename." };
  }
  store.setPaneName(targetPaneId, nextName);
  return { toolCallId: callId, name, content: `Renamed pane [${targetPaneId}] to \"${nextName}\".` };
}

function executeListSnippets(callId: string, name: string, owner?: string): ToolResult {
  const normalizedOwner = (owner ?? "both").toLowerCase();
  const allowed = normalizedOwner === "user" || normalizedOwner === "assistant" || normalizedOwner === "both"
    ? normalizedOwner
    : "both";
  const snippets = useSnippetStore.getState().snippets
    .filter((snippet) => allowed === "both" || snippet.owner === allowed)
    .sort((a, b) => {
      if (a.isFavorite !== b.isFavorite) return a.isFavorite ? -1 : 1;
      return b.updatedAt - a.updatedAt;
    });

  if (snippets.length === 0) {
    return { toolCallId: callId, name, content: `No snippets found for owner filter \"${allowed}\".` };
  }

  const lines = snippets.map((snippet) => {
    const preview = snippet.content.length > 80 ? `${snippet.content.slice(0, 80)}...` : snippet.content;
    return `- ${snippet.name} [${snippet.id}] owner=${snippet.owner} category=${snippet.category} :: ${preview}`;
  });

  return { toolCallId: callId, name, content: lines.join("\n") };
}

function executeCreateSnippet(callId: string, name: string, args: Record<string, any>): ToolResult {
  const snippetName = String(args.name ?? "").trim();
  const snippetContent = String(args.content ?? "").trim();
  if (!snippetName || !snippetContent) {
    return { toolCallId: callId, name, content: "Error: name and content are required." };
  }

  const tags = Array.isArray(args.tags)
    ? args.tags.map((tag) => String(tag).trim()).filter(Boolean)
    : [];

  useSnippetStore.getState().addSnippet({
    name: snippetName,
    content: snippetContent,
    owner: "assistant",
    category: typeof args.category === "string" && args.category.trim() ? args.category.trim() : "General",
    description: typeof args.description === "string" ? args.description.trim() : "",
    tags,
  });

  const created = useSnippetStore.getState().snippets.at(-1);
  return {
    toolCallId: callId,
    name,
    content: `Created assistant snippet \"${snippetName}\"${created ? ` [${created.id}]` : ""}.`,
  };
}

async function executeRunSnippet(
  callId: string,
  name: string,
  snippetRef?: string,
  paneRef?: string,
  params?: Record<string, string>,
  execute?: boolean,
): Promise<ToolResult> {
  const ref = String(snippetRef ?? "").trim();
  if (!ref) {
    return { toolCallId: callId, name, content: "Error: snippet is required." };
  }

  const snippets = useSnippetStore.getState().snippets;
  const lower = ref.toLowerCase();
  const snippet = snippets.find((entry) => entry.id === ref)
    ?? snippets.find((entry) => entry.name.trim().toLowerCase() === lower);
  if (!snippet) {
    return { toolCallId: callId, name, content: `Error: Snippet not found for \"${ref}\".` };
  }

  const paneId = resolvePaneIdByRef(paneRef);
  if (!paneId) {
    return { toolCallId: callId, name, content: "Error: No terminal pane found for snippet execution." };
  }

  const controller = getTerminalController(paneId);
  if (!controller) {
    return { toolCallId: callId, name, content: `Error: Pane ${paneId} is not ready.` };
  }

  const templateParams: Record<string, string> = {};
  if (params && typeof params === "object") {
    for (const [key, value] of Object.entries(params)) {
      templateParams[key] = String(value ?? "");
    }
  }

  const resolved = resolveSnippetTemplate(snippet.content, templateParams);
  await controller.sendText(resolved, { execute: execute !== false, trackHistory: true });
  useSnippetStore.getState().incrementUseCount(snippet.id);

  return {
    toolCallId: callId,
    name,
    content: `Snippet \"${snippet.name}\" executed in pane [${paneId}]${execute === false ? " (without Enter)" : ""}.`,
  };
}

function executeSetLayoutPreset(callId: string, name: string, preset?: string, surfaceRef?: string, workspaceRef?: string): ToolResult {
  const allowed = ["single", "2-columns", "3-columns", "grid-2x2", "main-stack"] as const;
  if (!preset || !allowed.includes(preset as (typeof allowed)[number])) {
    return {
      toolCallId: callId,
      name,
      content: "Error: preset must be one of single, 2-columns, 3-columns, grid-2x2, main-stack.",
    };
  }

  const surfaceId = resolveSurfaceIdByRef(surfaceRef, workspaceRef);
  if (!surfaceId) {
    return { toolCallId: callId, name, content: `Error: Surface not found for \"${surfaceRef ?? ""}\".` };
  }

  const store = useWorkspaceStore.getState();
  store.setActiveSurface(surfaceId);
  store.applyPresetLayout(preset as "single" | "2-columns" | "3-columns" | "grid-2x2" | "main-stack");

  return { toolCallId: callId, name, content: `Applied preset \"${preset}\" to surface [${surfaceId}].` };
}

function executeEqualizeLayout(callId: string, name: string, surfaceRef?: string, workspaceRef?: string): ToolResult {
  const surfaceId = resolveSurfaceIdByRef(surfaceRef, workspaceRef);
  if (!surfaceId) {
    return { toolCallId: callId, name, content: `Error: Surface not found for \"${surfaceRef ?? ""}\".` };
  }

  const store = useWorkspaceStore.getState();
  store.setActiveSurface(surfaceId);
  store.equalizeLayout();
  return { toolCallId: callId, name, content: `Equalized layout ratios for surface [${surfaceId}].` };
}

function executeOpenCanvasBrowser(callId: string, name: string, url?: string, panelName?: string): ToolResult {
  const store = useWorkspaceStore.getState();
  const surface = store.activeSurface();
  if (!surface || surface.layoutMode !== "canvas") {
    return { toolCallId: callId, name, content: "Error: No active canvas surface. Switch to a canvas surface first or create one." };
  }

  const rawUrl = url?.trim() || "https://google.com";
  const normalizedUrl = rawUrl.match(/^https?:\/\//) ? rawUrl : `https://${rawUrl}`;

  const paneId = store.createCanvasPanel(surface.id, {
    panelType: "browser",
    paneIcon: "web",
    paneName: panelName?.trim() || "Browser",
    url: normalizedUrl,
  });

  if (!paneId) {
    return { toolCallId: callId, name, content: "Error: Failed to create canvas browser panel." };
  }

  return {
    toolCallId: callId,
    name,
    content: `Created canvas browser panel [${paneId}]${url ? ` loading ${url}` : ""}. Use browser_navigate with pane="${paneId}" to navigate it.`,
  };
}


async function executeBrowserNavigate(callId: string, name: string, url?: string, paneRef?: string): Promise<ToolResult> {
  if (!url?.trim()) {
    return { toolCallId: callId, name, content: "Error: URL is required." };
  }

  // Canvas browser pane
  if (paneRef?.trim()) {
    const paneId = resolvePaneIdByRef(paneRef);
    if (!paneId) {
      return { toolCallId: callId, name, content: `Error: Pane not found for "${paneRef}".` };
    }
    const ctrl = getCanvasBrowserController(paneId);
    if (!ctrl) {
      return { toolCallId: callId, name, content: `Error: Pane ${paneId} is not a browser panel or is not mounted yet.` };
    }
    ctrl.navigate(url);
    return { toolCallId: callId, name, content: `Canvas browser [${paneId}] navigating to ${url}.` };
  }

  // Sidebar browser
  const browser = getBrowserController();
  if (!browser) {
    return { toolCallId: callId, name, content: "Error: Browser panel is not available. Use open_canvas_browser to create one on a canvas." };
  }
  await browser.navigate(url);
  return { toolCallId: callId, name, content: `Navigated browser to ${url}.` };
}

async function executeBrowserBack(callId: string, name: string): Promise<ToolResult> {
  const browser = getBrowserController();
  if (!browser) {
    return { toolCallId: callId, name, content: "Error: Browser panel is not available." };
  }
  await browser.back();
  return { toolCallId: callId, name, content: "Browser navigated back." };
}

async function executeBrowserForward(callId: string, name: string): Promise<ToolResult> {
  const browser = getBrowserController();
  if (!browser) {
    return { toolCallId: callId, name, content: "Error: Browser panel is not available." };
  }
  await browser.forward();
  return { toolCallId: callId, name, content: "Browser navigated forward." };
}

async function executeBrowserReload(callId: string, name: string): Promise<ToolResult> {
  const browser = getBrowserController();
  if (!browser) {
    return { toolCallId: callId, name, content: "Error: Browser panel is not available." };
  }
  await browser.reload();
  return { toolCallId: callId, name, content: "Browser reloaded." };
}

async function executeBrowserReadDom(callId: string, name: string, paneRef?: string): Promise<ToolResult> {
  // Canvas browser — use the registry controller
  if (paneRef?.trim()) {
    const paneId = resolvePaneIdByRef(paneRef);
    if (!paneId) {
      return { toolCallId: callId, name, content: `Error: Pane not found for "${paneRef}".` };
    }
    const ctrl = getCanvasBrowserController(paneId);
    if (!ctrl) {
      return { toolCallId: callId, name, content: `Error: Pane ${paneId} is not a browser panel or not mounted.` };
    }
    const snapshot = await ctrl.getDomSnapshot();
    const text = snapshot.text || "(empty DOM text)";
    const preview = text.length > 12000 ? `${text.slice(0, 12000)}\n\n[truncated]` : text;
    return {
      toolCallId: callId,
      name,
      content: `URL: ${snapshot.url}\nTitle: ${snapshot.title}\n\nDOM text:\n${preview}`,
    };
  }

  // Sidebar browser
  const browser = getBrowserController();
  if (!browser) {
    return { toolCallId: callId, name, content: "Error: Browser panel is not available." };
  }
  const snapshot = await browser.getDomSnapshot();
  const text = snapshot.text || "(empty DOM text)";
  const preview = text.length > 12000 ? `${text.slice(0, 12000)}\n\n[truncated]` : text;
  return {
    toolCallId: callId,
    name,
    content: `URL: ${snapshot.url}\nTitle: ${snapshot.title}\n\nDOM text:\n${preview}`,
  };
}

async function executeBrowserScreenshot(callId: string, name: string): Promise<ToolResult> {
  const browser = getBrowserController();
  if (!browser) {
    return { toolCallId: callId, name, content: "Error: Browser panel is not available." };
  }

  const shot = await browser.captureScreenshot();
  const amux = getBridge();
  if (!amux?.saveVisionScreenshot) {
    return { toolCallId: callId, name, content: "Error: Vision screenshot persistence is not available in this environment." };
  }

  const saved = await amux.saveVisionScreenshot({ dataUrl: shot.dataUrl });
  if (!saved?.ok) {
    return { toolCallId: callId, name, content: `Error: Failed to save screenshot: ${saved?.error || "unknown error"}` };
  }

  return {
    toolCallId: callId,
    name,
    content: `Screenshot saved: ${saved.path}\nExpiresAt: ${saved.expiresAt ? new Date(saved.expiresAt).toISOString() : "unknown"}\nPage: ${shot.title || "(untitled)"}\nURL: ${shot.url}`,
  };
}

// ---------------------------------------------------------------------------
// Browser-use tool implementations (canvas browser panels)
// ---------------------------------------------------------------------------

function resolveCanvasBrowser(callId: string, name: string, paneRef?: string): { ctrl: NonNullable<ReturnType<typeof getCanvasBrowserController>>; paneId: string } | ToolResult {
  if (!paneRef?.trim()) {
    return { toolCallId: callId, name, content: "Error: pane parameter is required for browser-use tools." };
  }
  const paneId = resolvePaneIdByRef(paneRef);
  if (!paneId) {
    return { toolCallId: callId, name, content: `Error: Pane not found for "${paneRef}".` };
  }
  const ctrl = getCanvasBrowserController(paneId);
  if (!ctrl) {
    return { toolCallId: callId, name, content: `Error: Pane "${paneRef}" is not a browser panel or is not mounted.` };
  }
  return { ctrl, paneId };
}

function isToolResult(v: unknown): v is ToolResult {
  return typeof v === "object" && v !== null && "toolCallId" in v;
}

async function executeBrowserClick(
  callId: string, name: string, paneRef?: string, selector?: string, text?: string,
): Promise<ToolResult> {
  const resolved = resolveCanvasBrowser(callId, name, paneRef);
  if (isToolResult(resolved)) return resolved;

  if (!selector && !text) {
    return { toolCallId: callId, name, content: "Error: Provide either a CSS selector or text to match." };
  }

  try {
    const script = selector
      ? `(() => {
          const el = document.querySelector(${JSON.stringify(selector)});
          if (!el) return { ok: false, error: 'Element not found: ' + ${JSON.stringify(selector)} };
          el.scrollIntoView({ block: 'center' });
          el.click();
          return { ok: true, tag: el.tagName, text: (el.textContent || '').slice(0, 100) };
        })()`
      : `(() => {
          const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_ELEMENT);
          const target = ${JSON.stringify(text)}.toLowerCase();
          let node;
          while ((node = walker.nextNode())) {
            const el = node;
            if (el.children.length === 0 || el.tagName === 'A' || el.tagName === 'BUTTON') {
              if ((el.textContent || '').trim().toLowerCase().includes(target)) {
                el.scrollIntoView({ block: 'center' });
                el.click();
                return { ok: true, tag: el.tagName, text: (el.textContent || '').slice(0, 100) };
              }
            }
          }
          return { ok: false, error: 'No element found containing text: ' + ${JSON.stringify(text)} };
        })()`;

    const result = await resolved.ctrl.executeJavaScript(script) as any;
    if (!result?.ok) {
      return { toolCallId: callId, name, content: `Error: ${result?.error || "Click failed"}` };
    }
    return { toolCallId: callId, name, content: `Clicked <${result.tag}> "${result.text}"` };
  } catch (err: any) {
    return { toolCallId: callId, name, content: `Error: ${err.message || String(err)}` };
  }
}

async function executeBrowserType(
  callId: string, name: string, paneRef?: string, selector?: string, text?: string, clear?: boolean,
): Promise<ToolResult> {
  const resolved = resolveCanvasBrowser(callId, name, paneRef);
  if (isToolResult(resolved)) return resolved;

  if (!selector || !text) {
    return { toolCallId: callId, name, content: "Error: Both selector and text are required." };
  }

  try {
    const shouldClear = clear !== false;
    const script = `(() => {
      const el = document.querySelector(${JSON.stringify(selector)});
      if (!el) return { ok: false, error: 'Element not found: ' + ${JSON.stringify(selector)} };
      el.focus();
      ${shouldClear ? `
      if ('value' in el) { el.value = ''; }
      else if (el.isContentEditable) { el.textContent = ''; }
      ` : ""}
      if ('value' in el) {
        const nativeSet = Object.getOwnPropertyDescriptor(
          Object.getPrototypeOf(el).constructor.prototype, 'value'
        )?.set;
        if (nativeSet) { nativeSet.call(el, ${JSON.stringify(text)}); }
        else { el.value = ${JSON.stringify(text)}; }
        el.dispatchEvent(new Event('input', { bubbles: true }));
        el.dispatchEvent(new Event('change', { bubbles: true }));
      } else if (el.isContentEditable) {
        el.textContent = ${JSON.stringify(text)};
        el.dispatchEvent(new Event('input', { bubbles: true }));
      }
      return { ok: true, tag: el.tagName };
    })()`;

    const result = await resolved.ctrl.executeJavaScript(script) as any;
    if (!result?.ok) {
      return { toolCallId: callId, name, content: `Error: ${result?.error || "Type failed"}` };
    }
    return { toolCallId: callId, name, content: `Typed into <${result.tag}> "${selector}"` };
  } catch (err: any) {
    return { toolCallId: callId, name, content: `Error: ${err.message || String(err)}` };
  }
}

async function executeBrowserScroll(
  callId: string, name: string, paneRef?: string, direction?: string, amount?: number, selector?: string,
): Promise<ToolResult> {
  const resolved = resolveCanvasBrowser(callId, name, paneRef);
  if (isToolResult(resolved)) return resolved;

  const pixels = amount || 400;
  const delta = direction === "up" ? -pixels : pixels;

  try {
    const script = selector
      ? `(() => {
          const el = document.querySelector(${JSON.stringify(selector)});
          if (!el) return { ok: false, error: 'Element not found' };
          el.scrollBy(0, ${delta});
          return { ok: true, scrollTop: el.scrollTop };
        })()`
      : `(() => { window.scrollBy(0, ${delta}); return { ok: true, scrollY: window.scrollY }; })()`;

    const result = await resolved.ctrl.executeJavaScript(script) as any;
    if (!result?.ok) {
      return { toolCallId: callId, name, content: `Error: ${result?.error || "Scroll failed"}` };
    }
    return { toolCallId: callId, name, content: `Scrolled ${direction} by ${pixels}px. Position: ${result.scrollY ?? result.scrollTop}` };
  } catch (err: any) {
    return { toolCallId: callId, name, content: `Error: ${err.message || String(err)}` };
  }
}

async function executeBrowserGetElements(
  callId: string, name: string, paneRef?: string, filter?: string, limit?: number,
): Promise<ToolResult> {
  const resolved = resolveCanvasBrowser(callId, name, paneRef);
  if (isToolResult(resolved)) return resolved;

  const maxItems = Math.min(limit || 50, 200);
  const filterType = filter || "all";

  try {
    const script = `(() => {
      const filterType = ${JSON.stringify(filterType)};
      const selectors = {
        links: 'a[href]',
        buttons: 'button, [role="button"], input[type="submit"], input[type="button"]',
        inputs: 'input:not([type="hidden"]), textarea, select, [contenteditable="true"]',
        all: 'a[href], button, [role="button"], input:not([type="hidden"]), textarea, select, [contenteditable="true"]',
      };
      const sel = selectors[filterType] || selectors.all;
      const els = Array.from(document.querySelectorAll(sel)).slice(0, ${maxItems});
      return els.map((el, i) => {
        const rect = el.getBoundingClientRect();
        const visible = rect.width > 0 && rect.height > 0 && rect.top < window.innerHeight && rect.bottom > 0;
        if (!visible) return null;
        const text = (el.textContent || '').trim().slice(0, 80);
        const tag = el.tagName.toLowerCase();
        const href = el.getAttribute('href') || '';
        const type = el.getAttribute('type') || '';
        const placeholder = el.getAttribute('placeholder') || '';
        const id = el.id ? '#' + el.id : '';
        const cls = el.className && typeof el.className === 'string' ? '.' + el.className.split(' ')[0] : '';
        const hint = tag + id + cls;
        return { tag, text, href, type, placeholder, hint };
      }).filter(Boolean);
    })()`;

    const result = await resolved.ctrl.executeJavaScript(script) as any[];
    if (!result || result.length === 0) {
      return { toolCallId: callId, name, content: `No ${filterType} elements found on the page.` };
    }

    const lines = result.map((el: any) => {
      const parts = [`<${el.tag}>`];
      if (el.text) parts.push(`"${el.text}"`);
      if (el.href) parts.push(`href=${el.href}`);
      if (el.type) parts.push(`type=${el.type}`);
      if (el.placeholder) parts.push(`placeholder="${el.placeholder}"`);
      parts.push(`selector="${el.hint}"`);
      return parts.join(" ");
    });

    return { toolCallId: callId, name, content: `Found ${result.length} ${filterType} elements:\n${lines.join("\n")}` };
  } catch (err: any) {
    return { toolCallId: callId, name, content: `Error: ${err.message || String(err)}` };
  }
}

async function executeBrowserEvalJs(
  callId: string, name: string, paneRef?: string, code?: string,
): Promise<ToolResult> {
  const resolved = resolveCanvasBrowser(callId, name, paneRef);
  if (isToolResult(resolved)) return resolved;

  if (!code?.trim()) {
    return { toolCallId: callId, name, content: "Error: code parameter is required." };
  }

  try {
    const result = await resolved.ctrl.executeJavaScript(code);
    const output = result === undefined ? "(undefined)" : JSON.stringify(result, null, 2);
    const maxChars = 12000;
    const truncated = output.length > maxChars
      ? `${output.slice(0, maxChars)}\n\n[truncated to ${maxChars} chars]`
      : output;
    return { toolCallId: callId, name, content: truncated };
  } catch (err: any) {
    return { toolCallId: callId, name, content: `Error: ${err.message || String(err)}` };
  }
}

async function executeReadTerminalContent(
  callId: string,
  name: string,
  paneRef?: string,
  opts?: { include_dom?: boolean },
): Promise<ToolResult> {
  const paneId = resolvePaneIdByRef(paneRef);
  if (!paneId) {
    return { toolCallId: callId, name, content: "Error: No terminal pane found. Open a terminal first or provide a valid pane name/ID." };
  }

  // Check if this is a canvas browser panel
  const browserController = getCanvasBrowserController(paneId);
  if (browserController) {
    if (opts?.include_dom) {
      try {
        const snapshot = await browserController.getDomSnapshot();
        return {
          toolCallId: callId,
          name,
          content: `Browser Panel\nURL: ${snapshot.url}\nTitle: ${snapshot.title}\n\n${snapshot.text}`,
        };
      } catch (err: any) {
        return { toolCallId: callId, name, content: `Error reading browser DOM: ${err.message || String(err)}` };
      }
    }
    return {
      toolCallId: callId,
      name,
      content: `Browser Panel\nURL: ${browserController.getUrl()}\nTitle: ${browserController.getTitle()}`,
    };
  }

  const content = getTerminalSnapshot(paneId).trim();
  if (!content) {
    return { toolCallId: callId, name, content: `Pane ${paneId} has no readable terminal content yet.` };
  }

  const maxChars = 16000;
  const output = content.length > maxChars
    ? `${content.slice(content.length - maxChars)}\n\n[truncated to last ${maxChars} chars]`
    : content;

  return { toolCallId: callId, name, content: output };
}

async function executeTerminalCommand(
  callId: string, name: string, command: string, paneId?: string,
): Promise<ToolResult> {
  const amux = getBridge();
  if (!amux?.sendTerminalInput && !amux?.executeManagedCommand) {
    return { toolCallId: callId, name, content: "Error: Terminal bridge not available." };
  }

  const targetPaneId = resolvePaneIdByRef(paneId);
  if (!targetPaneId) {
    return { toolCallId: callId, name, content: "Error: No terminal pane found. Open a terminal first or provide a valid pane name/ID." };
  }

  try {
    if (typeof command !== "string" || !command.trim()) {
      return { toolCallId: callId, name, content: "Error: Empty command." };
    }

    const normalizedCommand = command.trim();
    const securityLevel = useSettingsStore.getState().settings.securityLevel;
    const risk = assessCommandRisk(normalizedCommand, securityLevel);

    // Managed execution is required for policy approvals and auditability.
    if (amux?.executeManagedCommand) {
      const managedAwaiter = createManagedCommandAwaiter(targetPaneId, normalizedCommand, "agent");
      try {
        await amux.executeManagedCommand(targetPaneId, {
          command: normalizedCommand,
          rationale: "Agent requested terminal tool execution",
          allowNetwork: useSettingsStore.getState().settings.sandboxNetworkEnabled,
          sandboxEnabled: useSettingsStore.getState().settings.sandboxEnabled,
          securityLevel,
          source: "agent",
        });
      } catch (error) {
        managedAwaiter.cancel();
        throw error;
      }

      const managedResult = await managedAwaiter.promise;
      if (managedResult.status === "finished") {
        return {
          toolCallId: callId,
          name,
          content: `Command finished in pane ${targetPaneId} with exit code ${managedResult.exitCode ?? "unknown"}.`,
        };
      }
      if (managedResult.status === "approved") {
        return {
          toolCallId: callId,
          name,
          content: `Command approval accepted in pane ${targetPaneId} (${managedResult.decision}).`,
        };
      }
      if (managedResult.status === "denied") {
        return {
          toolCallId: callId,
          name,
          content: `Command was denied by approval policy in pane ${targetPaneId}.`,
        };
      }
      if (managedResult.status === "rejected") {
        return {
          toolCallId: callId,
          name,
          content: `Error: Command rejected in pane ${targetPaneId}: ${managedResult.message}`,
        };
      }
      return {
        toolCallId: callId,
        name,
        content: `Command queued in pane ${targetPaneId}, but timed out while waiting for completion.${risk.requiresApproval ? " Approval may still be pending." : ""}`,
      };
    }

    // Direct input fallback has no managed policy path, so reject risky commands.
    if (risk.requiresApproval) {
      return {
        toolCallId: callId,
        name,
        content: `Error: Managed execution unavailable; blocked risky command (${risk.riskLevel}): ${risk.reasons.join(", ")}`,
      };
    }

    if (amux?.sendTerminalInput) {
      await amux.sendTerminalInput(targetPaneId, encodeBase64(`${normalizedCommand}\r`));
      return {
        toolCallId: callId,
        name,
        content: `Command sent directly to pane ${targetPaneId} (managed policy unavailable).`,
      };
    }

    return { toolCallId: callId, name, content: "Error: No terminal execution path available." };
  } catch (err: any) {
    return { toolCallId: callId, name, content: `Error: ${err.message || String(err)}` };
  }
}

async function executeGetSystemInfo(
  callId: string, name: string,
): Promise<ToolResult> {
  const amux = getBridge();
  if (!amux?.getSystemMonitorSnapshot) {
    return { toolCallId: callId, name, content: "Error: System monitor not available." };
  }

  try {
    const snapshot = await amux.getSystemMonitorSnapshot({ processLimit: 5 });
    const info = [
      `CPU: ${snapshot.cpu?.usagePercent?.toFixed(1) ?? "N/A"}%`,
      `RAM: ${formatBytes(snapshot.memory?.usedBytes)} / ${formatBytes(snapshot.memory?.totalBytes)}`,
      ...(snapshot.gpus?.length > 0
        ? snapshot.gpus.map((g: any) =>
          `GPU: ${g.name} - ${g.utilizationPercent?.toFixed(1) ?? "N/A"}%, VRAM: ${formatBytes(g.memoryUsedBytes)} / ${formatBytes(g.memoryTotalBytes)}`
        )
        : []),
      `Top processes: ${snapshot.processes?.map((p: any) => `${p.name} (${p.cpuPercent?.toFixed(1)}%)`).join(", ") || "N/A"}`,
    ].join("\n");
    return { toolCallId: callId, name, content: info };
  } catch (err: any) {
    return { toolCallId: callId, name, content: `Error: ${err.message || String(err)}` };
  }
}

function formatBytes(bytes: number | null | undefined): string {
  if (bytes == null) return "N/A";
  const gb = bytes / (1024 * 1024 * 1024);
  if (gb >= 1) return `${gb.toFixed(1)}GB`;
  const mb = bytes / (1024 * 1024);
  return `${mb.toFixed(0)}MB`;
}

/**
 * Build a capability description for the system prompt based on available tools.
 */
export function getToolCapabilityDescription(tools: ToolDefinition[]): string {
  if (tools.length === 0) return "";

  const names = tools.map((t) => t.function.name);
  const capabilities: string[] = [];
  const described = new Set<string>();

  if (names.includes("list_terminals")) {
    capabilities.push("- List available terminal panes");
    described.add("list_terminals");
  }
  if (names.includes("run_terminal_command")) {
    capabilities.push("- Execute shell commands in a specific terminal pane (defaults to active pane)");
    described.add("run_terminal_command");
  }
  if (names.includes("read_active_terminal_content")) {
    capabilities.push("- Read terminal content or browser panel info from the active pane or by pane name/ID");
    described.add("read_active_terminal_content");
  }
  if (names.includes("get_system_info")) {
    capabilities.push("- Check system resource usage (CPU, RAM, GPU)");
    described.add("get_system_info");
  }
  if (names.includes("list_workspaces")) {
    capabilities.push("- List workspaces, surfaces, and panes");
    described.add("list_workspaces");
  }
  if (names.includes("create_workspace")) {
    capabilities.push("- Create and switch workspaces");
    described.add("create_workspace");
  }
  if (names.includes("set_active_workspace")) {
    capabilities.push("- Switch active workspace");
    described.add("set_active_workspace");
  }
  if (names.includes("create_surface")) {
    capabilities.push("- Create surfaces (tabs) in a workspace");
    described.add("create_surface");
  }
  if (names.includes("set_active_surface")) {
    capabilities.push("- Switch active surface");
    described.add("set_active_surface");
  }
  if (names.includes("split_pane")) {
    capabilities.push("- Split panes horizontally or vertically");
    described.add("split_pane");
  }
  if (names.includes("set_layout_preset")) {
    capabilities.push("- Apply workspace layout presets");
    described.add("set_layout_preset");
  }
  if (names.includes("equalize_layout")) {
    capabilities.push("- Equalize pane split ratios");
    described.add("equalize_layout");
  }
  if (names.includes("rename_pane")) {
    capabilities.push("- Rename panes for clearer targeted operations");
    described.add("rename_pane");
  }
  if (names.includes("list_snippets")) {
    capabilities.push("- Read snippets created by user/assistant");
    described.add("list_snippets");
  }
  if (names.includes("create_snippet")) {
    capabilities.push("- Create reusable assistant-owned snippets");
    described.add("create_snippet");
  }
  if (names.includes("run_snippet")) {
    capabilities.push("- Execute snippets in a targeted pane");
    described.add("run_snippet");
  }
  if (names.includes("open_canvas_browser")) {
    capabilities.push("- Open a new browser panel on a canvas surface");
    described.add("open_canvas_browser");
  }
  if (names.includes("browser_navigate")) {
    capabilities.push("- Navigate the integrated web browser (sidebar or canvas browser by pane ID)");
    described.add("browser_navigate");
  }
  if (names.includes("browser_back") || names.includes("browser_forward") || names.includes("browser_reload")) {
    capabilities.push("- Control browser history and reload");
    described.add("browser_back");
    described.add("browser_forward");
    described.add("browser_reload");
  }
  if (names.includes("browser_read_dom")) {
    capabilities.push("- Read DOM content (title, URL, text) from the browser");
    described.add("browser_read_dom");
  }
  if (names.includes("browser_take_screenshot")) {
    capabilities.push("- Capture browser screenshots to temporary vision storage (auto-expire)");
    described.add("browser_take_screenshot");
  }
  if (names.includes("browser_click")) {
    capabilities.push("- Interact with canvas browser panels: click elements, type text, scroll, extract elements, run JavaScript");
    described.add("browser_click");
    described.add("browser_type");
    described.add("browser_scroll");
    described.add("browser_get_elements");
    described.add("browser_eval_js");
  }
  if (names.includes("send_slack_message")) {
    capabilities.push("- Send messages to Slack channels");
    described.add("send_slack_message");
  }
  if (names.includes("send_discord_message")) {
    capabilities.push("- Send messages to Discord channels");
    described.add("send_discord_message");
  }
  if (names.includes("send_telegram_message")) {
    capabilities.push("- Send messages to Telegram chats");
    described.add("send_telegram_message");
  }
  if (names.includes("send_whatsapp_message")) {
    capabilities.push("- Send WhatsApp messages");
    described.add("send_whatsapp_message");
  }

  for (const tool of tools) {
    if (described.has(tool.function.name)) {
      continue;
    }
    capabilities.push(`- ${tool.function.description}`);
    described.add(tool.function.name);
  }

  return `\n\nYou have access to the following tools:\n${capabilities.join("\n")}\n\nUse them when the user asks you to perform these actions. For messaging tools, confirm with the user before sending if the message content isn't clearly specified.`;
}
