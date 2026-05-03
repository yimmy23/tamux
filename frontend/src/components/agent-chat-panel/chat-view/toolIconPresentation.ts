export type ToolIconKind =
  | "web"
  | "guideline"
  | "skill"
  | "python"
  | "terminal"
  | "file"
  | "search"
  | "memory"
  | "workspace"
  | "communication"
  | "audio"
  | "system"
  | "model"
  | "agent"
  | "task"
  | "goal"
  | "routine"
  | "trigger"
  | "workflow"
  | "debate"
  | "collaboration"
  | "plugin"
  | "thread"
  | "default";

export type ToolIconPresentation = {
  kind: ToolIconKind;
  glyph: string;
  label: string;
};

const TOOL_ICONS: Record<ToolIconKind, ToolIconPresentation> = {
  web: { kind: "web", glyph: "WEB", label: "Web" },
  guideline: { kind: "guideline", glyph: "BOOK", label: "Guideline" },
  skill: { kind: "skill", glyph: "BRAIN", label: "Skill" },
  python: { kind: "python", glyph: "PY", label: "Python" },
  terminal: { kind: "terminal", glyph: ">_", label: "Terminal" },
  file: { kind: "file", glyph: "FILE", label: "File" },
  search: { kind: "search", glyph: "SRCH", label: "Search" },
  memory: { kind: "memory", glyph: "MEM", label: "Memory" },
  workspace: { kind: "workspace", glyph: "WKSP", label: "Workspace" },
  communication: { kind: "communication", glyph: "MSG", label: "Communication" },
  audio: { kind: "audio", glyph: "AUD", label: "Audio" },
  system: { kind: "system", glyph: "SYS", label: "System" },
  model: { kind: "model", glyph: "MOD", label: "Model" },
  agent: { kind: "agent", glyph: "AGNT", label: "Agent" },
  task: { kind: "task", glyph: "TASK", label: "Task" },
  goal: { kind: "goal", glyph: "GOAL", label: "Goal" },
  routine: { kind: "routine", glyph: "RTN", label: "Routine" },
  trigger: { kind: "trigger", glyph: "TRIG", label: "Trigger" },
  workflow: { kind: "workflow", glyph: "FLOW", label: "Workflow" },
  debate: { kind: "debate", glyph: "DEB", label: "Debate" },
  collaboration: { kind: "collaboration", glyph: "TEAM", label: "Collaboration" },
  plugin: { kind: "plugin", glyph: "PLUG", label: "Plugin" },
  thread: { kind: "thread", glyph: "THRD", label: "Thread" },
  default: { kind: "default", glyph: "TOOL", label: "Tool" },
};

export function getToolIconPresentation(
  toolName: string,
  toolArguments?: string,
): ToolIconPresentation {
  const normalizedName = toolName.trim().toLowerCase();

  if (isPythonTool(normalizedName, toolArguments)) return TOOL_ICONS.python;
  if (isWebTool(normalizedName)) return TOOL_ICONS.web;
  if (hasToolName(TOOL_NAME_GROUPS.guideline, normalizedName)) return TOOL_ICONS.guideline;
  if (hasToolName(TOOL_NAME_GROUPS.skill, normalizedName)) return TOOL_ICONS.skill;
  if (normalizedName.includes(TOOL_NAME_FRAGMENTS.generatedTool)) {
    return TOOL_ICONS.skill;
  }
  if (isPluginTool(normalizedName)) return TOOL_ICONS.plugin;
  if (hasToolName(TOOL_NAME_GROUPS.collaboration, normalizedName)) return TOOL_ICONS.collaboration;
  if (hasToolName(TOOL_NAME_GROUPS.memory, normalizedName)) return TOOL_ICONS.memory;
  if (hasToolName(TOOL_NAME_GROUPS.file, normalizedName)) return TOOL_ICONS.file;
  if (hasToolName(TOOL_NAME_GROUPS.search, normalizedName)) return TOOL_ICONS.search;
  if (hasToolName(TOOL_NAME_GROUPS.workspace, normalizedName)) return TOOL_ICONS.workspace;
  if (hasToolName(TOOL_NAME_GROUPS.communication, normalizedName)) return TOOL_ICONS.communication;
  if (hasToolName(TOOL_NAME_GROUPS.audio, normalizedName)) return TOOL_ICONS.audio;
  if (hasToolName(TOOL_NAME_GROUPS.system, normalizedName)) return TOOL_ICONS.system;
  if (hasToolName(TOOL_NAME_GROUPS.model, normalizedName)) return TOOL_ICONS.model;
  if (hasToolName(TOOL_NAME_GROUPS.agent, normalizedName)) return TOOL_ICONS.agent;
  if (hasToolName(TOOL_NAME_GROUPS.goal, normalizedName)) return TOOL_ICONS.goal;
  if (hasToolName(TOOL_NAME_GROUPS.routine, normalizedName)) return TOOL_ICONS.routine;
  if (hasToolName(TOOL_NAME_GROUPS.trigger, normalizedName)) return TOOL_ICONS.trigger;
  if (hasToolName(TOOL_NAME_GROUPS.workflow, normalizedName)) return TOOL_ICONS.workflow;
  if (hasToolName(TOOL_NAME_GROUPS.debate, normalizedName)) return TOOL_ICONS.debate;
  if (hasToolName(TOOL_NAME_GROUPS.task, normalizedName)) return TOOL_ICONS.task;
  if (hasToolName(TOOL_NAME_GROUPS.thread, normalizedName)) return TOOL_ICONS.thread;
  if (isTerminalTool(normalizedName)) return TOOL_ICONS.terminal;

  return TOOL_ICONS.default;
}

function isPythonTool(normalizedName: string, toolArguments?: string): boolean {
  if (normalizedName === TOOL_NAMES.pythonExecute || normalizedName.includes("python")) return true;

  const args = parseObject(toolArguments);
  const languageHint = readString(args, "language_hint").toLowerCase();
  if (languageHint.includes("python")) return true;

  const command = readString(args, "command").trim().toLowerCase();
  return commandUsesPython(command);
}

function isWebTool(normalizedName: string): boolean {
  return hasToolName(TOOL_NAME_GROUPS.web, normalizedName)
    || normalizedName.startsWith(TOOL_NAME_PREFIXES.browser)
    || normalizedName.includes(TOOL_NAME_FRAGMENTS.webBrowsing);
}

function isTerminalTool(normalizedName: string): boolean {
  return hasToolName(TOOL_NAME_GROUPS.terminal, normalizedName)
    || normalizedName.includes(TOOL_NAME_FRAGMENTS.terminal);
}

function isPluginTool(normalizedName: string): boolean {
  return hasToolName(TOOL_NAME_GROUPS.plugin, normalizedName)
    || normalizedName.startsWith(TOOL_NAME_PREFIXES.plugin)
    || normalizedName.includes(TOOL_NAME_FRAGMENTS.plugin);
}

function hasToolName(names: readonly string[], normalizedName: string): boolean {
  return names.includes(normalizedName);
}

function commandUsesPython(command: string): boolean {
  if (!command) return false;
  return command.startsWith("python ")
    || command.startsWith("python3 ")
    || command.startsWith("python -")
    || command.startsWith("python3 -")
    || command.startsWith("uv run python ")
    || command.includes(" python ")
    || command.includes(" python3 ");
}

function parseObject(value: string | undefined): Record<string, unknown> | null {
  if (!value) return null;
  try {
    const parsed: unknown = JSON.parse(value);
    return parsed && typeof parsed === "object" && !Array.isArray(parsed)
      ? parsed as Record<string, unknown>
      : null;
  } catch {
    return null;
  }
}

function readString(source: Record<string, unknown> | null, key: string): string {
  const value = source?.[key];
  return typeof value === "string" ? value : "";
}
import { TOOL_NAME_FRAGMENTS, TOOL_NAME_GROUPS, TOOL_NAME_PREFIXES, TOOL_NAMES } from "@/lib/agentTools/toolNames";
