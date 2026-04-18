import type React from "react";
import { useEffect, useRef, useState } from "react";
import type { ComposerAttachment } from "./types";
import { inputStyle } from "../shared";

const TEXT_ATTACHMENT_EXTENSIONS = new Set([
  "txt", "md", "markdown", "json", "yaml", "yml", "toml", "ini", "cfg", "conf",
  "rs", "ts", "tsx", "js", "jsx", "py", "sh", "sql", "csv", "log",
]);

function fileLooksTextual(file: File): boolean {
  if (file.type.startsWith("text/")) return true;
  const ext = file.name.includes(".") ? file.name.split(".").pop()?.toLowerCase() ?? "" : "";
  return TEXT_ATTACHMENT_EXTENSIONS.has(ext);
}

async function readComposerAttachment(file: File): Promise<ComposerAttachment | null> {
  const kind = file.type.startsWith("image/")
    ? "image"
    : file.type.startsWith("audio/")
      ? "audio"
      : fileLooksTextual(file)
        ? "text"
        : null;
  if (!kind) return null;

  if (kind === "text") {
    return {
      id: `${file.name}:${file.size}:${file.lastModified}`,
      name: file.name,
      size: file.size,
      kind,
      mimeType: file.type || "text/plain",
      textContent: await file.text(),
    };
  }

  const dataUrl = await new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(typeof reader.result === "string" ? reader.result : "");
    reader.onerror = () => reject(reader.error ?? new Error("file read failed"));
    reader.readAsDataURL(file);
  });
  return {
    id: `${file.name}:${file.size}:${file.lastModified}`,
    name: file.name,
    size: file.size,
    kind,
    mimeType: file.type || (kind === "image" ? "image/png" : "audio/wav"),
    dataUrl,
  };
}

async function blobToBase64(blob: Blob): Promise<string> {
  const dataUrl = await new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(typeof reader.result === "string" ? reader.result : "");
    reader.onerror = () => reject(reader.error ?? new Error("blob read failed"));
    reader.readAsDataURL(blob);
  });
  const commaIndex = dataUrl.indexOf(",");
  return commaIndex >= 0 ? dataUrl.slice(commaIndex + 1) : dataUrl;
}

function readSpeechToTextContent(result: unknown): string {
  if (typeof result === "string") {
    return result.trim();
  }
  if (!result || typeof result !== "object") {
    return "";
  }
  const record = result as Record<string, unknown>;
  if (typeof record.text === "string") {
    return record.text.trim();
  }
  if (typeof record.content === "string") {
    return record.content.trim();
  }
  if (record.data && typeof record.data === "object" && record.data !== null) {
    const nested = record.data as Record<string, unknown>;
    if (typeof nested.text === "string") {
      return nested.text.trim();
    }
    if (typeof nested.content === "string") {
      return nested.content.trim();
    }
  }
  return "";
}

export function ChatComposer({
  input,
  setInput,
  attachments,
  setAttachments,
  inputRef,
  onKeyDown,
  agentSettings,
  isStreamingResponse,
  onStopStreaming,
  onSend,
  canStartGoalRun,
  onStartGoalRun,
  onUpdateReasoningEffort,
}: {
  input: string;
  setInput: React.Dispatch<React.SetStateAction<string>>;
  attachments: ComposerAttachment[];
  setAttachments: React.Dispatch<React.SetStateAction<ComposerAttachment[]>>;
  inputRef: React.RefObject<HTMLTextAreaElement | null>;
  onKeyDown: (event: React.KeyboardEvent) => void;
  agentSettings: {
    enabled: boolean;
    chatFontFamily: string;
    reasoning_effort: string;
    audio_stt_enabled: boolean;
    audio_stt_provider: string;
    audio_stt_model: string;
    audio_stt_language: string;
    audio_tts_enabled: boolean;
    audio_tts_provider: string;
    audio_tts_model: string;
    audio_tts_voice: string;
    audio_tts_auto_speak: boolean;
  };
  isStreamingResponse: boolean;
  onStopStreaming: () => void;
  onSend: () => void;
  canStartGoalRun: boolean;
  onStartGoalRun: () => void;
  onUpdateReasoningEffort: (value: string) => void;
}) {
  const fileInputRef = useRef<HTMLInputElement | null>(null);
  const [dropActive, setDropActive] = useState(false);
  const [isRecording, setIsRecording] = useState(false);
  const [isTranscribing, setIsTranscribing] = useState(false);
  const mediaRecorderRef = useRef<MediaRecorder | null>(null);
  const mediaStreamRef = useRef<MediaStream | null>(null);
  const recordedChunksRef = useRef<Blob[]>([]);

  useEffect(() => {
    return () => {
      mediaRecorderRef.current?.stop();
      mediaStreamRef.current?.getTracks().forEach((track) => track.stop());
    };
  }, []);

  const appendFiles = async (files: File[]) => {
    if (files.length === 0) return;
    const loaded = await Promise.all(files.map((file) => readComposerAttachment(file)));
    setAttachments((current) => [...current, ...loaded.filter((item): item is ComposerAttachment => Boolean(item))]);
  };

  const handleAttachmentSelect = async (event: React.ChangeEvent<HTMLInputElement>) => {
    const files = Array.from(event.target.files ?? []);
    await appendFiles(files);
    event.target.value = "";
  };

  const voiceCaptureAvailable = agentSettings.enabled
    && agentSettings.audio_stt_enabled
    && typeof window !== "undefined"
    && typeof MediaRecorder !== "undefined"
    && !!navigator.mediaDevices?.getUserMedia
    && !!(window.amux?.agentSpeechToText || window.tamux?.agentSpeechToText);

  const toggleRecording = async () => {
    if (isRecording) {
      mediaRecorderRef.current?.stop();
      return;
    }
    const bridge = window.amux ?? window.tamux;
    if (!bridge?.agentSpeechToText || !navigator.mediaDevices?.getUserMedia || typeof MediaRecorder === "undefined") {
      return;
    }

    try {
      const stream = await navigator.mediaDevices.getUserMedia({ audio: true });
      mediaStreamRef.current = stream;
      recordedChunksRef.current = [];
      const recorder = new MediaRecorder(stream);
      mediaRecorderRef.current = recorder;
      recorder.ondataavailable = (event) => {
        if (event.data.size > 0) {
          recordedChunksRef.current.push(event.data);
        }
      };
      recorder.onstop = () => {
        const mimeType = recorder.mimeType || recordedChunksRef.current[0]?.type || "audio/webm";
        const blob = new Blob(recordedChunksRef.current, { type: mimeType });
        mediaRecorderRef.current = null;
        mediaStreamRef.current?.getTracks().forEach((track) => track.stop());
        mediaStreamRef.current = null;
        recordedChunksRef.current = [];
        setIsRecording(false);
        void (async () => {
          if (blob.size === 0) {
            return;
          }
          setIsTranscribing(true);
          try {
            const base64Audio = await blobToBase64(blob);
            const result = await bridge.agentSpeechToText?.(base64Audio, mimeType, {
              provider: agentSettings.audio_stt_provider,
              model: agentSettings.audio_stt_model,
              language: agentSettings.audio_stt_language || undefined,
            });
            const transcript = readSpeechToTextContent(result);
            if (transcript) {
              setInput((current) => current.trim() ? `${current.trimEnd()} ${transcript}` : transcript);
            }
          } catch (error) {
            console.error("speech-to-text failed", error);
          } finally {
            setIsTranscribing(false);
          }
        })();
      };
      recorder.start();
      setIsRecording(true);
    } catch (error) {
      console.error("microphone capture failed", error);
      mediaStreamRef.current?.getTracks().forEach((track) => track.stop());
      mediaStreamRef.current = null;
      mediaRecorderRef.current = null;
      setIsRecording(false);
    }
  };

  return (
    <div
      style={{
        padding: "var(--space-3)",
        borderTop: "1px solid var(--border)",
        flexShrink: 0,
        display: "flex",
        flexDirection: "column",
        background: "var(--bg-tertiary)",
        userSelect: "auto",
        boxShadow: dropActive ? "0 0 0 2px rgba(94, 231, 223, 0.45) inset" : undefined,
      }}
      onDragOver={(event) => {
        if (!agentSettings.enabled) return;
        if (event.dataTransfer?.files?.length) {
          event.preventDefault();
          setDropActive(true);
        }
      }}
      onDragLeave={() => setDropActive(false)}
      onDrop={(event) => {
        if (!agentSettings.enabled) return;
        const files = Array.from(event.dataTransfer?.files ?? []);
        if (files.length === 0) return;
        event.preventDefault();
        setDropActive(false);
        void appendFiles(files);
      }}
    >
      <div
        style={{
          display: "grid",
          gridTemplateColumns: "auto 1fr",
          alignItems: "start",
          gap: "var(--space-2)",
          border: "1px solid rgba(94, 231, 223, 0.3)",
          background: "var(--bg-tertiary)",
          borderRadius: "var(--radius-md)",
          padding: "8px 10px",
        }}
      >
        <span
          style={{
            color: "#5ee7df",
            fontFamily: "var(--font-mono)",
            fontSize: "var(--text-sm)",
            lineHeight: "24px",
            userSelect: "auto",
          }}
        >
          &gt;
        </span>
        <textarea
          ref={inputRef}
          value={input}
          onChange={(event) => setInput(event.target.value)}
          onPaste={(event) => {
            const files = Array.from(event.clipboardData?.files ?? []);
            if (files.length > 0) {
              event.preventDefault();
              void appendFiles(files);
            }
          }}
          onKeyDown={onKeyDown}
          rows={3}
          placeholder={agentSettings.enabled ? "Type a message... (Enter to send, Ctrl+Enter for newline)" : "Agent disabled — enable in Settings > Agent"}
          disabled={!agentSettings.enabled}
          style={{
            ...inputStyle,
            width: "100%",
            resize: "none",
            background: "transparent",
            border: "none",
            color: "var(--text-primary)",
            padding: "4px 0",
            fontFamily: agentSettings.chatFontFamily,
            outline: "none",
            opacity: agentSettings.enabled ? 1 : 0.5,
            minHeight: 72,
          }}
        />
      </div>

      {attachments.length > 0 && (
        <div style={{ display: "flex", flexWrap: "wrap", gap: "var(--space-2)", marginTop: "var(--space-2)" }}>
          {attachments.map((attachment) => (
            <div
              key={attachment.id}
              style={{
                display: "flex",
                alignItems: "center",
                gap: "var(--space-2)",
                border: "1px solid var(--glass-border)",
                borderRadius: "var(--radius-sm)",
                padding: "4px 8px",
                fontSize: 11,
                color: "var(--text-secondary)",
              }}
            >
              <span>{attachment.kind === "image" ? "🖼" : attachment.kind === "audio" ? "🔊" : "📄"} {attachment.name}</span>
              <button
                type="button"
                onClick={() => setAttachments((current) => current.filter((item) => item.id !== attachment.id))}
                style={{ background: "transparent", border: "none", color: "var(--text-muted)", cursor: "pointer", fontSize: 11 }}
              >
                ×
              </button>
            </div>
          ))}
        </div>
      )}

      <div style={{ marginTop: "var(--space-2)", display: "flex", justifyContent: "space-between", alignItems: "center", gap: "var(--space-2)" }}>
        <div style={{ display: "flex", alignItems: "flex-start", flexDirection: "column", gap: 4 }}>
          <span style={{ fontSize: 11, color: "var(--text-muted)", fontFamily: "var(--font-mono)" }}>
            Reasoning effort
          </span>
          <select
            value={agentSettings.reasoning_effort}
            onChange={(event) => onUpdateReasoningEffort(event.target.value)}
            title="Reasoning effort"
            style={{
              fontSize: 10,
              fontFamily: "var(--font-mono)",
              background: "var(--bg-surface)",
              color: "var(--text-secondary)",
              border: "1px solid var(--glass-border)",
              borderRadius: 3,
              padding: "1px 4px",
              cursor: "pointer",
              outline: "none",
            }}
          >
            <option value="none">off</option>
            <option value="minimal">minimal</option>
            <option value="low">low</option>
            <option value="medium">medium</option>
            <option value="high">high</option>
            <option value="xhigh">xhigh</option>
          </select>
        </div>
        <div style={{ display: "flex", gap: "var(--space-2)", alignItems: "center" }}>
          {voiceCaptureAvailable && (
            <button
              type="button"
              onClick={() => {
                void toggleRecording();
              }}
              disabled={!agentSettings.enabled || isTranscribing}
              style={{
                border: `1px solid ${isRecording ? "rgba(255, 99, 132, 0.55)" : "var(--glass-border)"}`,
                background: isRecording ? "rgba(255, 99, 132, 0.12)" : "rgba(255,255,255,0.04)",
                color: isRecording ? "#ff9aa9" : "var(--text-secondary)",
                borderRadius: "var(--radius-sm)",
                padding: "6px 10px",
                fontSize: 12,
                cursor: !agentSettings.enabled || isTranscribing ? "not-allowed" : "pointer",
                opacity: !agentSettings.enabled || isTranscribing ? 0.5 : 1,
              }}
              title={isRecording ? "Stop recording" : isTranscribing ? "Transcribing..." : "Record voice message"}
            >
              {isRecording ? "Stop" : isTranscribing ? "..." : "Mic"}
            </button>
          )}
          <input
            ref={fileInputRef}
            type="file"
            accept="image/*,audio/*,.txt,.md,.markdown,.json,.yaml,.yml,.toml,.ini,.cfg,.conf,.rs,.ts,.tsx,.js,.jsx,.py,.sh,.sql,.csv,.log"
            multiple
            onChange={handleAttachmentSelect}
            style={{ display: "none" }}
          />
          <button
            type="button"
            onClick={() => fileInputRef.current?.click()}
            disabled={!agentSettings.enabled}
            style={{
              border: "1px solid var(--glass-border)",
              background: "rgba(255,255,255,0.04)",
              color: "var(--text-secondary)",
              borderRadius: "var(--radius-sm)",
              padding: "6px 10px",
              fontSize: 12,
              cursor: !agentSettings.enabled ? "not-allowed" : "pointer",
              opacity: !agentSettings.enabled ? 0.5 : 1,
            }}
          >
            Attach
          </button>
          {canStartGoalRun && (
            <button
              type="button"
              onClick={onStartGoalRun}
              disabled={!agentSettings.enabled || !input.trim()}
              style={{
                border: "1px solid var(--mission-border)",
                background: "var(--mission-soft)",
                color: "var(--mission)",
                borderRadius: "var(--radius-sm)",
                padding: "6px 12px",
                fontSize: 12,
                fontWeight: 700,
                cursor: !agentSettings.enabled || !input.trim() ? "not-allowed" : "pointer",
                opacity: !agentSettings.enabled || !input.trim() ? 0.5 : 1,
              }}
            >
              Goal Run
            </button>
          )}
          {isStreamingResponse && (
            <button
              type="button"
              onClick={onStopStreaming}
              style={{
                border: "1px solid rgba(255, 118, 117, 0.45)",
                background: "rgba(255, 118, 117, 0.15)",
                color: "#ff7675",
                borderRadius: "var(--radius-sm)",
                padding: "6px 10px",
                fontSize: 12,
                fontWeight: 600,
                cursor: "pointer",
              }}
            >
              Stop
            </button>
          )}
          <button
            type="button"
            onClick={onSend}
            disabled={!agentSettings.enabled || (!input.trim() && attachments.length === 0)}
            style={{
              border: "1px solid var(--accent)",
              background: "rgba(94, 231, 223, 0.16)",
              color: "var(--accent)",
              borderRadius: "var(--radius-sm)",
              padding: "6px 12px",
              fontSize: 12,
              fontWeight: 700,
              cursor: !agentSettings.enabled || (!input.trim() && attachments.length === 0) ? "not-allowed" : "pointer",
              opacity: !agentSettings.enabled || (!input.trim() && attachments.length === 0) ? 0.5 : 1,
            }}
          >
            Send
          </button>
        </div>
      </div>
    </div>
  );
}
