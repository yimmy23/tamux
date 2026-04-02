import { useCallback, useEffect, useRef } from "react";
import type { MutableRefObject } from "react";
import type { Terminal } from "@xterm/xterm";
import { getBridge } from "@/lib/bridge";
import type { OperationalEvent } from "@/lib/agentMissionStore";
import type { TerminalSendOptions } from "@/lib/terminalRegistry";
import { useWorkspaceStore } from "@/lib/workspaceStore";
import { findLeaf } from "@/lib/bspTree";
import {
  cloneSessionForDuplication,
  queuePaneBootstrapCommand,
  resolveDuplicateActiveBootstrapCommand,
  resolveDuplicateBootstrapCommand,
  resolveDuplicateSourceSessionId,
} from "@/lib/paneDuplication";
import { encodeTextToBase64, wrapBracketedPaste } from "./utils";

type InputSequenceState = {
  inEscape: boolean;
  inCsi: boolean;
  inOsc: boolean;
  oscEscape: boolean;
};

type CommandLogEntry = {
  command: string;
  path: string;
  cwd: string | null;
  workspaceId: string | null;
  surfaceId: string | null;
  paneId: string;
};

type TerminalInputSettings = {
  bracketedPaste: boolean;
  autoCopyOnSelect: boolean;
  sandboxNetworkEnabled: boolean;
  sandboxEnabled: boolean;
  securityLevel: "highest" | "moderate" | "lowest" | "yolo";
};

export function useTerminalInput({
  paneId,
  paneName,
  paneWorkspaceId,
  paneSurfaceId,
  paneWorkspaceCwd,
  settings,
  operationalEvents,
  splitActive,
  termRef,
  requestedSessionIdRef,
  sessionReadyRef,
  addCommandLogEntry,
}: {
  paneId: string;
  paneName: string;
  paneWorkspaceId?: string;
  paneSurfaceId?: string;
  paneWorkspaceCwd?: string;
  settings: TerminalInputSettings;
  operationalEvents: OperationalEvent[];
  splitActive: (direction: "horizontal" | "vertical", paneName?: string, options?: { sessionId?: string | null; paneIcon?: string }) => void;
  termRef: MutableRefObject<Terminal | null>;
  requestedSessionIdRef: MutableRefObject<string | undefined>;
  sessionReadyRef: MutableRefObject<boolean>;
  addCommandLogEntry: (entry: CommandLogEntry) => void;
}) {
  const platformRef = useRef("linux");
  const commandBufferRef = useRef("");
  const inputSequenceStateRef = useRef<InputSequenceState>({
    inEscape: false,
    inCsi: false,
    inOsc: false,
    oscEscape: false,
  });
  const bracketedPasteRef = useRef(settings.bracketedPaste);
  const autoCopyOnSelectRef = useRef(settings.autoCopyOnSelect);
  const lastShellCommandRef = useRef<{ command: string; timestamp: number } | null>(null);
  const commandPathRef = useRef("human-typed");

  useEffect(() => {
    bracketedPasteRef.current = settings.bracketedPaste;
    autoCopyOnSelectRef.current = settings.autoCopyOnSelect;
  }, [settings.autoCopyOnSelect, settings.bracketedPaste]);

  useEffect(() => {
    let disposed = false;
    const amux = getBridge();

    void amux?.getPlatform?.().then((value: string) => {
      if (!disposed && typeof value === "string" && value) {
        platformRef.current = value;
      }
    });

    return () => {
      disposed = true;
    };
  }, []);

  const commitCommandBuffer = useCallback(() => {
    const command = commandBufferRef.current.replace(/\s+/g, " ").trim();
    commandBufferRef.current = "";

    if (!command) return;

    const lastShellCommand = lastShellCommandRef.current;
    if (
      lastShellCommand
      && lastShellCommand.command === command
      && Date.now() - lastShellCommand.timestamp <= 1500
    ) {
      return;
    }

    addCommandLogEntry({
      command,
      path: commandPathRef.current,
      cwd: paneWorkspaceCwd ?? null,
      workspaceId: paneWorkspaceId ?? null,
      surfaceId: paneSurfaceId ?? null,
      paneId,
    });
    commandPathRef.current = "human-typed";
  }, [addCommandLogEntry, paneId, paneSurfaceId, paneWorkspaceCwd, paneWorkspaceId]);

  const trackInput = useCallback((text: string) => {
    const sequenceState = inputSequenceStateRef.current;

    for (const char of text) {
      const code = char.charCodeAt(0);

      if (sequenceState.inOsc) {
        if (sequenceState.oscEscape) {
          sequenceState.oscEscape = false;
          if (char === "\\") {
            sequenceState.inOsc = false;
          }
          continue;
        }

        if (char === "\u0007") {
          sequenceState.inOsc = false;
          continue;
        }

        if (char === "\u001b") {
          sequenceState.oscEscape = true;
        }
        continue;
      }

      if (sequenceState.inCsi) {
        if (code >= 0x40 && code <= 0x7e) {
          sequenceState.inCsi = false;
        }
        continue;
      }

      if (sequenceState.inEscape) {
        sequenceState.inEscape = false;
        if (char === "[") {
          sequenceState.inCsi = true;
        } else if (char === "]") {
          sequenceState.inOsc = true;
          sequenceState.oscEscape = false;
        }
        continue;
      }

      if (char === "\u001b") {
        sequenceState.inEscape = true;
        continue;
      }

      if (char === "\r" || char === "\n") {
        commitCommandBuffer();
        continue;
      }

      if (char === "\u007f" || char === "\b") {
        commandBufferRef.current = commandBufferRef.current.slice(0, -1);
        continue;
      }

      if (char === "\u0015") {
        commandBufferRef.current = "";
        continue;
      }

      if (code >= 0x20 || char === "\t") {
        commandBufferRef.current += char;
      }
    }
  }, [commitCommandBuffer]);

  const sendTextInput = useCallback(async (
    text: string,
    options?: TerminalSendOptions,
  ) => {
    if (!text) return false;

    const amux = getBridge();
    if (!sessionReadyRef.current) return false;

    if (options?.execute && options?.managed !== false) {
      if (!amux?.executeManagedCommand) return false;
      const managedPath = options?.source === "agent"
        ? "assistant-managed"
        : options?.source === "gateway"
          ? "gateway-managed"
          : options?.source === "replay"
            ? "replay-managed"
            : "human-managed";
      addCommandLogEntry({
        command: text.trim(),
        path: managedPath,
        cwd: paneWorkspaceCwd ?? null,
        workspaceId: paneWorkspaceId ?? null,
        surfaceId: paneSurfaceId ?? null,
        paneId,
      });
      await amux.executeManagedCommand(paneId, {
        command: text,
        rationale: options?.rationale ?? "Managed execution requested from the terminal UI",
        allowNetwork: options?.allowNetwork ?? settings.sandboxNetworkEnabled,
        sandboxEnabled: options?.sandboxEnabled ?? settings.sandboxEnabled,
        securityLevel: settings.securityLevel,
        cwd: paneWorkspaceCwd ?? undefined,
        languageHint: options?.languageHint ?? "shell",
        source: options?.source ?? "agent",
      });
      return true;
    }

    if (!amux?.sendTerminalInput) return false;

    let payload = options?.execute ? `${text}\r` : text;
    if (options?.trackHistory !== false) {
      commandPathRef.current = options?.bracketed ? "human-paste" : "human-typed";
      trackInput(payload);
    }

    const termBracketedPaste = bracketedPasteRef.current && (termRef.current?.modes.bracketedPasteMode ?? false);
    payload = options?.bracketed ? wrapBracketedPaste(payload, termBracketedPaste) : payload;
    await amux.sendTerminalInput(paneId, encodeTextToBase64(payload));
    return true;
  }, [addCommandLogEntry, paneId, paneSurfaceId, paneWorkspaceCwd, paneWorkspaceId, sessionReadyRef, settings.sandboxEnabled, settings.sandboxNetworkEnabled, settings.securityLevel, termRef, trackInput]);

  const duplicateSplit = useCallback(async (direction: "horizontal" | "vertical") => {
    if (!paneWorkspaceId || !paneSurfaceId) return;

    const state = useWorkspaceStore.getState();
    const workspace = state.workspaces.find((entry) => entry.id === paneWorkspaceId);
    const surface = workspace?.surfaces.find((entry) => entry.id === paneSurfaceId);
    if (!workspace || !surface || surface.layoutMode !== "bsp") return;

    const sourceSessionId = resolveDuplicateSourceSessionId(
      paneId,
      findLeaf(surface.layout, paneId)?.sessionId ?? requestedSessionIdRef.current ?? null,
      operationalEvents,
    );
    const cloneResult = await cloneSessionForDuplication(paneId, sourceSessionId, {
      workspaceId: workspace.id,
      cwd: workspace.cwd || null,
    });
    const sourceName = surface.paneNames[paneId] ?? paneName;
    const sourceIcon = surface.paneIcons[paneId] ?? "terminal";

    splitActive(direction, `${sourceName} Copy`, {
      sessionId: cloneResult?.sessionId ?? null,
      paneIcon: sourceIcon,
    });

    const duplicatedPaneId = useWorkspaceStore.getState().activePaneId();
    if (!duplicatedPaneId) return;
    const bootstrapCommand =
      resolveDuplicateActiveBootstrapCommand(paneId, operationalEvents)
      ?? resolveDuplicateBootstrapCommand(paneId, operationalEvents)
      ?? cloneResult?.activeCommand;
    if (bootstrapCommand) {
      queuePaneBootstrapCommand(duplicatedPaneId, bootstrapCommand);
    }
  }, [operationalEvents, paneId, paneName, paneSurfaceId, paneWorkspaceId, requestedSessionIdRef, splitActive]);

  return {
    platformRef,
    commandBufferRef,
    autoCopyOnSelectRef,
    lastShellCommandRef,
    commandPathRef,
    trackInput,
    sendTextInput,
    duplicateSplit,
  };
}
