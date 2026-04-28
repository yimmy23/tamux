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
  icon: ZoraiNavIconId;
  description: string;
};

export type ZoraiNavIconId =
  | "threads"
  | "goals"
  | "workspaces"
  | "tools"
  | "activity"
  | "settings";

export const zoraiNavItems: ZoraiNavItem[] = [
  {
    id: "threads",
    label: "Threads",
    railLabel: "Conversation Threads",
    icon: "threads",
    description: "Talk with agents, route participants, and launch goals.",
  },
  {
    id: "goals",
    label: "Goals",
    railLabel: "Mission Control",
    icon: "goals",
    description: "Inspect durable goals, steps, approvals, and active execution.",
  },
  {
    id: "workspaces",
    label: "Workspaces",
    railLabel: "Workspace Board",
    icon: "workspaces",
    description: "Coordinate board-owned tasks across thread and goal targets.",
  },
  {
    id: "tools",
    label: "Tools",
    railLabel: "Operator Tools",
    icon: "tools",
    description: "Open terminal, files, browser, history, system, and vault tools.",
  },
  {
    id: "activity",
    label: "Activity",
    railLabel: "Activity Feed",
    icon: "activity",
    description: "Review events, approvals, notifications, and audit state.",
  },
  {
    id: "settings",
    label: "Settings",
    railLabel: "Settings",
    icon: "settings",
    description: "Configure providers, models, tools, gateways, audio, and runtime behavior.",
  },
];

export function getDefaultZoraiView(): ZoraiViewId {
  return "threads";
}
