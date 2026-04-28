import {
  formatGoalRunDuration,
  formatGoalRunStatus,
  type GoalAgentAssignment,
  type GoalRun,
  type GoalRunEvent,
  type GoalRunStep,
  type TodoStatus,
} from "@/lib/goalRuns";

export type GoalWorkspaceMode =
  | "dossier"
  | "files"
  | "progress"
  | "usage"
  | "active-agent"
  | "threads"
  | "needs-attention";

export type GoalWorkspaceTone = "normal" | "muted" | "active" | "success" | "warning" | "danger" | "accent";

export interface GoalWorkspaceRow {
  id: string;
  text: string;
  tone?: GoalWorkspaceTone;
  depth?: number;
  selected?: boolean;
  targetThreadId?: string;
  targetFilePath?: string;
}

export interface GoalWorkspaceSection {
  title: string;
  rows: GoalWorkspaceRow[];
}

export interface GoalWorkspaceModel {
  summaryTitle: string;
  tabs: Array<{ id: GoalWorkspaceMode; label: string; active: boolean }>;
  planTitle: string;
  planRows: GoalWorkspaceRow[];
  centerTitle: string;
  centerRows: GoalWorkspaceRow[];
  detailTitle: string;
  detailSections: GoalWorkspaceSection[];
  footerTitle: string;
  footerSegments: GoalWorkspaceRow[];
}

export interface GoalWorkspaceOptions {
  mode?: GoalWorkspaceMode;
  expandedStepIds?: Set<string>;
  promptExpanded?: boolean;
  selectedStepId?: string | null;
  selectedCenterIndex?: number;
  projectionFiles?: GoalProjectionFile[];
}

export interface GoalProjectionFile {
  relativePath: string;
  absolutePath: string;
  sizeBytes?: number | null;
}

const modeTabs: Array<{ id: GoalWorkspaceMode; label: string; center: string; detail: string }> = [
  { id: "dossier", label: "Dossier", center: "Run timeline", detail: "Dossier" },
  { id: "files", label: "Files", center: "Files", detail: "File details" },
  { id: "progress", label: "Progress", center: "Progress", detail: "Progress details" },
  { id: "usage", label: "Usage", center: "Usage", detail: "Usage details" },
  { id: "active-agent", label: "Active agent", center: "Active agent", detail: "Runtime details" },
  { id: "threads", label: "Threads", center: "Threads", detail: "Thread details" },
  { id: "needs-attention", label: "Needs attention", center: "Needs attention", detail: "Attention details" },
];

export function splitGoalStepTitle(title: string): { confidence: "low" | "medium" | "high" | null; title: string } {
  if (title.startsWith("[LOW]")) return { confidence: "low", title: title.slice(5).trimStart() };
  if (title.startsWith("[MEDIUM]")) return { confidence: "medium", title: title.slice(8).trimStart() };
  if (title.startsWith("[HIGH]")) return { confidence: "high", title: title.slice(6).trimStart() };
  return { confidence: null, title };
}

export function buildGoalWorkspaceModel(run: GoalRun, options: GoalWorkspaceOptions = {}): GoalWorkspaceModel {
  const mode = options.mode ?? "dossier";
  const selectedStep = selectedStepForRun(run, options.selectedStepId);
  const modeMeta = modeTabs.find((tab) => tab.id === mode) ?? modeTabs[0];
  const centerRows = buildCenterRows(run, mode, options.selectedCenterIndex ?? 0, options.projectionFiles ?? []);

  return {
    summaryTitle: "Goal Mission Control",
    tabs: modeTabs.map((tab) => ({ id: tab.id, label: tab.label, active: tab.id === mode })),
    planTitle: "Plan",
    planRows: buildPlanRows(run, options.expandedStepIds ?? new Set(), Boolean(options.promptExpanded), selectedStep?.id ?? null),
    centerTitle: modeMeta.center,
    centerRows,
    detailTitle: modeMeta.detail,
    detailSections: buildDetailSections(run, mode, selectedStep, options.selectedCenterIndex ?? 0, options.projectionFiles ?? []),
    footerTitle: "Step Actions",
    footerSegments: buildFooterSegments(run, selectedStep),
  };
}

function buildPlanRows(run: GoalRun, expandedStepIds: Set<string>, promptExpanded: boolean, selectedStepId: string | null): GoalWorkspaceRow[] {
  const rows: GoalWorkspaceRow[] = [
    {
      id: "goal-prompt",
      text: `Goal Prompt  ${promptExpanded ? "[Hide]" : "[Show]"}`,
      tone: "accent",
    },
  ];

  if (promptExpanded) {
    rows.push({ id: "goal-prompt-body", text: run.goal || "No goal prompt available.", tone: "muted", depth: 1 });
  }

  const mainThread = mainAgentThread(run);
  rows.push(mainThread
    ? { id: "main-thread", text: `[thread] ${mainThread.label}  ${mainThread.threadId}`, tone: "active", targetThreadId: mainThread.threadId }
    : { id: "main-thread-empty", text: "No main agent thread yet.", tone: "muted" });

  const steps = sortedSteps(run);
  if (steps.length > 0) {
    rows.push({ id: "steps-label", text: "Steps:", tone: "accent" });
  }

  for (const [index, step] of steps.entries()) {
    const expanded = expandedStepIds.has(step.id);
    const parsed = splitGoalStepTitle(step.title);
    const confidence = confidenceSymbol(parsed.confidence);
    const status = stepMarkerState(run, step, index);
    rows.push({
      id: `step-${step.id}`,
      text: `${index + 1}. ${parsed.title}${confidence ? ` ${confidence}` : ""}`,
      tone: markerTone(status),
      selected: step.id === selectedStepId,
    });

    if (expanded) {
      for (const todo of todosForStep(run, index)) {
        rows.push({
          id: `todo-${todo.id}`,
          text: `${todoStatusChip(todo.status)} ${todo.content}`,
          tone: todo.status === "completed" ? "success" : todo.status === "blocked" ? "danger" : "muted",
          depth: 1,
        });
      }
    }
  }

  return rows;
}

function buildCenterRows(run: GoalRun, mode: GoalWorkspaceMode, selectedIndex: number, projectionFiles: GoalProjectionFile[]): GoalWorkspaceRow[] {
  switch (mode) {
    case "dossier":
      return timelineRows(run, selectedIndex);
    case "files":
      return fileRows(run, selectedIndex, projectionFiles);
    case "progress":
      return progressRows(run, selectedIndex);
    case "usage":
      return usageRows(run, selectedIndex);
    case "active-agent":
      return activeAgentRows(run, selectedIndex);
    case "threads":
      return threadRows(run, selectedIndex);
    case "needs-attention":
      return attentionRows(run, selectedIndex);
  }
}

function buildDetailSections(run: GoalRun, mode: GoalWorkspaceMode, selectedStep: GoalRunStep | null, selectedCenterIndex: number, projectionFiles: GoalProjectionFile[]): GoalWorkspaceSection[] {
  if (mode === "dossier") return dossierDetails(run, selectedStep, selectedCenterIndex);
  if (mode === "files") return fileDetails(run, selectedCenterIndex, projectionFiles);
  if (mode === "progress") return progressDetails(run, selectedCenterIndex);
  if (mode === "usage") return usageDetails(run, selectedCenterIndex);
  if (mode === "active-agent") return activeAgentDetails(run, selectedCenterIndex);
  if (mode === "threads") return threadDetails(run, selectedCenterIndex);
  return attentionDetails(run, selectedCenterIndex);
}

function dossierDetails(run: GoalRun, selectedStep: GoalRunStep | null, selectedCenterIndex: number): GoalWorkspaceSection[] {
  const sections: GoalWorkspaceSection[] = [];
  if (selectedStep) {
    const parsed = splitGoalStepTitle(selectedStep.title);
    const rows: GoalWorkspaceRow[] = [
      { id: "selected-title", text: `${stepPosition(run, selectedStep) + 1}. ${parsed.title}${confidenceSymbol(parsed.confidence) ? ` ${confidenceSymbol(parsed.confidence)}` : ""}`, tone: "active" },
    ];
    if (selectedStep.instructions) rows.push({ id: "instructions", text: selectedStep.instructions, tone: "muted" });
    if (selectedStep.summary) rows.push({ id: "summary", text: selectedStep.summary, tone: "active" });
    if (selectedStep.error) rows.push({ id: "error", text: selectedStep.error, tone: "danger" });
    sections.push({ title: "Selected Step", rows });
  }

  if (run.dossier) {
    const matchingUnit = selectedStep ? run.dossier.units.find((unit) => unit.id === selectedStep.id) : null;
    const rows: GoalWorkspaceRow[] = [
      { id: "projection", text: `Projection ${matchingUnit?.status || run.dossier.projection_state}`, tone: "active" },
    ];
    const summary = matchingUnit?.summary || run.dossier.summary;
    if (summary) rows.push({ id: "dossier-summary", text: summary, tone: "active" });
    if (run.dossier.projection_error) rows.push({ id: "projection-error", text: run.dossier.projection_error, tone: "danger" });
    sections.push({ title: "Execution Dossier", rows });
  }

  const taskRows = relatedTaskRows(run, selectedStep);
  sections.push({ title: "Related Tasks", rows: taskRows.length ? taskRows : [{ id: "no-tasks", text: "No related tasks.", tone: "muted" }] });

  const selectedEvent = [...(run.events ?? [])].reverse()[selectedCenterIndex];
  if (selectedEvent) {
    const rows: GoalWorkspaceRow[] = [{ id: selectedEvent.id, text: selectedEvent.message, tone: "active" }];
    if (selectedEvent.details) rows.push({ id: `${selectedEvent.id}-details`, text: selectedEvent.details, tone: "muted" });
    for (const todo of selectedEvent.todo_snapshot ?? []) {
      rows.push({ id: `${selectedEvent.id}-${todo.id}`, text: `${todoStatusChip(todo.status)} ${todo.content}`, tone: "muted" });
    }
    sections.push({ title: "Selected Timeline Item", rows });
  }

  return sections;
}

function progressDetails(run: GoalRun, selectedIndex: number): GoalWorkspaceSection[] {
  const rows = progressRows(run, selectedIndex);
  const selected = rows[selectedIndex] ?? rows[0];
  const details: GoalWorkspaceRow[] = selected ? [selected] : [{ id: "empty", text: "No progress data available.", tone: "muted" }];
  if (run.dossier?.latest_resume_decision) {
    details.push({ id: "resume", text: `${run.dossier.latest_resume_decision.action} via ${run.dossier.latest_resume_decision.reason_code}`, tone: "active" });
    if (run.dossier.latest_resume_decision.reason) details.push({ id: "resume-reason", text: run.dossier.latest_resume_decision.reason, tone: "muted" });
  }
  return [{ title: selected?.text.includes("Resume") ? "Resume Decision" : "Execution Dossier", rows: details }];
}

function usageDetails(run: GoalRun, selectedIndex: number): GoalWorkspaceSection[] {
  const rows = usageRows(run, selectedIndex);
  return [{ title: selectedIndex <= 0 ? "Goal Usage" : "Model Usage", rows: rows.length ? rows : [{ id: "empty", text: "No usage data available.", tone: "muted" }] }];
}

function activeAgentDetails(run: GoalRun, selectedIndex: number): GoalWorkspaceSection[] {
  const rows = activeAgentRows(run, selectedIndex);
  return [{ title: selectedIndex === 0 ? "Current Owner" : "Runtime Assignment", rows: rows.length ? rows : [{ id: "empty", text: "No runtime owner metadata.", tone: "muted" }] }];
}

function threadDetails(run: GoalRun, selectedIndex: number): GoalWorkspaceSection[] {
  const rows = threadRows(run, selectedIndex);
  const selected = rows[selectedIndex] ?? rows[0];
  return [{ title: "Thread", rows: selected ? [selected, { id: `${selected.id}-open`, text: "[open] open linked thread", tone: "accent", targetThreadId: selected.targetThreadId }] : [{ id: "empty", text: "No linked threads available.", tone: "muted" }] }];
}

function attentionDetails(run: GoalRun, selectedIndex: number): GoalWorkspaceSection[] {
  const rows = attentionRows(run, selectedIndex);
  return [{ title: "Status", rows: rows.length ? rows : [{ id: "empty", text: "No blockers or review items.", tone: "muted" }] }];
}

function timelineRows(run: GoalRun, selectedIndex: number): GoalWorkspaceRow[] {
  const events = [...(run.events ?? [])].reverse();
  if (events.length === 0) return [{ id: "empty", text: "Waiting for run events.", tone: "muted" }];
  return events.flatMap((event, index) => eventRows(event, index === selectedIndex));
}

function eventRows(event: GoalRunEvent, selected: boolean): GoalWorkspaceRow[] {
  const rows: GoalWorkspaceRow[] = [{ id: event.id, text: event.message || "event", tone: eventTone(event), selected }];
  if (event.details) rows.push({ id: `${event.id}-details`, text: event.details, tone: "muted", depth: 1, selected });
  for (const todo of event.todo_snapshot ?? []) {
    rows.push({ id: `${event.id}-${todo.id}`, text: `${todoStatusChip(todo.status)} ${todo.content}`, tone: "muted", depth: 1, selected });
  }
  return rows;
}

function fileRows(run: GoalRun, selectedIndex: number, projectionFiles: GoalProjectionFile[]): GoalWorkspaceRow[] {
  if (projectionFiles.length > 0) {
    return projectionFiles.map((file, index) => ({
      id: `file-${file.relativePath}`,
      text: file.relativePath,
      selected: index === selectedIndex,
      targetFilePath: file.absolutePath,
      tone: index === selectedIndex ? "accent" : "normal",
    }));
  }
  const entries: GoalWorkspaceRow[] = [
    ...(run.generated_skill_path ? [{
      id: "generated-skill",
      text: `Generated skill: ${run.generated_skill_path}`,
      targetFilePath: run.generated_skill_path,
    }] : []),
    ...((run.memory_updates ?? []).map((entry, index) => ({ id: `memory-${index}`, text: `Memory update: ${entry}`, tone: "muted" as const }))),
  ];
  return entries.length ? markSelected(entries, selectedIndex) : [{ id: "empty", text: "No goal files yet.", tone: "muted" }];
}

function fileDetails(run: GoalRun, selectedIndex: number, projectionFiles: GoalProjectionFile[]): GoalWorkspaceSection[] {
  const rows = fileRows(run, selectedIndex, projectionFiles);
  const selected = rows[selectedIndex] ?? rows[0];
  if (!selected || selected.id === "empty") return [{ title: "Selected File", rows }];
  const metadata: GoalWorkspaceRow[] = [selected];
  if (selected.targetFilePath) metadata.push({ id: `${selected.id}-path`, text: `Path ${selected.targetFilePath}`, tone: "muted" });
  const projection = projectionFiles.find((file) => file.absolutePath === selected.targetFilePath);
  if (typeof projection?.sizeBytes === "number") metadata.push({ id: `${selected.id}-size`, text: `Size ${projection.sizeBytes} bytes`, tone: "muted" });
  if (selected.targetFilePath) metadata.push({ id: `${selected.id}-open`, text: "Press Enter or click to open the preview.", tone: "accent", targetFilePath: selected.targetFilePath });
  return [{ title: "Selected File", rows: metadata }];
}

function progressRows(run: GoalRun, selectedIndex: number): GoalWorkspaceRow[] {
  const rows: GoalWorkspaceRow[] = [];
  if (run.dossier) rows.push({ id: "dossier", text: "[dossier] Execution Dossier", tone: "active" });
  if (run.dossier?.latest_resume_decision) rows.push({ id: "resume", text: "[resume] Resume Decision", tone: "active" });
  for (const unit of run.dossier?.units ?? []) rows.push({ id: unit.id, text: `[${unit.status}] ${unit.title}`, tone: unit.status === "completed" ? "success" : "active" });
  return markSelected(rows.length ? rows : [{ id: "empty", text: "No progress data available.", tone: "muted" }], selectedIndex);
}

function usageRows(run: GoalRun, selectedIndex: number): GoalWorkspaceRow[] {
  const rows: GoalWorkspaceRow[] = [];
  if ((run.total_prompt_tokens ?? 0) > 0 || (run.total_completion_tokens ?? 0) > 0 || run.estimated_cost_usd != null) {
    rows.push({ id: "total", text: `Goal total  prompt ${formatCount(run.total_prompt_tokens ?? 0)}  completion ${formatCount(run.total_completion_tokens ?? 0)}${run.estimated_cost_usd != null ? `  cost ${formatCost(run.estimated_cost_usd)}` : ""}`, tone: "active" });
  }
  for (const usage of run.model_usage ?? []) rows.push({ id: `${usage.provider}-${usage.model}`, text: `${usage.provider}/${usage.model}  ${usage.request_count} req  in ${formatCount(usage.prompt_tokens)}  out ${formatCount(usage.completion_tokens)}${usage.duration_ms ? `  ${formatGoalRunDuration(usage.duration_ms)}` : ""}` });
  for (const assignment of runtimeAssignments(run)) rows.push({ id: `role-${assignment.role_id}`, text: `Role ${assignment.role_id}  ${assignment.inherit_from_main ? "inherits main" : `${assignment.provider}/${assignment.model}`}` });
  return markSelected(rows.length ? rows : [{ id: "empty", text: "No usage data available.", tone: "muted" }], selectedIndex);
}

function activeAgentRows(run: GoalRun, selectedIndex: number): GoalWorkspaceRow[] {
  const rows: GoalWorkspaceRow[] = [];
  if (run.current_step_owner_profile) rows.push({ id: "current", text: `Current ${run.current_step_owner_profile.agent_label}`, tone: "active" });
  if (run.planner_owner_profile) rows.push({ id: "planner", text: `Planner ${run.planner_owner_profile.agent_label}`, tone: "active" });
  for (const assignment of runtimeAssignments(run)) rows.push({ id: `assignment-${assignment.role_id}`, text: `[${assignment.role_id}] ${assignment.model}`, tone: assignment.enabled ? "active" : "muted" });
  for (const threadId of goalThreadTargets(run)) rows.push({ id: `thread-${threadId}`, text: `[thread] ${threadId}`, tone: "accent", targetThreadId: threadId });
  return markSelected(rows.length ? rows : [{ id: "empty", text: "No runtime owner metadata.", tone: "muted" }], selectedIndex);
}

function threadRows(run: GoalRun, selectedIndex: number): GoalWorkspaceRow[] {
  const entries = goalThreadEntries(run);
  return markSelected(entries.length
    ? entries.map((entry) => ({ id: entry.threadId, text: `[thread] ${entry.label}  ${entry.threadId}`, tone: "accent" as const, targetThreadId: entry.threadId }))
    : [{ id: "empty", text: "No linked threads available.", tone: "muted" }], selectedIndex);
}

function attentionRows(run: GoalRun, selectedIndex: number): GoalWorkspaceRow[] {
  const rows: GoalWorkspaceRow[] = [];
  if (run.last_error) rows.push({ id: "last-error", text: "Last error available", tone: "danger" });
  if (run.dossier?.projection_error) rows.push({ id: "projection-error", text: "Projection error available", tone: "danger" });
  rows.push({ id: "approvals", text: `Approvals ${run.approval_count ?? 0}`, tone: "active" });
  rows.push({ id: "status", text: `Status ${formatGoalRunStatus(run.status)}`, tone: "active" });
  return markSelected(rows, selectedIndex);
}

function buildFooterSegments(run: GoalRun, selectedStep: GoalRunStep | null): GoalWorkspaceRow[] {
  const segments: GoalWorkspaceRow[] = [];
  if (selectedStep) {
    const parsed = splitGoalStepTitle(selectedStep.title);
    segments.push({ id: "step", text: `${stepPosition(run, selectedStep) + 1}. ${parsed.title}${confidenceSymbol(parsed.confidence) ? ` ${confidenceSymbol(parsed.confidence)}` : ""}`, tone: "active" });
  } else {
    segments.push({ id: "prompt", text: "Goal Prompt", tone: "active" });
  }
  if (["queued", "planning", "running", "awaiting_approval", "paused"].includes(run.status)) {
    segments.push({ id: "toggle", text: `[${run.status === "paused" ? "Resume" : "Pause"}] Ctrl+S`, tone: run.status === "paused" ? "success" : "warning" });
  }
  segments.push({ id: "actions", text: "[Actions] A", tone: "accent" });
  segments.push({ id: "retry", text: "[Retry step] R", tone: "warning" });
  segments.push({ id: "rerun", text: "[Rerun from here] Shift+R", tone: "danger" });
  segments.push({ id: "refresh", text: "[Refresh] Ctrl+R", tone: "accent" });
  return segments;
}

function selectedStepForRun(run: GoalRun, selectedStepId?: string | null): GoalRunStep | null {
  const steps = sortedSteps(run);
  return steps.find((step) => step.id === selectedStepId)
    ?? (typeof run.current_step_index === "number" ? steps[run.current_step_index] : null)
    ?? steps[0]
    ?? null;
}

function sortedSteps(run: GoalRun): GoalRunStep[] {
  return [...(run.steps ?? [])].sort((a, b) => (a.position ?? 0) - (b.position ?? 0));
}

function stepPosition(run: GoalRun, step: GoalRunStep): number {
  const index = sortedSteps(run).findIndex((entry) => entry.id === step.id);
  return index >= 0 ? index : 0;
}

function todosForStep(run: GoalRun, stepIndex: number) {
  for (let index = (run.events ?? []).length - 1; index >= 0; index -= 1) {
    const todos = run.events?.[index]?.todo_snapshot?.filter((todo) => todo.step_index === stepIndex) ?? [];
    if (todos.length > 0) return todos;
  }
  return [];
}

function mainAgentThread(run: GoalRun): { label: string; threadId: string } | null {
  if (run.thread_id) return { label: "Main agent", threadId: run.thread_id };
  if (run.root_thread_id) return { label: run.planner_owner_profile ? `Main agent (${run.planner_owner_profile.agent_label})` : "Main agent", threadId: run.root_thread_id };
  if (run.active_thread_id) return { label: run.current_step_owner_profile ? `Main agent (${run.current_step_owner_profile.agent_label})` : "Main agent", threadId: run.active_thread_id };
  const firstExecution = run.execution_thread_ids?.[0];
  return firstExecution ? { label: "Main agent", threadId: firstExecution } : null;
}

function goalThreadTargets(run: GoalRun): string[] {
  return [run.active_thread_id, run.root_thread_id, run.thread_id, ...(run.execution_thread_ids ?? [])]
    .filter((entry): entry is string => Boolean(entry && entry.trim()))
    .filter((entry, index, list) => list.indexOf(entry) === index);
}

function goalThreadEntries(run: GoalRun): Array<{ label: string; threadId: string }> {
  const entries: Array<{ label: string; threadId: string }> = [];
  const push = (label: string, threadId?: string | null) => {
    if (threadId && !entries.some((entry) => entry.threadId === threadId)) entries.push({ label, threadId });
  };
  push(run.current_step_owner_profile?.agent_label ?? "Main agent", run.active_thread_id);
  push(run.planner_owner_profile?.agent_label ?? "Planner", run.root_thread_id);
  push("Goal thread", run.thread_id);
  for (const [index, threadId] of (run.execution_thread_ids ?? []).entries()) push(index === 0 ? run.current_step_owner_profile?.agent_label ?? "Execution 1" : `Execution ${index + 1}`, threadId);
  return entries;
}

function runtimeAssignments(run: GoalRun): GoalAgentAssignment[] {
  return (run.runtime_assignment_list?.length ? run.runtime_assignment_list : run.launch_assignment_snapshot) ?? [];
}

function relatedTaskRows(run: GoalRun, selectedStep: GoalRunStep | null): GoalWorkspaceRow[] {
  const ids = selectedStep?.task_id ? [selectedStep.task_id] : run.child_task_ids ?? [];
  return ids.map((id) => ({ id, text: `[task] ${id}`, tone: "accent" }));
}

function stepMarkerState(run: GoalRun, step: GoalRunStep, index: number): "pending" | "completed" | "running" | "error" {
  if (step.status === "completed") return "completed";
  if (step.status === "failed" || Boolean(step.error?.trim())) return "error";
  if (run.current_step_index === index || step.status === "running" || step.status === "planning" || step.status === "awaiting_approval") return "running";
  return "pending";
}

function markerTone(state: ReturnType<typeof stepMarkerState>): GoalWorkspaceTone {
  if (state === "completed") return "success";
  if (state === "running") return "accent";
  if (state === "error") return "danger";
  return "normal";
}

function confidenceSymbol(confidence: ReturnType<typeof splitGoalStepTitle>["confidence"]): string {
  if (confidence === "low") return "˅";
  if (confidence === "medium") return "=";
  if (confidence === "high") return "˄";
  return "";
}

function todoStatusChip(status: TodoStatus): string {
  if (status === "in_progress") return "[~]";
  if (status === "completed") return "[x]";
  if (status === "blocked") return "[!]";
  return "[ ]";
}

function eventTone(event: GoalRunEvent): GoalWorkspaceTone {
  if (event.phase.includes("error") || event.message.toLowerCase().includes("failed")) return "danger";
  if (event.phase.includes("todo")) return "warning";
  return "normal";
}

function markSelected(rows: GoalWorkspaceRow[], selectedIndex: number): GoalWorkspaceRow[] {
  return rows.map((row, index) => ({ ...row, selected: index === selectedIndex }));
}

function formatCount(value: number): string {
  return Math.round(value).toLocaleString();
}

function formatCost(value: number): string {
  return Math.abs(value) >= 1 ? `$${value.toFixed(2)}` : `$${value.toFixed(4)}`;
}
