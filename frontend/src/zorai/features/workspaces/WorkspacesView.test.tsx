import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { AgentChatPanelRuntimeContext } from "@/components/agent-chat-panel/runtime/context";
import type { AgentChatPanelRuntimeValue } from "@/components/agent-chat-panel/runtime/types";
import { WorkspacesView } from "./WorkspacesView";

describe("WorkspacesView polish", () => {
  it("keeps live status text and uses modal task details/editing", () => {
    const html = renderToStaticMarkup(
      <AgentChatPanelRuntimeContext.Provider value={{} as AgentChatPanelRuntimeValue}>
        <WorkspacesView />
      </AgentChatPanelRuntimeContext.Provider>,
    );

    expect(html).toContain("Workspace Board");
    expect(html).toContain("New task");
    expect(html).toContain('role="status"');
    expect(html).toContain('aria-live="polite"');
    expect(html).toContain("Workspace board ready.");
    expect(html).not.toContain("No task selected.");
  });
});
