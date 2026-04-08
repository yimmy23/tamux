import type { Dispatch, SetStateAction } from "react";
import { buildHydratedRemoteThread, useAgentStore } from "@/lib/agentStore";
import type { AgentMessage, AgentProviderConfig, AgentThread, AgentTodoItem } from "@/lib/agentStore";
import { useAgentMissionStore } from "@/lib/agentMissionStore";
import { getAgentBridge } from "@/lib/agentDaemonConfig";
import { fetchThreadTodos } from "@/lib/agentTodos";
import { useWorkspaceStore } from "@/lib/workspaceStore";
import type { GoalRun } from "@/lib/goalRuns";
import type { WelesHealthState } from "@/lib/agentStore/types";

export function normalizeBridgePayload(payload: any) {
  if (payload && typeof payload === "object" && "data" in payload) {
    return payload.data ?? {};
  }
  return payload ?? {};
}

function parseWorkflowDetails(details: string | null) {
  if (!details) {
    return null;
  }
  try {
    const parsed = JSON.parse(details);
    return parsed && typeof parsed === "object" ? parsed : null;
  } catch {
    return null;
  }
}

function formatSkillWorkflowNotice(
  kind: string,
  message: string | null,
  details: string | null,
): { kind: string; message: string | null } {
  const parsed = parseWorkflowDetails(details);
  const recommendedSkill =
    typeof parsed?.recommended_skill === "string" ? parsed.recommended_skill : null;
  const confidenceTier =
    typeof parsed?.confidence_tier === "string" ? parsed.confidence_tier : null;
  const recommendedAction =
    typeof parsed?.recommended_action === "string" ? parsed.recommended_action : null;
  const skipRationale =
    typeof parsed?.skip_rationale === "string" ? parsed.skip_rationale : null;

  if (kind === "skill-preflight") {
    const nextKind = confidenceTier === "strong"
      ? "skill-discovery-required"
      : "skill-discovery-recommended";
    const summary = [
      confidenceTier === "strong" ? "Skill gate required" : "Skill guidance ready",
      recommendedSkill ? `skill=${recommendedSkill}` : null,
      confidenceTier ? `confidence=${confidenceTier}` : null,
      recommendedAction ? `next=${recommendedAction}` : null,
    ].filter(Boolean).join(" | ");
    return { kind: nextKind, message: summary || message };
  }

  if (kind === "skill-gate") {
    const summary = [
      "Skill gate blocked progress",
      recommendedSkill ? `skill=${recommendedSkill}` : null,
      recommendedAction ? `next=${recommendedAction}` : null,
    ].filter(Boolean).join(" | ");
    return { kind: "skill-discovery-required", message: summary || message };
  }

  if (kind === "skill-discovery-skipped") {
    const summary = [
      "Skill recommendation skipped",
      recommendedSkill ? `skill=${recommendedSkill}` : null,
      skipRationale ? `why=${skipRationale}` : null,
    ].filter(Boolean).join(" | ");
    return { kind, message: summary || message };
  }

  if (kind === "skill-community-scout") {
    const candidates = Array.isArray(parsed?.candidates) ? parsed.candidates.length : null;
    const timeout =
      typeof parsed?.community_preapprove_timeout_secs === "number"
        ? parsed.community_preapprove_timeout_secs
        : null;
    const summary = [
      "Community scout update",
      typeof candidates === "number" ? `candidates=${candidates}` : null,
      typeof timeout === "number" ? `timeout=${timeout}s` : null,
    ].filter(Boolean).join(" | ");
    return { kind, message: summary || message };
  }

  return { kind, message };
}

export function appendDaemonSystemMessage(content: string, threadId: string | null) {
  if (!threadId) return;
  useAgentStore.getState().addMessage(threadId, {
    role: "system",
    content,
    inputTokens: 0,
    outputTokens: 0,
    totalTokens: 0,
    isCompactionSummary: false,
  });
}

export function recordDaemonWorkflowNotice({
  event,
  activePaneId,
  activeWorkspace,
}: {
  event: any;
  activePaneId: string | null;
  activeWorkspace: ReturnType<ReturnType<typeof useWorkspaceStore.getState>["activeWorkspace"]>;
}) {
  const daemonThreadId = typeof event?.thread_id === "string" ? event.thread_id : null;
  const localThreadId = useAgentStore.getState().threads.find((thread) => thread.daemonThreadId === daemonThreadId)?.id ?? null;
  const thread = localThreadId
    ? useAgentStore.getState().threads.find((entry) => entry.id === localThreadId)
    : undefined;
  const paneId = thread?.paneId ?? activePaneId ?? "agent";
  const workspaceId = thread?.workspaceId ?? activeWorkspace?.id ?? null;
  const surfaceId = thread?.surfaceId ?? activeWorkspace?.surfaces?.[0]?.id ?? null;
  const rawKind = typeof event?.kind === "string" ? event.kind : "tool-call";
  const rawMessage = typeof event?.message === "string" ? event.message : null;
  const details = typeof event?.details === "string" ? event.details : null;
  const normalized = formatSkillWorkflowNotice(rawKind, rawMessage, details);
  const kind = normalized.kind;
  const message = normalized.message;

  if (kind === "transport-fallback" && details) {
    try {
      const parsed = JSON.parse(details);
      const provider = typeof parsed?.provider === "string" ? parsed.provider : null;
      const toTransport = parsed?.to === "chat_completions" ? "chat_completions" : null;
      if (provider && toTransport) {
        const currentSettings = useAgentStore.getState().agentSettings;
        const currentConfig = currentSettings[provider as keyof typeof currentSettings];
        if (currentConfig && typeof currentConfig === "object" && "base_url" in currentConfig) {
          useAgentStore.getState().updateAgentSetting(
            provider as keyof typeof currentSettings,
            {
              ...(currentConfig as AgentProviderConfig),
              api_transport: toTransport,
            } as any,
          );
        }
      }
    } catch {
      // Best-effort notice handling.
    }
  }

  useAgentMissionStore.getState().recordOperationalEvent({
    paneId,
    workspaceId,
    surfaceId,
    sessionId: daemonThreadId,
    kind: kind as any,
    command: kind,
    message: message ?? (details ? details : null),
  });
}

export async function reloadDaemonThreadIntoLocalState({
  daemonThreadId,
  setThreadTodos,
  setDaemonTodosByThread,
}: {
  daemonThreadId: string;
  setThreadTodos: (threadId: string, todos: AgentTodoItem[]) => void;
  setDaemonTodosByThread: Dispatch<SetStateAction<Record<string, AgentTodoItem[]>>>;
}) {
  const amux = getAgentBridge();
  if (!amux?.agentGetThread) return;

  const localThreadId = useAgentStore.getState().threads.find(
    (thread) => thread.daemonThreadId === daemonThreadId,
  )?.id;
  if (!localThreadId) return;

  const remoteThread = await amux.agentGetThread(daemonThreadId).catch(() => null) as any;
  const hydrated = buildHydratedRemoteThread(
    (remoteThread ?? {}) as any,
    remoteThread?.agent_name ?? "assistant",
  );
  if (!hydrated) return;

  const reloadedThread = {
    ...hydrated.thread,
    id: localThreadId,
    daemonThreadId,
  } as AgentThread;
  const reloadedMessages = hydrated.messages.map((message) => ({
    ...message,
    threadId: localThreadId,
  })) as AgentMessage[];

  useAgentStore.setState((state) => ({
    threads: state.threads.map((thread) => thread.id === localThreadId ? reloadedThread : thread),
    messages: {
      ...state.messages,
      [localThreadId]: reloadedMessages,
    },
  }));

  const todos = await fetchThreadTodos(daemonThreadId).catch(() => []);
  setThreadTodos(localThreadId, todos);
  setDaemonTodosByThread((current) => ({ ...current, [daemonThreadId]: todos }));
}

export function syncWelesHealth(
  event: any,
  setWelesHealth: Dispatch<SetStateAction<WelesHealthState | null>>,
  appendSystemMessage: (content: string) => void,
) {
  const state = typeof event.state === "string" ? event.state : "healthy";
  const reason = typeof event.reason === "string" ? event.reason : undefined;
  const checkedAt = typeof event.checked_at === "number" ? event.checked_at : Date.now();
  const nextHealth = { state, reason, checkedAt };
  setWelesHealth(nextHealth);
  if (state === "degraded") {
    appendSystemMessage(`WELES degraded\n\n${reason || "Daemon vitality checks require attention."}`);
  }
}

export function refreshGoalRuns(setGoalRunsForTrace: Dispatch<SetStateAction<GoalRun[]>>) {
  return (runs: GoalRun[]) => setGoalRunsForTrace(runs);
}
