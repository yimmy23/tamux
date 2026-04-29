import { describe, expect, it } from "vitest";
import type { AgentThread, SubAgentDefinition } from "@/lib/agentStore";
import { buildThreadFilterTabs, filterThreads } from "./threadFilterModel";

function thread(overrides: Partial<AgentThread>): AgentThread {
  return {
    id: "thread-1",
    daemonThreadId: "thread-1",
    workspaceId: null,
    surfaceId: null,
    paneId: null,
    agent_name: "Svarog",
    title: "Thread",
    createdAt: Date.now(),
    updatedAt: Date.now(),
    messageCount: 1,
    totalInputTokens: 0,
    totalOutputTokens: 0,
    totalTokens: 0,
    compactionCount: 0,
    lastMessagePreview: "",
    upstreamThreadId: null,
    upstreamProvider: null,
    upstreamModel: null,
    upstreamAssistantId: null,
    ...overrides,
  };
}

describe("thread filters", () => {
  it("adds subagent thread tabs from configured subagents and loaded thread agent names", () => {
    const tabs = buildThreadFilterTabs(
      [
        thread({ id: "dazhbog-thread", agent_name: "Dazhbog" }),
        thread({ id: "mokosh-thread", agent_name: "Mokosh" }),
      ],
      [{ id: "verifier", name: "Verifier", builtin: false } as SubAgentDefinition],
      new Set(),
    );

    expect(tabs.map((tab) => tab.label)).toEqual(expect.arrayContaining(["Dazhbog", "Mokosh", "Verifier"]));
  });

  it("keeps dynamic subagent threads out of Svarog and inside their own tab", () => {
    const dazhbog = thread({ id: "dazhbog-thread", agent_name: "Dazhbog" });
    const svarog = thread({ id: "svarog-thread", agent_name: "Svarog" });

    expect(filterThreads([dazhbog, svarog], {
      tab: "svarog",
      dateFilter: "all",
      fromDate: "",
      toDate: "",
      goalThreadIds: new Set(),
    }).map((item) => item.id)).toEqual(["svarog-thread"]);

    expect(filterThreads([dazhbog, svarog], {
      tab: "agent:dazhbog",
      dateFilter: "all",
      fromDate: "",
      toDate: "",
      goalThreadIds: new Set(),
    }).map((item) => item.id)).toEqual(["dazhbog-thread"]);
  });

  it("does not treat missing agent ownership as Svarog-owned", () => {
    const swarozyc = thread({ id: "swarozyc-thread", agent_name: "", title: "Swarozyc worker" });
    const svarog = thread({ id: "svarog-thread", agent_name: "Svarog" });

    expect(filterThreads([swarozyc, svarog], {
      tab: "svarog",
      dateFilter: "all",
      fromDate: "",
      toDate: "",
      goalThreadIds: new Set(),
    }).map((item) => item.id)).toEqual(["svarog-thread"]);
  });
});
