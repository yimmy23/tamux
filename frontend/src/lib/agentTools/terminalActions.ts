import { assessCommandRisk } from "../agentMissionStore";
import { getBridge } from "../bridge";
import { getCanvasBrowserController } from "../canvasBrowserRegistry";
import { useSettingsStore } from "../settingsStore";
import { getTerminalSnapshot } from "../terminalRegistry";
import type { ToolResult } from "./types";
import { resolvePaneIdByRef } from "./workspaceHelpers";

function encodeBase64(text: string): string {
  const bytes = new TextEncoder().encode(text);
  let binary = "";
  for (const byte of bytes) {
    binary += String.fromCharCode(byte);
  }
  return btoa(binary);
}

type ManagedAwaitResult =
  | { status: "finished"; exitCode?: number | null }
  | { status: "approved"; decision: string }
  | { status: "rejected"; message: string }
  | { status: "denied" }
  | { status: "timeout" };

function createManagedCommandAwaiter(
  paneId: string,
  command: string,
  _source: "agent" | "gateway" = "agent",
  timeoutMs = 5 * 60 * 1000,
): { promise: Promise<ManagedAwaitResult>; cancel: () => void } {
  let cancel = () => {};
  const promise = new Promise<ManagedAwaitResult>((resolve) => {
    const amux = getBridge();
    if (!amux?.onTerminalEvent) {
      resolve({ status: "timeout" });
      return;
    }

    const normalized = command.trim();
    let executionId: string | null = null;
    let sawMatchingApproval = false;
    const matchingApprovalIds = new Set<string>();
    let sawCommandStarted = false;
    let settled = false;

    const finish = (result: ManagedAwaitResult) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      unsubscribe?.();
      resolve(result);
    };

    const unsubscribe = amux.onTerminalEvent((event: any) => {
      if (!event || event.paneId !== paneId) return;
      if (event.type === "approval-required") {
        const approvalCommand = String(event.approval?.command ?? "").trim();
        if (approvalCommand === normalized) {
          sawMatchingApproval = true;
          const approvalId = String(event.approval?.approvalId ?? event.approval?.approval_id ?? "").trim();
          if (approvalId) matchingApprovalIds.add(approvalId);
        }
        return;
      }
      if (event.type === "approval-resolved") {
        const decision = String(event.decision ?? "").toLowerCase();
        const approvalId = String(event.approvalId ?? "").trim();
        const matchesApproval = approvalId ? matchingApprovalIds.has(approvalId) : sawMatchingApproval;
        if (matchesApproval && decision === "deny") { finish({ status: "denied" }); return; }
        if (matchesApproval && decision) finish({ status: "approved", decision });
        return;
      }
      if (event.type === "managed-queued") {
        const queuedCommand = String(event.snapshot?.command ?? "").trim();
        if (queuedCommand === normalized) executionId = String(event.executionId ?? "") || null;
        return;
      }
      if (event.type === "managed-rejected") {
        const rejectedExecutionId = String(event.executionId ?? "");
        if (executionId) {
          if (rejectedExecutionId === executionId) finish({ status: "rejected", message: String(event.message ?? "managed command rejected") });
          return;
        }
        finish({ status: "rejected", message: String(event.message ?? "managed command rejected") });
        return;
      }
      if (event.type === "managed-finished") {
        const finishedExecutionId = String(event.executionId ?? "");
        const finishedCommand = String(event.command ?? "").trim();
        if (executionId) {
          if (finishedExecutionId === executionId) finish({ status: "finished", exitCode: event.exitCode ?? null });
          return;
        }
        if (finishedCommand === normalized) finish({ status: "finished", exitCode: event.exitCode ?? null });
        return;
      }
      if (event.type === "command-started") {
        const started = (() => {
          const b64 = String(event.commandB64 ?? "");
          if (!b64) return "";
          try {
            const binary = atob(b64);
            return new TextDecoder().decode(Uint8Array.from(binary, (char) => char.charCodeAt(0))).trim();
          } catch {
            return "";
          }
        })();
        if (started === normalized) sawCommandStarted = true;
        return;
      }
      if (event.type === "command-finished" && sawCommandStarted) {
        finish({ status: "finished", exitCode: event.exitCode ?? null });
      }
    });

    const timer = window.setTimeout(() => finish({ status: "timeout" }), timeoutMs);
    cancel = () => finish({ status: "rejected", message: "managed command wait cancelled" });
  });

  return { promise, cancel };
}

export async function executeReadTerminalContent(
  callId: string,
  name: string,
  paneRef?: string,
  opts?: { include_dom?: boolean },
): Promise<ToolResult> {
  const paneId = resolvePaneIdByRef(paneRef);
  if (!paneId) return { toolCallId: callId, name, content: "Error: No terminal pane found. Open a terminal first or provide a valid pane name/ID." };

  const browserController = getCanvasBrowserController(paneId);
  if (browserController) {
    if (opts?.include_dom) {
      try {
        const snapshot = await browserController.getDomSnapshot();
        return { toolCallId: callId, name, content: `Browser Panel\nURL: ${snapshot.url}\nTitle: ${snapshot.title}\n\n${snapshot.text}` };
      } catch (error: any) {
        return { toolCallId: callId, name, content: `Error reading browser DOM: ${error.message || String(error)}` };
      }
    }
    return { toolCallId: callId, name, content: `Browser Panel\nURL: ${browserController.getUrl()}\nTitle: ${browserController.getTitle()}` };
  }

  const content = getTerminalSnapshot(paneId).trim();
  if (!content) return { toolCallId: callId, name, content: `Pane ${paneId} has no readable terminal content yet.` };
  const maxChars = 16000;
  const output = content.length > maxChars ? `${content.slice(content.length - maxChars)}\n\n[truncated to last ${maxChars} chars]` : content;
  return { toolCallId: callId, name, content: output };
}

export async function executeTerminalCommand(callId: string, name: string, command: string, paneId?: string): Promise<ToolResult> {
  const amux = getBridge();
  if (!amux?.sendTerminalInput && !amux?.executeManagedCommand) {
    return { toolCallId: callId, name, content: "Error: Terminal bridge not available." };
  }
  const targetPaneId = resolvePaneIdByRef(paneId);
  if (!targetPaneId) {
    return { toolCallId: callId, name, content: "Error: No terminal pane found. Open a terminal first or provide a valid pane name/ID." };
  }

  try {
    if (typeof command !== "string" || !command.trim()) {
      return { toolCallId: callId, name, content: "Error: Empty command." };
    }

    const normalizedCommand = command.trim();
    const securityLevel = useSettingsStore.getState().settings.securityLevel;
    const risk = assessCommandRisk(normalizedCommand, securityLevel);

    if (amux?.executeManagedCommand) {
      const managedAwaiter = createManagedCommandAwaiter(targetPaneId, normalizedCommand, "agent");
      try {
        await amux.executeManagedCommand(targetPaneId, {
          command: normalizedCommand,
          rationale: "Agent requested terminal tool execution",
          allowNetwork: useSettingsStore.getState().settings.sandboxNetworkEnabled,
          sandboxEnabled: useSettingsStore.getState().settings.sandboxEnabled,
          securityLevel,
          source: "agent",
        });
      } catch (error) {
        managedAwaiter.cancel();
        throw error;
      }

      const managedResult = await managedAwaiter.promise;
      if (managedResult.status === "finished") {
        return { toolCallId: callId, name, content: `Command finished in pane ${targetPaneId} with exit code ${managedResult.exitCode ?? "unknown"}.` };
      }
      if (managedResult.status === "approved") {
        return { toolCallId: callId, name, content: `Command approval accepted in pane ${targetPaneId} (${managedResult.decision}).` };
      }
      if (managedResult.status === "denied") {
        return { toolCallId: callId, name, content: `Command was denied by approval policy in pane ${targetPaneId}.` };
      }
      if (managedResult.status === "rejected") {
        return { toolCallId: callId, name, content: `Error: Command rejected in pane ${targetPaneId}: ${managedResult.message}` };
      }
      return { toolCallId: callId, name, content: `Command queued in pane ${targetPaneId}, but timed out while waiting for completion.${risk.requiresApproval ? " Approval may still be pending." : ""}` };
    }

    if (risk.requiresApproval) {
      return { toolCallId: callId, name, content: `Error: Managed execution unavailable; blocked risky command (${risk.riskLevel}): ${risk.reasons.join(", ")}` };
    }
    if (amux?.sendTerminalInput) {
      await amux.sendTerminalInput(targetPaneId, encodeBase64(`${normalizedCommand}\r`));
      return { toolCallId: callId, name, content: `Command sent directly to pane ${targetPaneId} (managed policy unavailable).` };
    }
    return { toolCallId: callId, name, content: "Error: No terminal execution path available." };
  } catch (error: any) {
    return { toolCallId: callId, name, content: `Error: ${error.message || String(error)}` };
  }
}

function formatBytes(bytes: number | null | undefined): string {
  if (bytes == null) return "N/A";
  const gb = bytes / (1024 * 1024 * 1024);
  if (gb >= 1) return `${gb.toFixed(1)}GB`;
  const mb = bytes / (1024 * 1024);
  return `${mb.toFixed(0)}MB`;
}

export async function executeGetSystemInfo(callId: string, name: string): Promise<ToolResult> {
  const amux = getBridge();
  if (!amux?.getSystemMonitorSnapshot) {
    return { toolCallId: callId, name, content: "Error: System monitor not available." };
  }
  try {
    const snapshot = await amux.getSystemMonitorSnapshot({ processLimit: 5 });
    const info = [
      `CPU: ${snapshot.cpu?.usagePercent?.toFixed(1) ?? "N/A"}%`,
      `RAM: ${formatBytes(snapshot.memory?.usedBytes)} / ${formatBytes(snapshot.memory?.totalBytes)}`,
      ...(snapshot.gpus?.length > 0
        ? snapshot.gpus.map((gpu: any) => `GPU: ${gpu.name} - ${gpu.utilizationPercent?.toFixed(1) ?? "N/A"}%, VRAM: ${formatBytes(gpu.memoryUsedBytes)} / ${formatBytes(gpu.memoryTotalBytes)}`)
        : []),
      `Top processes: ${snapshot.processes?.map((process: any) => `${process.name} (${process.cpuPercent?.toFixed(1)}%)`).join(", ") || "N/A"}`,
    ].join("\n");
    return { toolCallId: callId, name, content: info };
  } catch (error: any) {
    return { toolCallId: callId, name, content: `Error: ${error.message || String(error)}` };
  }
}
