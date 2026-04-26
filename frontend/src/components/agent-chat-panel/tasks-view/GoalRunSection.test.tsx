import { Children, isValidElement, type ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";
import type { GoalRun } from "../../../lib/goalRuns";
import type { AgentRun } from "../../../lib/agentRuns";
import { GoalRunDetail } from "./GoalRunSection";

function resolveTree(node: ReactNode): ReactNode {
  if (node == null || typeof node === "boolean" || typeof node === "string" || typeof node === "number") {
    return node;
  }
  if (Array.isArray(node)) {
    return node.map((child) => resolveTree(child));
  }
  if (!isValidElement(node)) {
    return node;
  }
  if (typeof node.type === "function") {
    return resolveTree(node.type(node.props));
  }

  return {
    ...node,
    props: {
      ...node.props,
      children: Children.toArray(node.props.children).map((child) => resolveTree(child)),
    },
  };
}

function elementText(node: ReactNode): string {
  if (node == null || typeof node === "boolean") {
    return "";
  }
  if (typeof node === "string" || typeof node === "number") {
    return String(node);
  }
  if (Array.isArray(node)) {
    return node.map((child) => elementText(child)).join("");
  }
  if (!isValidElement(node)) {
    return "";
  }
  return elementText(node.props.children);
}

function makeGoalRun(overrides: Partial<GoalRun> = {}): GoalRun {
  return {
    id: "goal-usage",
    title: "Token accounting",
    goal: "Show model usage",
    status: "completed",
    created_at: 1_700_000_000_000,
    completed_at: 1_700_000_120_000,
    replan_count: 0,
    total_prompt_tokens: 1234,
    total_completion_tokens: 567,
    estimated_cost_usd: 0.0425,
    duration_ms: 120_000,
    model_usage: [
      {
        provider: "openrouter",
        model: "anthropic/claude-sonnet-4",
        request_count: 2,
        prompt_tokens: 1000,
        completion_tokens: 500,
        estimated_cost_usd: 0.04,
        duration_ms: 90_000,
      },
      {
        provider: "openai",
        model: "gpt-5.4-mini",
        request_count: 1,
        prompt_tokens: 234,
        completion_tokens: 67,
        estimated_cost_usd: 0.0025,
        duration_ms: 30_000,
      },
    ],
    planner_owner_profile: {
      agent_label: "Svarog",
      provider: "openai",
      model: "gpt-5.4",
    },
    runtime_assignment_list: [
      {
        role_id: "svarog",
        enabled: true,
        provider: "openai",
        model: "gpt-5.4",
        inherit_from_main: false,
      },
      {
        role_id: "weles",
        enabled: true,
        provider: "openrouter",
        model: "anthropic/claude-sonnet-4",
        reasoning_effort: "high",
        inherit_from_main: false,
      },
    ],
    ...overrides,
  };
}

function makeAgentRun(overrides: Partial<AgentRun> = {}): AgentRun {
  return {
    id: "run-1",
    task_id: "task-1",
    kind: "subagent",
    classification: "coding",
    title: "Verifier pass",
    description: "Review the goal output",
    status: "completed",
    priority: "normal",
    progress: 100,
    created_at: 1_700_000_060_000,
    source: "subagent",
    goal_run_id: "goal-usage",
    parent_task_id: "task-root",
    ...overrides,
  };
}

describe("GoalRunDetail", () => {
  it("renders aggregate and per-model usage statistics", () => {
    const resolved = resolveTree(
      <GoalRunDetail
        goalRun={makeGoalRun()}
        agentRuns={[makeAgentRun()]}
        busy={false}
        onRetryStep={vi.fn()}
        onRerunFromStep={vi.fn()}
      />,
    );

    const text = elementText(resolved);

    expect(text).toContain("Usage");
    expect(text).toContain("Prompt Tokens");
    expect(text).toContain("1,234");
    expect(text).toContain("Completion Tokens");
    expect(text).toContain("567");
    expect(text).toContain("$0.0425");
    expect(text).toContain("openrouter");
    expect(text).toContain("anthropic/claude-sonnet-4");
    expect(text).toContain("2 req");
    expect(text).toContain("90s");
    expect(text).toContain("openai");
    expect(text).toContain("gpt-5.4-mini");
    expect(text).toContain("Agents");
    expect(text).toContain("Planner");
    expect(text).toContain("Svarog");
    expect(text).toContain("weles");
    expect(text).toContain("Verifier pass");
    expect(text).toContain("subagent");
  });

  it("hides usage when no token or model statistics exist", () => {
    const resolved = resolveTree(
      <GoalRunDetail
        goalRun={makeGoalRun({
          total_prompt_tokens: null,
          total_completion_tokens: null,
          estimated_cost_usd: null,
          model_usage: [],
        })}
        busy={false}
        onRetryStep={vi.fn()}
        onRerunFromStep={vi.fn()}
      />,
    );

    expect(elementText(resolved)).not.toContain("Usage");
  });
});
