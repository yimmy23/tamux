const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");

const preloadPath = path.join(__dirname, "preload.cjs");
const runtimePath = path.join(__dirname, "agent-query-runtime.cjs");
const handlerPath = path.join(__dirname, "main", "agent-ipc-handlers.cjs");

const preloadSrc = fs.readFileSync(preloadPath, "utf8");
const runtime = require(runtimePath);
const { registerAgentIpcHandlers } = require(handlerPath);

function createHandlerHarness() {
  const handlers = new Map();
  const queries = [];
  const persistedAudio = [];
  const ipcMain = {
    handle(name, handler) {
      handlers.set(name, handler);
    },
  };
  const sendAgentQuery = async (...args) => {
    queries.push(args);
    if (args[1] === "speech-to-text-result") {
      return { text: "transcribed hello" };
    }
    if (args[1] === "text-to-speech-result") {
      return { path: "/tmp/speech.mp3", mime_type: "audio/mpeg" };
    }
    return { ok: true };
  };

  registerAgentIpcHandlers(
    ipcMain,
    { sendAgentCommand: () => {}, sendAgentQuery },
    {
      logToFile: () => {},
      openAICodexAuthHandlers: {
        status: async () => ({ available: false }),
        login: async () => ({ available: false }),
        logout: async () => ({ ok: true }),
      },
      saveTempAudioCapture: (payload) => {
        persistedAudio.push(payload);
        return { ok: true, path: "/tmp/capture.webm", mimeType: payload.mimeType || "audio/webm" };
      },
    },
  );

  return { handlers, queries, persistedAudio };
}

test("preload exposes speech bridge methods", () => {
  assert.match(
    preloadSrc,
    /agentSpeechToText:\s*\(base64Audio, mimeType, options\)\s*=>\s*ipcRenderer\.invoke\('agent-speech-to-text', base64Audio, mimeType, options\)/,
  );
  assert.match(
    preloadSrc,
    /agentTextToSpeech:\s*\(text, voice, options\)\s*=>\s*ipcRenderer\.invoke\('agent-text-to-speech', text, voice, options\)/,
  );
});

test("runtime allowlists speech query response types", () => {
  assert.ok(runtime.AGENT_QUERY_RESPONSE_TYPES.includes("speech-to-text-result"));
  assert.ok(runtime.AGENT_QUERY_RESPONSE_TYPES.includes("text-to-speech-result"));
});

test("agent IPC handlers route speech APIs through the daemon bridge", async () => {
  const { handlers, queries, persistedAudio } = createHandlerHarness();

  assert.ok(handlers.has("agent-speech-to-text"));
  assert.ok(handlers.has("agent-text-to-speech"));

  const sttResult = await handlers.get("agent-speech-to-text")(null, "Zm9v", "audio/webm", {
    provider: "openai",
    model: "whisper-1",
    language: "en",
  });
  const ttsResult = await handlers.get("agent-text-to-speech")(null, "Hello", "alloy", {
    provider: "openai",
    model: "gpt-4o-mini-tts",
    response_format: "mp3",
  });

  assert.deepEqual(sttResult, { text: "transcribed hello" });
  assert.deepEqual(ttsResult, { path: "/tmp/speech.mp3", mime_type: "audio/mpeg", file_url: "file:///tmp/speech.mp3" });
  assert.deepEqual(persistedAudio, [{ base64: "Zm9v", mimeType: "audio/webm" }]);
  assert.deepEqual(queries, [
    [
      {
        type: "speech-to-text",
        args_json: JSON.stringify({
          provider: "openai",
          model: "whisper-1",
          language: "en",
          path: "/tmp/capture.webm",
          mime_type: "audio/webm",
        }),
      },
      "speech-to-text-result",
      30000,
    ],
    [
      {
        type: "text-to-speech",
        args_json: JSON.stringify({
          provider: "openai",
          model: "gpt-4o-mini-tts",
          response_format: "mp3",
          input: "Hello",
          voice: "alloy",
        }),
      },
      "text-to-speech-result",
      30000,
    ],
  ]);
});
