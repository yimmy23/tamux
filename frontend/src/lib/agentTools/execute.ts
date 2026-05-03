import { executePluginAssistantTool } from "../../plugins/assistantToolRegistry";
import { executeAgentQueryMemory, executeDiscordMessage, executeGatewayMessage, executeWhatsAppMessage } from "./gatewayActions";
import {
  executeBrowserBack,
  executeBrowserClick,
  executeBrowserEvalJs,
  executeBrowserForward,
  executeBrowserGetElements,
  executeBrowserNavigate,
  executeBrowserReadDom,
  executeBrowserReload,
  executeBrowserScreenshot,
  executeBrowserScroll,
  executeBrowserType,
} from "./browserActions";
import { executeGetSystemInfo, executeReadTerminalContent, executeTerminalCommand } from "./terminalActions";
import { TOOL_NAMES } from "./toolNames";
import type { ToolCall, ToolResult } from "./types";
import {
  executeCreateSnippet,
  executeCreateSurface,
  executeCreateWorkspace,
  executeEqualizeLayout,
  executeListSnippets,
  executeListTerminals,
  executeListWorkspaces,
  executeOpenCanvasBrowser,
  executeRenamePane,
  executeRunSnippet,
  executeSetActiveSurface,
  executeSetActiveWorkspace,
  executeSetLayoutPreset,
  executeSplitPane,
} from "./workspaceActions";

function withWelesReview(call: ToolCall, result: ToolResult): ToolResult {
  return {
    ...result,
    weles_review: result.weles_review ?? call.weles_review,
  };
}

export async function executeTool(call: ToolCall): Promise<ToolResult> {
  const name = call.function.name;
  let args: Record<string, any>;
  try {
    args = JSON.parse(call.function.arguments);
  } catch {
    return withWelesReview(call, {
      toolCallId: call.id,
      name,
      content: "Error: Invalid JSON arguments",
    });
  }

  try {
    const result = await (async (): Promise<ToolResult> => {
      switch (name) {
      case TOOL_NAMES.sendSlackMessage:
        return await executeGatewayMessage(call.id, name, "slack", args.channel, args.message);
      case TOOL_NAMES.sendDiscordMessage:
        return await executeDiscordMessage(call.id, name, args.channel_id, args.user_id, args.message);
      case TOOL_NAMES.sendTelegramMessage:
        return await executeGatewayMessage(call.id, name, "telegram", args.chat_id, args.message);
      case TOOL_NAMES.sendWhatsAppMessage:
        return await executeWhatsAppMessage(call.id, name, args.phone, args.message);
      case TOOL_NAMES.listTerminals:
        return executeListTerminals(call.id, name);
      case TOOL_NAMES.listWorkspaces:
        return executeListWorkspaces(call.id, name);
      case TOOL_NAMES.createWorkspace:
        return executeCreateWorkspace(call.id, name, args.name);
      case TOOL_NAMES.setActiveWorkspace:
        return executeSetActiveWorkspace(call.id, name, args.workspace);
      case TOOL_NAMES.createSurface:
        return executeCreateSurface(call.id, name, args.workspace, args.name);
      case TOOL_NAMES.setActiveSurface:
        return executeSetActiveSurface(call.id, name, args.surface, args.workspace);
      case TOOL_NAMES.splitPane:
        return executeSplitPane(call.id, name, args.direction, args.pane || args.pane_id, args.new_pane_name);
      case TOOL_NAMES.renamePane:
        return executeRenamePane(call.id, name, args.name, args.pane || args.pane_id);
      case TOOL_NAMES.setLayoutPreset:
        return executeSetLayoutPreset(call.id, name, args.preset, args.surface, args.workspace);
      case TOOL_NAMES.equalizeLayout:
        return executeEqualizeLayout(call.id, name, args.surface, args.workspace);
      case TOOL_NAMES.listSnippets:
        return executeListSnippets(call.id, name, args.owner);
      case TOOL_NAMES.createSnippet:
        return executeCreateSnippet(call.id, name, args);
      case TOOL_NAMES.runSnippet:
        return await executeRunSnippet(call.id, name, args.snippet, args.pane || args.pane_id, args.params, args.execute);
      case TOOL_NAMES.openCanvasBrowser:
        return executeOpenCanvasBrowser(call.id, name, args.url, args.name, args.profileId);
      case TOOL_NAMES.browserNavigate:
        return await executeBrowserNavigate(call.id, name, args.url, args.pane);
      case TOOL_NAMES.browserBack:
        return await executeBrowserBack(call.id, name);
      case TOOL_NAMES.browserForward:
        return await executeBrowserForward(call.id, name);
      case TOOL_NAMES.browserReload:
        return await executeBrowserReload(call.id, name);
      case TOOL_NAMES.browserReadDom:
        return await executeBrowserReadDom(call.id, name, args.pane);
      case TOOL_NAMES.browserTakeScreenshot:
        return await executeBrowserScreenshot(call.id, name);
      case TOOL_NAMES.browserClick:
        return await executeBrowserClick(call.id, name, args.pane, args.selector, args.text);
      case TOOL_NAMES.browserType:
        return await executeBrowserType(call.id, name, args.pane, args.selector, args.text, args.clear);
      case TOOL_NAMES.browserScroll:
        return await executeBrowserScroll(call.id, name, args.pane, args.direction, args.amount, args.selector);
      case TOOL_NAMES.browserGetElements:
        return await executeBrowserGetElements(call.id, name, args.pane, args.filter, args.limit);
      case TOOL_NAMES.browserEvalJs:
        return await executeBrowserEvalJs(call.id, name, args.pane, args.code);
      case TOOL_NAMES.readActiveTerminalContent:
        return await executeReadTerminalContent(call.id, name, args.pane || args.pane_id, { include_dom: args.include_dom });
      case TOOL_NAMES.runTerminalCommand:
        return await executeTerminalCommand(call.id, name, args.command, args.pane || args.pane_id);
      case TOOL_NAMES.getSystemInfo:
        return await executeGetSystemInfo(call.id, name);
      case TOOL_NAMES.agentQueryMemory:
        return await executeAgentQueryMemory(call.id, name, args.query);
      default: {
        const pluginResult = await executePluginAssistantTool(call, args);
        if (pluginResult) {
          return withWelesReview(call, pluginResult);
        }
        return withWelesReview(call, {
          toolCallId: call.id,
          name,
          content: `Error: Unknown tool '${name}'`,
        });
      }
      }
    })();
    return withWelesReview(call, result);
  } catch (error: any) {
    return withWelesReview(call, {
      toolCallId: call.id,
      name,
      content: `Error: ${error.message || String(error)}`,
    });
  }
}
