import { createRef } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { ChatComposer } from "./Composer";

const agentSettings = {
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
};

describe("ChatComposer", () => {
  it("shows a pending TTS placeholder in the input area", () => {
    const html = renderToStaticMarkup(
      <ChatComposer
        input=""
        setInput={() => {}}
        attachments={[]}
        setAttachments={() => {}}
        inputRef={createRef<HTMLTextAreaElement>()}
        onKeyDown={() => {}}
        agentSettings={agentSettings}
        isStreamingResponse={false}
        isSynthesizingSpeech={true}
        onStopStreaming={() => {}}
        onSend={() => {}}
        canStartGoalRun={false}
        onStartGoalRun={() => {}}
        onUpdateReasoningEffort={() => {}}
      />,
    );

    expect(html).toContain('placeholder="Preparing speech..."');
  });
});
