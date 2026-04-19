import { describe, expect, it } from "vitest";
import type { AgentMessage } from "@/lib/agentStore";
import {
  findLatestAgentToolTextToSpeechPlayback,
  resolveAudioPlaybackSource,
  resolveToolResultAudioPlaybackSource,
} from "./audioPlayback";

function makeToolMessage(partial: Partial<AgentMessage>): AgentMessage {
  return {
    id: partial.id ?? "message-1",
    threadId: partial.threadId ?? "thread-1",
    createdAt: partial.createdAt ?? 1,
    role: partial.role ?? "tool",
    content: partial.content ?? "",
    inputTokens: 0,
    outputTokens: 0,
    totalTokens: 0,
    isCompactionSummary: false,
    ...partial,
  };
}

describe("audioPlayback helpers", () => {
  it("prefers file_url before url and path", () => {
    expect(resolveAudioPlaybackSource({
      file_url: "file:///tmp/speech.mp3",
      url: "https://example.com/speech.mp3",
      path: "/tmp/speech.mp3",
    })).toBe("file:///tmp/speech.mp3");
  });

  it("parses a playable source from text_to_speech tool JSON", () => {
    expect(resolveToolResultAudioPlaybackSource(JSON.stringify({
      path: "/tmp/tool-speech.mp3",
    }))).toBe("/tmp/tool-speech.mp3");
  });

  it("finds the latest completed text_to_speech tool result that has audio", () => {
    const playback = findLatestAgentToolTextToSpeechPlayback([
      makeToolMessage({
        id: "tool-1",
        toolCallId: "call-1",
        toolName: "bash_command",
        toolStatus: "done",
        content: "/repo",
      }),
      makeToolMessage({
        id: "tool-2",
        toolCallId: "call-2",
        toolName: "text_to_speech",
        toolStatus: "done",
        content: JSON.stringify({ path: "/tmp/tool-speech.mp3" }),
      }),
    ]);

    expect(playback).toEqual({
      toolCallId: "call-2",
      source: "/tmp/tool-speech.mp3",
    });
  });

  it("does not replay the latest handled text_to_speech tool result", () => {
    const messages = [
      makeToolMessage({
        id: "tool-2",
        toolCallId: "call-2",
        toolName: "text_to_speech",
        toolStatus: "done",
        content: JSON.stringify({ path: "/tmp/tool-speech.mp3" }),
      }),
    ];

    expect(findLatestAgentToolTextToSpeechPlayback(messages, "call-2")).toBeNull();
  });
});
