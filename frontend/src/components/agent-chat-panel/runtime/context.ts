import { createContext, useContext } from "react";
import type { AgentChatPanelRuntimeValue } from "./types";

export const AgentChatPanelRuntimeContext = createContext<AgentChatPanelRuntimeValue | null>(null);

export function useAgentChatPanelRuntime(): AgentChatPanelRuntimeValue {
  const runtime = useContext(AgentChatPanelRuntimeContext);
  if (!runtime) {
    throw new Error("AgentChatPanel runtime is only available inside AgentChatPanelProvider.");
  }
  return runtime;
}
