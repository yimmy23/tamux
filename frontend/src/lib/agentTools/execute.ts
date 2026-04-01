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
        return await executeReadTerminalContent(call.id, name, args.pane || args.pane_id, { include_dom: args.include_dom });
      case "run_terminal_command":
        return await executeTerminalCommand(call.id, name, args.command, args.pane || args.pane_id);
      case "get_system_info":
        return await executeGetSystemInfo(call.id, name);
      case "agent_query_memory":
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
