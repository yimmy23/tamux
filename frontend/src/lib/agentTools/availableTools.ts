import { listPluginAssistantTools } from "../../plugins/assistantToolRegistry";
import { useAgentStore } from "../agentStore";
import { BROWSER_USE_TOOLS, VISION_TOOLS, WEB_BROWSING_TOOLS } from "./toolDefinitionsBrowser";
import { GATEWAY_TOOLS, SYSTEM_TOOLS, TERMINAL_TOOLS } from "./toolDefinitionsGatewayTerminal";
import { WORKSPACE_TOOLS } from "./toolDefinitionsWorkspace";
import type { ToolDefinition } from "./types";

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
    tools.push(...WEB_BROWSING_TOOLS, ...BROWSER_USE_TOOLS);
  }
  if (options.enable_vision_tool) {
    tools.push(...VISION_TOOLS);
  }
  if (options.gateway_enabled) {
    const settings = useAgentStore.getState().agentSettings;
    if (settings.slack_token) tools.push(GATEWAY_TOOLS[0]);
    if (settings.discord_token) tools.push(GATEWAY_TOOLS[1]);
    if (settings.telegram_token) tools.push(GATEWAY_TOOLS[2]);
    if (settings.whatsapp_token || settings.whatsapp_allowed_contacts) tools.push(GATEWAY_TOOLS[3]);
  }
  if (useAgentStore.getState().agentSettings.enable_honcho_memory) {
    const honchoTool = SYSTEM_TOOLS.find((tool) => tool.function.name === "agent_query_memory");
    if (honchoTool) {
      tools.push(honchoTool);
    }
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
