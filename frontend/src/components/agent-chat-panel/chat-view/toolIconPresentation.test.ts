import { describe, expect, it } from "vitest";

import { TOOL_NAMES } from "@/lib/agentTools/toolNames";
import { getToolIconPresentation } from "./toolIconPresentation";

describe("getToolIconPresentation", () => {
  it("classifies web browsing and search tools as web", () => {
    expect(getToolIconPresentation(TOOL_NAMES.webSearch).kind).toBe("web");
    expect(getToolIconPresentation(TOOL_NAMES.webSearch).glyph).toBe("🌐");
    expect(getToolIconPresentation(TOOL_NAMES.fetchUrl).kind).toBe("web");
    expect(getToolIconPresentation(TOOL_NAMES.browserNavigate).kind).toBe("web");
  });

  it("classifies guideline and skill governance tools distinctly", () => {
    expect(getToolIconPresentation(TOOL_NAMES.readGuideline).kind).toBe("guideline");
    expect(getToolIconPresentation(TOOL_NAMES.readGuideline).glyph).toBe("📖");
    expect(getToolIconPresentation(TOOL_NAMES.readSkill).kind).toBe("skill");
    expect(getToolIconPresentation(TOOL_NAMES.readSkill).glyph).toBe("🧠");
  });

  it("classifies direct and shell-wrapped Python calls as python", () => {
    expect(getToolIconPresentation(TOOL_NAMES.pythonExecute).kind).toBe("python");
    expect(
      getToolIconPresentation(
        TOOL_NAMES.bashCommand,
        JSON.stringify({ command: "python3 -c \"print('ok')\"" }),
      ).kind,
    ).toBe("python");
  });

  it("classifies terminal calls as terminal otherwise", () => {
    expect(getToolIconPresentation(TOOL_NAMES.bashCommand).kind).toBe("terminal");
    expect(getToolIconPresentation(TOOL_NAMES.bashCommand).glyph).toBe("⌨");
    expect(getToolIconPresentation(TOOL_NAMES.runTerminalCommand).kind).toBe("terminal");
  });

  it("classifies the broader daemon and desktop tool families", () => {
    const examples: Array<[string, ReturnType<typeof getToolIconPresentation>["kind"]]> = [
      [TOOL_NAMES.readFile, "file"],
      [TOOL_NAMES.applyPatch, "file"],
      [TOOL_NAMES.searchFiles, "search"],
      [TOOL_NAMES.searchHistory, "search"],
      [TOOL_NAMES.readMemory, "memory"],
      [TOOL_NAMES.agentQueryMemory, "memory"],
      [TOOL_NAMES.listWorkspaces, "workspace"],
      [TOOL_NAMES.splitPane, "workspace"],
      [TOOL_NAMES.sendSlackMessage, "communication"],
      [TOOL_NAMES.notifyUser, "communication"],
      [TOOL_NAMES.speechToText, "audio"],
      [TOOL_NAMES.getSystemInfo, "system"],
      [TOOL_NAMES.getCostSummary, "system"],
      [TOOL_NAMES.getGitStatus, "git"],
      [TOOL_NAMES.getGitLineStatuses, "git"],
      [TOOL_NAMES.listProviders, "model"],
      [TOOL_NAMES.switchModel, "model"],
      [TOOL_NAMES.spawnSubagent, "agent"],
      [TOOL_NAMES.handoffThreadAgent, "agent"],
      [TOOL_NAMES.updateTodo, "todo"],
      [TOOL_NAMES.getTodos, "todo"],
      [TOOL_NAMES.listTodos, "todo"],
      [TOOL_NAMES.enqueueTask, "task"],
      [TOOL_NAMES.startGoalRun, "goal"],
      [TOOL_NAMES.createRoutine, "routine"],
      [TOOL_NAMES.addTrigger, "trigger"],
      [TOOL_NAMES.runWorkflowPack, "workflow"],
      [TOOL_NAMES.runDebate, "debate"],
      [TOOL_NAMES.broadcastContribution, "collaboration"],
      [TOOL_NAMES.pluginApiCall, "plugin"],
      [TOOL_NAMES.synthesizeTool, "skill"],
      [TOOL_NAMES.listThreads, "thread"],
    ];

    for (const [toolName, kind] of examples) {
      expect(getToolIconPresentation(toolName).kind, toolName).toBe(kind);
    }

    expect(getToolIconPresentation(TOOL_NAMES.getGitStatus).glyph).toBe("⑂");
    expect(getToolIconPresentation(TOOL_NAMES.updateTodo).glyph).toBe("☑");
  });
});
