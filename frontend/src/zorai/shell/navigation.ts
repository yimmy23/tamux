export type ZoraiViewId =
  | "threads"
  | "goals"
  | "workspaces"
  | "tools"
  | "activity"
  | "settings";

export type ZoraiNavItem = {
  id: ZoraiViewId;
  label: string;
  railLabel: string;
  shortLabel: string;
  description: string;
};

export const zoraiNavItems: ZoraiNavItem[] = [
  {
    id: "threads",
    label: "Threads",
    railLabel: "Conversation Threads",
    shortLabel: "Th",
    description: "Talk with agents, route participants, and launch goals.",
  },
  {
    id: "goals",
    label: "Goals",
    railLabel: "Mission Control",
    shortLabel: "Go",
    description: "Inspect durable goals, steps, approvals, and active execution.",
  },
  {
    id: "workspaces",
    label: "Workspaces",
    railLabel: "Workspace Board",
    shortLabel: "Ws",
    description: "Coordinate board-owned tasks across thread and goal targets.",
  },
  {
    id: "tools",
    label: "Tools",
    railLabel: "Operator Tools",
    shortLabel: "Tl",
    description: "Open terminal, files, browser, history, system, and vault tools.",
  },
  {
    id: "activity",
    label: "Activity",
    railLabel: "Activity Feed",
    shortLabel: "Ac",
    description: "Review events, approvals, notifications, and audit state.",
  },
  {
    id: "settings",
    label: "Settings",
    railLabel: "Settings",
    shortLabel: "St",
    description: "Configure providers, models, tools, gateways, audio, and runtime behavior.",
  },
];

export function getDefaultZoraiView(): ZoraiViewId {
  return "threads";
}
