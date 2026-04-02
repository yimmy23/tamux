import { useCallback, type MutableRefObject } from "react";
import { clearThreadAbortController, getEffectiveContextWindow, setThreadAbortController, useAgentStore } from "@/lib/agentStore";
import { prepareOpenAIRequest, sendChatCompletion } from "@/lib/agentClient";
import type { AgentProviderConfig } from "@/lib/agentStore";
import { buildHonchoContext, syncMessagesToHoncho } from "@/lib/honchoClient";
import { executeTool, getAvailableTools, getToolCapabilityDescription } from "@/lib/agentTools";
import { useAgentMissionStore } from "@/lib/agentMissionStore";
import { useWorkspaceStore } from "@/lib/workspaceStore";

export function useLegacyAgentMessaging({
  activeThreadId,
  agentSettings,
  addMessage,
  abortRef,
  createThread,
  setView,
  stopStreaming,
  updateLastAssistantMessage,
}: {
  activeThreadId: string | null;
  agentSettings: ReturnType<typeof useAgentStore.getState>["agentSettings"];
  addMessage: ReturnType<typeof useAgentStore.getState>["addMessage"];
  abortRef: MutableRefObject<AbortController | null>;
  createThread: ReturnType<typeof useAgentStore.getState>["createThread"];
  setView: (view: import("./types").AgentChatPanelView) => void;
  stopStreaming: (threadId?: string | null) => void;
  updateLastAssistantMessage: ReturnType<typeof useAgentStore.getState>["updateLastAssistantMessage"];
}) {
  const sendMessageLegacy = useCallback((text: string) => {
    let threadId = activeThreadId;
    if (!threadId) {
      const workspaceId = useWorkspaceStore.getState().activeWorkspaceId;
      const surfaceId = useWorkspaceStore.getState().activeSurface()?.id ?? null;
      const paneId = useWorkspaceStore.getState().activePaneId();
      threadId = createThread({
        workspaceId,
        surfaceId,
        paneId,
        title: text.slice(0, 50),
      });
      setView("chat");
    }

    addMessage(threadId, {
      role: "user",
      content: text,
      inputTokens: 0,
      outputTokens: 0,
      totalTokens: 0,
      isCompactionSummary: false,
    });

    const providerConfig = agentSettings[agentSettings.active_provider] as AgentProviderConfig;
    const currentThreadId = threadId;
    const tools = getAvailableTools({
      enable_bash_tool: agentSettings.enable_bash_tool,
      gateway_enabled: agentSettings.gateway_enabled,
      enable_vision_tool: agentSettings.enable_vision_tool,
      enable_web_browsing_tool: agentSettings.enable_web_browsing_tool,
    });
    const toolCapabilities = getToolCapabilityDescription(tools);
    const systemPrompt = agentSettings.system_prompt + toolCapabilities;

    stopStreaming(currentThreadId);

    addMessage(currentThreadId, {
      role: "assistant",
      content: "",
      provider: agentSettings.active_provider,
      model: providerConfig.model,
      api_transport: providerConfig.api_transport,
      inputTokens: 0,
      outputTokens: 0,
      totalTokens: 0,
      isCompactionSummary: false,
      isStreaming: true,
    });
    const controller = new AbortController();
    abortRef.current = controller;
    setThreadAbortController(currentThreadId, controller);

    void (async () => {
      const configuredToolLoops = Number(agentSettings.max_tool_loops ?? 0);
      const maxToolLoops = Number.isFinite(configuredToolLoops) && configuredToolLoops > 0
        ? Math.min(1000, configuredToolLoops)
        : Infinity;
      let loopCount = 0;
      let allCurrentMessages = useAgentStore.getState().getThreadMessages(currentThreadId);
      await syncMessagesToHoncho(agentSettings, currentThreadId, allCurrentMessages);
      const getCurrentProviderConfig = () => (
        useAgentStore.getState().agentSettings[agentSettings.active_provider] as AgentProviderConfig
      );
      let preparedRequest = prepareOpenAIRequest(
        allCurrentMessages.slice(0, -1),
        {
          ...useAgentStore.getState().agentSettings,
          context_window_tokens: getEffectiveContextWindow(
            agentSettings.active_provider,
            getCurrentProviderConfig(),
          ),
        },
        agentSettings.active_provider,
        getCurrentProviderConfig().model,
        getCurrentProviderConfig().api_transport,
        getCurrentProviderConfig().auth_source,
        getCurrentProviderConfig().assistant_id,
        useAgentStore.getState().threads.find((entry) => entry.id === currentThreadId),
      );
      let lastPersistedReasoning: string | null = null;
      const honchoContext = await buildHonchoContext(agentSettings, currentThreadId, text);
      const effectiveSystemPrompt = honchoContext
        ? `${systemPrompt}\n\nCross-session memory:\n${honchoContext}`
        : systemPrompt;

      const persistReasoningTrace = (reasoning: string) => {
        const normalized = reasoning.trim();
        if (!normalized || normalized === lastPersistedReasoning) return;
        const thread = useAgentStore.getState().threads.find((entry) => entry.id === currentThreadId);
        const paneId = thread?.paneId ?? useWorkspaceStore.getState().activePaneId() ?? "agent";
        const workspaceId = thread?.workspaceId ?? useWorkspaceStore.getState().activeWorkspaceId;
        const surfaceId = thread?.surfaceId ?? useWorkspaceStore.getState().activeSurface()?.id ?? null;
        useAgentMissionStore.getState().recordCognitiveOutput({
          paneId,
          workspaceId,
          surfaceId,
          sessionId: null,
          text: `<INNER_MONOLOGUE>\n${normalized}\n</INNER_MONOLOGUE>`,
        });
        lastPersistedReasoning = normalized;
      };

      while (loopCount < maxToolLoops) {
        loopCount += 1;
        let accumulated = "";
        let accumulatedReasoning = "";
        const responseStartedAt = Date.now();
        let receivedToolCalls = false;

        try {
          for await (const chunk of sendChatCompletion({
            provider: agentSettings.active_provider,
            config: {
              ...providerConfig,
              api_transport: preparedRequest.transport,
            },
            system_prompt: effectiveSystemPrompt,
            messages: preparedRequest.messages,
            streaming: agentSettings.enable_streaming,
            signal: controller.signal,
            tools: tools.length > 0 ? tools : undefined,
            reasoning_effort: agentSettings.reasoning_effort,
            previousResponseId: preparedRequest.previousResponseId,
            upstreamThreadId: preparedRequest.upstreamThreadId,
          })) {
            if (chunk.type === "delta") {
              accumulated += chunk.content;
              if (chunk.reasoning) accumulatedReasoning += chunk.reasoning;
              updateLastAssistantMessage(currentThreadId, accumulated, true, {
                reasoning: accumulatedReasoning || undefined,
              });
              continue;
            }
            if (chunk.type === "done") {
              if (chunk.content && chunk.content !== accumulated) accumulated = chunk.content;
              if (chunk.reasoning) accumulatedReasoning = chunk.reasoning;
              persistReasoningTrace(accumulatedReasoning);
              const elapsedSeconds = Math.max(0.001, (Date.now() - responseStartedAt) / 1000);
              const outputTokens = Number(chunk.outputTokens ?? 0);
              const inputTokens = Number(chunk.inputTokens ?? 0);
              const totalTokens = Number(chunk.totalTokens ?? (inputTokens + outputTokens));
              const tps = outputTokens > 0 ? outputTokens / elapsedSeconds : undefined;
              updateLastAssistantMessage(currentThreadId, accumulated || "(empty response)", false, {
                inputTokens,
                outputTokens,
                totalTokens,
                reasoning: accumulatedReasoning || undefined,
                reasoningTokens: chunk.reasoningTokens,
                audioTokens: chunk.audioTokens,
                videoTokens: chunk.videoTokens,
                cost: chunk.cost,
                tps,
                api_transport: preparedRequest.transport,
                responseId: chunk.responseId,
              });
              continue;
            }
            if (chunk.type === "error") {
              updateLastAssistantMessage(currentThreadId, `Error: ${chunk.content}`, false);
              continue;
            }
            if (chunk.type === "transport_fallback") {
              useAgentStore.getState().updateAgentSetting(agentSettings.active_provider as keyof ReturnType<typeof useAgentStore.getState>["agentSettings"], {
                ...providerConfig,
                api_transport: "chat_completions",
              } as any);
              preparedRequest = { ...preparedRequest, transport: "chat_completions", previousResponseId: undefined, upstreamThreadId: undefined };
              continue;
            }
            if (chunk.type === "tool_calls" && chunk.toolCalls) {
              receivedToolCalls = true;
              if (chunk.reasoning) accumulatedReasoning = chunk.reasoning;
              if (chunk.content) accumulated = chunk.content;
              persistReasoningTrace(accumulatedReasoning);
              updateLastAssistantMessage(currentThreadId, accumulated || "Calling tools...", false, {
                reasoning: accumulatedReasoning || undefined,
                inputTokens: Number(chunk.inputTokens ?? 0),
                outputTokens: Number(chunk.outputTokens ?? 0),
                totalTokens: Number(chunk.totalTokens ?? ((chunk.inputTokens ?? 0) + (chunk.outputTokens ?? 0))),
                reasoningTokens: chunk.reasoningTokens,
                audioTokens: chunk.audioTokens,
                videoTokens: chunk.videoTokens,
                cost: chunk.cost,
                toolCalls: chunk.toolCalls,
                api_transport: preparedRequest.transport,
                responseId: chunk.responseId,
              });
              for (const toolCall of chunk.toolCalls) {
                addMessage(currentThreadId, {
                  role: "tool",
                  content: "",
                  toolName: toolCall.function.name,
                  toolCallId: toolCall.id,
                  toolArguments: toolCall.function.arguments,
                  toolStatus: "requested",
                  welesReview: toolCall.weles_review,
                  inputTokens: 0,
                  outputTokens: 0,
                  totalTokens: 0,
                  isCompactionSummary: false,
                });
              }
              for (const toolCall of chunk.toolCalls) {
                useAgentMissionStore.getState().recordToolCall({
                  toolName: toolCall.function.name,
                  arguments: toolCall.function.arguments,
                });
                const result = await executeTool(toolCall);
                addMessage(currentThreadId, {
                  role: "tool",
                  content: result.content,
                  toolName: result.name,
                  toolCallId: result.toolCallId,
                  toolArguments: toolCall.function.arguments,
                  toolStatus: result.content.startsWith("Error:") ? "error" : "done",
                  welesReview: result.weles_review ?? toolCall.weles_review,
                  inputTokens: 0,
                  outputTokens: 0,
                  totalTokens: 0,
                  isCompactionSummary: false,
                });
              }
              updateLastAssistantMessage(currentThreadId, accumulated || "Tools executed.", false);
              addMessage(currentThreadId, {
                role: "assistant",
                content: "",
                provider: agentSettings.active_provider,
                model: providerConfig.model,
                api_transport: preparedRequest.transport,
                inputTokens: 0,
                outputTokens: 0,
                totalTokens: 0,
                isCompactionSummary: false,
                isStreaming: true,
              });
            }
          }
        } catch (error: any) {
          if (error.name !== "AbortError") {
            updateLastAssistantMessage(currentThreadId, `Error: ${error.message || String(error)}`);
          }
          break;
        }

        if (!receivedToolCalls) break;
        allCurrentMessages = useAgentStore.getState().getThreadMessages(currentThreadId);
        preparedRequest = prepareOpenAIRequest(
          allCurrentMessages.slice(0, -1),
          {
            ...useAgentStore.getState().agentSettings,
            context_window_tokens: getEffectiveContextWindow(
              agentSettings.active_provider,
              getCurrentProviderConfig(),
            ),
          },
          agentSettings.active_provider,
          getCurrentProviderConfig().model,
          getCurrentProviderConfig().api_transport,
          getCurrentProviderConfig().auth_source,
          getCurrentProviderConfig().assistant_id,
          useAgentStore.getState().threads.find((entry) => entry.id === currentThreadId),
        );
      }

      await syncMessagesToHoncho(
        agentSettings,
        currentThreadId,
        useAgentStore.getState().getThreadMessages(currentThreadId),
      );

      if (Number.isFinite(maxToolLoops) && loopCount >= maxToolLoops) {
        updateLastAssistantMessage(currentThreadId, "(Tool execution limit reached)", false);
      }
      if (abortRef.current === controller) {
        abortRef.current = null;
      }
      clearThreadAbortController(currentThreadId, controller);
    })();
  }, [activeThreadId, addMessage, agentSettings, createThread, setView, stopStreaming, updateLastAssistantMessage, abortRef]);

  return { abortRef, sendMessageLegacy };
}
