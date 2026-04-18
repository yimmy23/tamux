import { createRef } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import type { AgentThread } from "@/lib/agentStore";
import { ChatView } from "./ChatView";

describe("ChatView participants", () => {
  it("renders a participant summary entry point with counts", () => {
    const activeThread: AgentThread = {
      id: "thread-1",
      daemonThreadId: "daemon-thread-1",
      workspaceId: null,
      surfaceId: null,
      paneId: null,
      agent_name: "Svarog",
      title: "Conversation",
      createdAt: 1,
      updatedAt: 1,
      messageCount: 0,
      totalInputTokens: 0,
      totalOutputTokens: 0,
      totalTokens: 0,
      compactionCount: 0,
      lastMessagePreview: "",
      threadParticipants: [
        {
          agentId: "weles",
          agentName: "Weles",
          instruction: "verify claims",
          status: "active",
          createdAt: 1,
          updatedAt: 1,
        },
        {
          agentId: "rarog",
          agentName: "Rarog",
          instruction: "watch approvals",
          status: "inactive",
          createdAt: 1,
          updatedAt: 1,
          deactivatedAt: 2,
        },
      ],
      queuedParticipantSuggestions: [
        {
          id: "sugg-1",
          targetAgentId: "weles",
          targetAgentName: "Weles",
          instruction: "verify claims",
          forceSend: true,
          status: "failed",
          createdAt: 1,
          updatedAt: 1,
          error: "provider unavailable",
        },
      ],
    };

    const html = renderToStaticMarkup(
      <ChatView
        messages={[]}
        todos={[]}
        input=""
        setInput={() => { }}
        inputRef={createRef<HTMLTextAreaElement>()}
        onKeyDown={() => { }}
        agentSettings={{
          enabled: true,
          chatFontFamily: "monospace",
          reasoning_effort: "high",
          audio_stt_enabled: true,
          audio_stt_provider: "openai",
          audio_stt_model: "whisper-1",
          audio_stt_language: "",
          audio_tts_enabled: true,
          audio_tts_provider: "openai",
          audio_tts_model: "gpt-4o-mini-tts",
          audio_tts_voice: "alloy",
          audio_tts_auto_speak: false,
        }}
        isStreamingResponse={false}
        activeThread={activeThread}
        messagesEndRef={createRef<HTMLDivElement>()}
        onSendMessage={() => { }}
        onSendParticipantSuggestion={() => { }}
        onDismissParticipantSuggestion={() => { }}
        onStopStreaming={() => { }}
        onUpdateReasoningEffort={() => { }}
        canStartGoalRun={false}
        onStartGoalRun={async () => false}
      />,
    );

    expect(html).toContain("Thread Participants");
    expect(html).toContain("1 active");
    expect(html).toContain("1 inactive");
    expect(html).toContain("1 queued");
    expect(html).toContain("View Details");
  });
});