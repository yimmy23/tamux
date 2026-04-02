import type { ToolDefinition } from "./types";

export function getToolCapabilityDescription(tools: ToolDefinition[]): string {
  if (tools.length === 0) {
    return "";
  }

  const names = tools.map((tool) => tool.function.name);
  const capabilities: string[] = [];
  const described = new Set<string>();

  if (names.includes("list_terminals")) { capabilities.push("- List available terminal panes"); described.add("list_terminals"); }
  if (names.includes("run_terminal_command")) { capabilities.push("- Execute shell commands in a specific terminal pane (defaults to active pane)"); described.add("run_terminal_command"); }
  if (names.includes("read_active_terminal_content")) { capabilities.push("- Read terminal content or browser panel info from the active pane or by pane name/ID"); described.add("read_active_terminal_content"); }
  if (names.includes("get_system_info")) { capabilities.push("- Check system resource usage (CPU, RAM, GPU)"); described.add("get_system_info"); }
  if (names.includes("list_workspaces")) { capabilities.push("- List workspaces, surfaces, and panes"); described.add("list_workspaces"); }
  if (names.includes("create_workspace")) { capabilities.push("- Create and switch workspaces"); described.add("create_workspace"); }
  if (names.includes("set_active_workspace")) { capabilities.push("- Switch active workspace"); described.add("set_active_workspace"); }
  if (names.includes("create_surface")) { capabilities.push("- Create surfaces (tabs) in a workspace"); described.add("create_surface"); }
  if (names.includes("set_active_surface")) { capabilities.push("- Switch active surface"); described.add("set_active_surface"); }
  if (names.includes("split_pane")) { capabilities.push("- Split panes horizontally or vertically"); described.add("split_pane"); }
  if (names.includes("set_layout_preset")) { capabilities.push("- Apply workspace layout presets"); described.add("set_layout_preset"); }
  if (names.includes("equalize_layout")) { capabilities.push("- Equalize pane split ratios"); described.add("equalize_layout"); }
  if (names.includes("rename_pane")) { capabilities.push("- Rename panes for clearer targeted operations"); described.add("rename_pane"); }
  if (names.includes("list_snippets")) { capabilities.push("- Read snippets created by user/assistant"); described.add("list_snippets"); }
  if (names.includes("create_snippet")) { capabilities.push("- Create reusable assistant-owned snippets"); described.add("create_snippet"); }
  if (names.includes("run_snippet")) { capabilities.push("- Execute snippets in a targeted pane"); described.add("run_snippet"); }
  if (names.includes("open_canvas_browser")) { capabilities.push("- Open a new browser panel on a canvas surface"); described.add("open_canvas_browser"); }
  if (names.includes("browser_navigate")) { capabilities.push("- Navigate the integrated web browser (sidebar or canvas browser by pane ID)"); described.add("browser_navigate"); }
  if (names.includes("browser_back") || names.includes("browser_forward") || names.includes("browser_reload")) {
    capabilities.push("- Control browser history and reload");
    described.add("browser_back");
    described.add("browser_forward");
    described.add("browser_reload");
  }
  if (names.includes("browser_read_dom")) { capabilities.push("- Read DOM content (title, URL, text) from the browser"); described.add("browser_read_dom"); }
  if (names.includes("browser_take_screenshot")) { capabilities.push("- Capture browser screenshots to temporary vision storage (auto-expire)"); described.add("browser_take_screenshot"); }
  if (names.includes("browser_click")) {
    capabilities.push("- Interact with canvas browser panels: click elements, type text, scroll, extract elements, run JavaScript");
    described.add("browser_click");
    described.add("browser_type");
    described.add("browser_scroll");
    described.add("browser_get_elements");
    described.add("browser_eval_js");
  }
  if (names.includes("send_slack_message")) { capabilities.push("- Send messages to Slack channels"); described.add("send_slack_message"); }
  if (names.includes("send_discord_message")) { capabilities.push("- Send messages to Discord channels"); described.add("send_discord_message"); }
  if (names.includes("send_telegram_message")) { capabilities.push("- Send messages to Telegram chats"); described.add("send_telegram_message"); }
  if (names.includes("send_whatsapp_message")) { capabilities.push("- Send WhatsApp messages"); described.add("send_whatsapp_message"); }

  for (const tool of tools) {
    if (described.has(tool.function.name)) {
      continue;
    }
    capabilities.push(`- ${tool.function.description}`);
    described.add(tool.function.name);
  }

  return `\n\nYou have access to the following tools:\n${capabilities.join("\n")}\n\nUse them when the user asks you to perform these actions. For messaging tools, confirm with the user before sending if the message content isn't clearly specified.`;
}
