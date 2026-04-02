import type React from "react";
import { AgentChatPanelRuntimeContext } from "./runtime/context";
import {
  AgentChatPanelAITrainingSurface,
  AgentChatPanelChatSurface,
  AgentChatPanelCodingAgentsSurface,
  AgentChatPanelContextSurface,
  AgentChatPanelCurrentSurface,
  AgentChatPanelGraphSurface,
  AgentChatPanelHeader,
  AgentChatPanelScaffold,
  AgentChatPanelTabs,
  AgentChatPanelThreadsSurface,
  AgentChatPanelTraceSurface,
  AgentChatPanelUsageSurface,
} from "./runtime/layout";
import { useAgentChatPanelProviderValue } from "./runtime/useAgentChatPanelProviderValue";

export function AgentChatPanelProvider({ children }: { children?: React.ReactNode }) {
  const { isOpen, value } = useAgentChatPanelProviderValue();
  if (!isOpen) {
    return null;
  }

  return (
    <AgentChatPanelRuntimeContext.Provider value={value}>
      {children}
    </AgentChatPanelRuntimeContext.Provider>
  );
}

export {
  AgentChatPanelAITrainingSurface,
  AgentChatPanelChatSurface,
  AgentChatPanelCodingAgentsSurface,
  AgentChatPanelContextSurface,
  AgentChatPanelCurrentSurface,
  AgentChatPanelGraphSurface,
  AgentChatPanelHeader,
  AgentChatPanelScaffold,
  AgentChatPanelTabs,
  AgentChatPanelThreadsSurface,
  AgentChatPanelTraceSurface,
  AgentChatPanelUsageSurface,
};
