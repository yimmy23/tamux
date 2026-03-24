import { Badge } from "../ui";
import { hasTerminalController } from "../../lib/terminalRegistry";

export function SearchOverlayStatus({ activePaneId }: { activePaneId: string | null }) {
  const status = !activePaneId
    ? { text: "No active terminal pane", variant: "warning" as const }
    : !hasTerminalController(activePaneId)
      ? { text: "Terminal not initialized", variant: "warning" as const }
      : { text: `Searching buffer: ${activePaneId}`, variant: "default" as const };

  return <Badge variant={status.variant} className="w-fit text-[10px] tracking-[0.02em]">{status.text}</Badge>;
}
