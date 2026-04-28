import { describe, expect, it } from "vitest";
import type { GoalRun } from "@/lib/goalRuns";
import { buildGoalWorkspaceModel, splitGoalStepTitle } from "./goalWorkspaceModel";

const baseRun: GoalRun = {
  id: "goal-1",
  title: "Ship release",
  goal: "Pin the contract and cut the landing ledger",
  status: "running",
  created_at: 1,
  thread_id: "thread-main",
  root_thread_id: "thread-root",
  active_thread_id: "thread-active",
  execution_thread_ids: ["thread-exec"],
  current_step_index: 0,
  current_step_title: "Pin the contract and cut the landing ledger",
  replan_count: 1,
  child_task_count: 2,
  child_task_ids: ["task-a", "task-b"],
  approval_count: 1,
  total_prompt_tokens: 1200,
  total_completion_tokens: 340,
  estimated_cost_usd: 0.0123,
  current_step_owner_profile: {
    agent_label: "Main agent",
    provider: "openai",
    model: "gpt-5.4",
    reasoning_effort: "high",
  },
  planner_owner_profile: {
    agent_label: "Planner",
    provider: "openai",
    model: "gpt-5.4",
    reasoning_effort: "medium",
  },
  runtime_assignment_list: [
    {
      role_id: "executor",
      enabled: true,
      provider: "openai",
      model: "gpt-5.4",
      reasoning_effort: "high",
      inherit_from_main: false,
    },
  ],
  steps: [
    {
      id: "step-1",
      title: "[HIGH] Pin the contract and cut the landing ledger",
      kind: "reason",
      status: "running",
      instructions: "Stay in the workspace root.",
      summary: "Contract pinned.",
    },
    {
      id: "step-2",
      title: "[LOW] Land the persisted substrate and minimal closed loop",
      kind: "command",
      status: "pending",
      instructions: "",
    },
  ],
  events: [
    {
      id: "event-1",
      timestamp: 1,
      phase: "todo",
      message: "goal todo updated",
      details: "Read harness/types.rs",
      step_index: 0,
      todo_snapshot: [
        { id: "todo-1", content: "Read harness/types.rs", status: "completed", position: 0, step_index: 0 },
      ],
    },
  ],
  dossier: {
    projection_state: "selected",
    summary: "Dossier summary",
    projection_error: null,
    latest_resume_decision: {
      action: "continue",
      reason_code: "healthy",
      reason: "No blockers",
      details: ["All checks fresh"],
      projection_state: "selected",
      decided_at: null,
    },
    units: [
      {
        id: "step-1",
        title: "Contract ledger",
        status: "running",
        execution_binding: "workspace",
        verification_binding: "proof",
        summary: "Ledger proof pending",
        proof_checks: [
          {
            id: "proof-1",
            title: "Review proof matrix",
            state: "pending",
            summary: null,
            evidence_ids: [],
            resolved_at: null,
          },
        ],
        evidence: [],
        report: null,
      },
    ],
    report: null,
  },
};

describe("goalWorkspaceModel", () => {
  it("matches TUI confidence prefixes", () => {
    expect(splitGoalStepTitle("[LOW] Risky step")).toEqual({ confidence: "low", title: "Risky step" });
    expect(splitGoalStepTitle("[MEDIUM] Normal step")).toEqual({ confidence: "medium", title: "Normal step" });
    expect(splitGoalStepTitle("[HIGH] Confident step")).toEqual({ confidence: "high", title: "Confident step" });
    expect(splitGoalStepTitle("Plain step")).toEqual({ confidence: null, title: "Plain step" });
  });

  it("builds the TUI five-region dossier workspace", () => {
    const model = buildGoalWorkspaceModel(baseRun, {
      mode: "dossier",
      expandedStepIds: new Set(["step-1"]),
      promptExpanded: false,
      selectedStepId: "step-1",
      selectedCenterIndex: 0,
    });

    expect(model.summaryTitle).toBe("Goal Mission Control");
    expect(model.tabs.map((tab) => tab.label)).toEqual([
      "Dossier",
      "Files",
      "Progress",
      "Usage",
      "Active agent",
      "Threads",
      "Needs attention",
    ]);
    expect(model.planTitle).toBe("Plan");
    expect(model.planRows.map((row) => row.text)).toContain("Goal Prompt  [Show]");
    expect(model.planRows.map((row) => row.text)).toContain("[thread] Main agent  thread-main");
    expect(model.planRows.map((row) => row.text)).toContain("1. Pin the contract and cut the landing ledger ˄");
    expect(model.planRows.map((row) => row.text)).toContain("[x] Read harness/types.rs");
    expect(model.centerTitle).toBe("Run timeline");
    expect(model.centerRows.map((row) => row.text)).toContain("goal todo updated");
    expect(model.detailTitle).toBe("Dossier");
    expect(model.detailSections.map((section) => section.title)).toContain("Selected Step");
    expect(model.detailSections.map((section) => section.title)).toContain("Execution Dossier");
    expect(model.footerTitle).toBe("Step Actions");
    expect(model.footerSegments.map((segment) => segment.text)).toContain("[Retry step] R");
    expect(model.footerSegments.map((segment) => segment.text)).toContain("[Rerun from here] Shift+R");
  });

  it("switches center and detail panes by TUI mode", () => {
    expect(buildGoalWorkspaceModel(baseRun, { mode: "usage" }).centerTitle).toBe("Usage");
    expect(buildGoalWorkspaceModel(baseRun, { mode: "usage" }).detailTitle).toBe("Usage details");
    expect(buildGoalWorkspaceModel(baseRun, { mode: "active-agent" }).centerTitle).toBe("Active agent");
    expect(buildGoalWorkspaceModel(baseRun, { mode: "threads" }).detailTitle).toBe("Thread details");
    expect(buildGoalWorkspaceModel(baseRun, { mode: "needs-attention" }).centerTitle).toBe("Needs attention");
  });

  it("marks linked threads and projection files as actionable targets", () => {
    const threads = buildGoalWorkspaceModel(baseRun, { mode: "threads" });
    expect(threads.centerRows.find((row) => row.targetThreadId === "thread-active")?.text).toContain("[thread]");

    const files = buildGoalWorkspaceModel(baseRun, {
      mode: "files",
      projectionFiles: [
        {
          relativePath: "dossier.json",
          absolutePath: "/home/example/.tamux/goals/goal-1/dossier.json",
          sizeBytes: 42,
        },
      ],
    });
    expect(files.centerRows[0]).toMatchObject({
      text: "dossier.json",
      targetFilePath: "/home/example/.tamux/goals/goal-1/dossier.json",
    });
    expect(files.detailSections[0].rows.map((row) => row.text)).toContain("Size 42 bytes");
  });
});
