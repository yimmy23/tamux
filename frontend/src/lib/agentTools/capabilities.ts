import { TOOL_NAMES } from "./toolNames";
import type { ToolDefinition } from "./types";

export function getToolCapabilityDescription(tools: ToolDefinition[]): string {
  if (tools.length === 0) {
    return "";
  }

  const names = tools.map((tool) => tool.function.name);
  const capabilities: string[] = [];
  const described = new Set<string>();

  if (names.includes(TOOL_NAMES.listTerminals)) { capabilities.push("- List available terminal panes"); described.add(TOOL_NAMES.listTerminals); }
  if (names.includes(TOOL_NAMES.runTerminalCommand)) { capabilities.push("- Execute shell commands in a specific terminal pane (defaults to active pane)"); described.add(TOOL_NAMES.runTerminalCommand); }
  if (names.includes(TOOL_NAMES.readActiveTerminalContent)) { capabilities.push("- Read terminal content or browser panel info from the active pane or by pane name/ID"); described.add(TOOL_NAMES.readActiveTerminalContent); }
  if (names.includes(TOOL_NAMES.getSystemInfo)) { capabilities.push("- Check system resource usage (CPU, RAM, GPU)"); described.add(TOOL_NAMES.getSystemInfo); }
  if (names.includes(TOOL_NAMES.listWorkspaces)) { capabilities.push("- List workspaces, surfaces, and panes"); described.add(TOOL_NAMES.listWorkspaces); }
  if (names.includes(TOOL_NAMES.createWorkspace)) { capabilities.push("- Create and switch workspaces"); described.add(TOOL_NAMES.createWorkspace); }
  if (names.includes(TOOL_NAMES.setActiveWorkspace)) { capabilities.push("- Switch active workspace"); described.add(TOOL_NAMES.setActiveWorkspace); }
  if (names.includes(TOOL_NAMES.createSurface)) { capabilities.push("- Create surfaces (tabs) in a workspace"); described.add(TOOL_NAMES.createSurface); }
  if (names.includes(TOOL_NAMES.setActiveSurface)) { capabilities.push("- Switch active surface"); described.add(TOOL_NAMES.setActiveSurface); }
  if (names.includes(TOOL_NAMES.splitPane)) { capabilities.push("- Split panes horizontally or vertically"); described.add(TOOL_NAMES.splitPane); }
  if (names.includes(TOOL_NAMES.setLayoutPreset)) { capabilities.push("- Apply workspace layout presets"); described.add(TOOL_NAMES.setLayoutPreset); }
  if (names.includes(TOOL_NAMES.equalizeLayout)) { capabilities.push("- Equalize pane split ratios"); described.add(TOOL_NAMES.equalizeLayout); }
  if (names.includes(TOOL_NAMES.renamePane)) { capabilities.push("- Rename panes for clearer targeted operations"); described.add(TOOL_NAMES.renamePane); }
  if (names.includes(TOOL_NAMES.listSnippets)) { capabilities.push("- Read snippets created by user/assistant"); described.add(TOOL_NAMES.listSnippets); }
  if (names.includes(TOOL_NAMES.createSnippet)) { capabilities.push("- Create reusable assistant-owned snippets"); described.add(TOOL_NAMES.createSnippet); }
  if (names.includes(TOOL_NAMES.runSnippet)) { capabilities.push("- Execute snippets in a targeted pane"); described.add(TOOL_NAMES.runSnippet); }
  if (names.includes(TOOL_NAMES.openCanvasBrowser)) { capabilities.push("- Open a new browser panel on a canvas surface"); described.add(TOOL_NAMES.openCanvasBrowser); }
  if (names.includes(TOOL_NAMES.browserNavigate)) { capabilities.push("- Navigate the integrated web browser (sidebar or canvas browser by pane ID)"); described.add(TOOL_NAMES.browserNavigate); }
  if (names.includes(TOOL_NAMES.browserBack) || names.includes(TOOL_NAMES.browserForward) || names.includes(TOOL_NAMES.browserReload)) {
    capabilities.push("- Control browser history and reload");
    described.add(TOOL_NAMES.browserBack);
    described.add(TOOL_NAMES.browserForward);
    described.add(TOOL_NAMES.browserReload);
  }
  if (names.includes(TOOL_NAMES.browserReadDom)) { capabilities.push("- Read DOM content (title, URL, text) from the browser"); described.add(TOOL_NAMES.browserReadDom); }
  if (names.includes(TOOL_NAMES.browserTakeScreenshot)) { capabilities.push("- Capture browser screenshots to temporary vision storage (auto-expire)"); described.add(TOOL_NAMES.browserTakeScreenshot); }
  if (names.includes(TOOL_NAMES.browserClick)) {
    capabilities.push("- Interact with canvas browser panels: click elements, type text, scroll, extract elements, run JavaScript");
    described.add(TOOL_NAMES.browserClick);
    described.add(TOOL_NAMES.browserType);
    described.add(TOOL_NAMES.browserScroll);
    described.add(TOOL_NAMES.browserGetElements);
    described.add(TOOL_NAMES.browserEvalJs);
  }
  if (names.includes(TOOL_NAMES.sendSlackMessage)) { capabilities.push("- Send messages to Slack channels"); described.add(TOOL_NAMES.sendSlackMessage); }
  if (names.includes(TOOL_NAMES.sendDiscordMessage)) { capabilities.push("- Send messages to Discord channels"); described.add(TOOL_NAMES.sendDiscordMessage); }
  if (names.includes(TOOL_NAMES.sendTelegramMessage)) { capabilities.push("- Send messages to Telegram chats"); described.add(TOOL_NAMES.sendTelegramMessage); }
  if (names.includes(TOOL_NAMES.sendWhatsAppMessage)) { capabilities.push("- Send WhatsApp messages"); described.add(TOOL_NAMES.sendWhatsAppMessage); }

  for (const tool of tools) {
    if (described.has(tool.function.name)) {
      continue;
    }
    capabilities.push(`- ${tool.function.description}`);
    described.add(tool.function.name);
  }

  return `\n\nYou have access to the following tools:\n${capabilities.join("\n")}\n\nUse them when the user asks you to perform these actions. For messaging tools, confirm with the user before sending if the message content isn't clearly specified.`;
}
