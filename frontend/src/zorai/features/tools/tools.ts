export const zoraiTools = [
  {
    id: "terminal",
    title: "Terminal",
    description: "Open managed terminal sessions as a secondary operator surface.",
  },
  {
    id: "canvas",
    title: "Infinite Canvas",
    description: "Arrange terminals and browser panels on a freeform operator canvas.",
  },
  {
    id: "files",
    title: "Files",
    description: "Inspect and move workspace files without leaving orchestration.",
  },
  {
    id: "browser",
    title: "Browser",
    description: "Use browser surfaces when an agent workflow needs web context.",
  },
  {
    id: "history",
    title: "Command History",
    description: "Review executed commands, status, and reusable shell entries.",
  },
  {
    id: "system",
    title: "System Monitor",
    description: "Inspect host and daemon runtime status.",
  },
  {
    id: "vault",
    title: "Session Vault",
    description: "Restore and review durable transcript memory.",
  },
] as const;

export type ZoraiToolId = (typeof zoraiTools)[number]["id"];

export function getDefaultZoraiTool(): ZoraiToolId {
  return "terminal";
}
