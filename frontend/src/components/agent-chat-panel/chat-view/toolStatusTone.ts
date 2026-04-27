import type { ToolEventGroup } from "./types";

export function toolStatusTone(status: ToolEventGroup["status"]) {
  switch (status) {
    case "done":
      return {
        text: "var(--success)",
        border: "rgba(74, 222, 128, 0.35)",
        background: "rgba(22, 101, 52, 0.18)",
      };
    case "error":
      return {
        text: "var(--danger)",
        border: "rgba(248, 113, 113, 0.38)",
        background: "rgba(127, 29, 29, 0.2)",
      };
    case "executing":
    case "requested":
    default:
      return {
        text: "var(--warning)",
        border: "rgba(251, 191, 36, 0.38)",
        background: "rgba(146, 64, 14, 0.18)",
      };
  }
}
