# Speech to Text (STT) and Text to Speech (TTS) in Zorai

This guide shows how to use voice input/output in Zorai across the TUI and desktop app, how to configure providers/models, and how to troubleshoot common failures.

---

## What you can do

- **Speech to Text (STT):** record audio and convert it to text
- **Text to Speech (TTS):** synthesize assistant responses to audio
- Configure STT/TTS provider/model/voice from Zorai settings (persisted in daemon config)

Voice settings are persisted in daemon config under `extra.audio_*`, so they survive restarts.

---

## TUI usage (keyboard-first)

### 1) Open voice settings

1. Open settings: `/settings`
2. Go to **Features** tab
3. Navigate to the **Audio** section

Editable fields:

- `STT Enabled`
- `STT Provider`
- `STT Model`
- `TTS Enabled`
- `TTS Provider`
- `TTS Model`
- `TTS Voice`

The values are saved through daemon `SetConfigItem` updates to:

- `/extra/audio_stt_enabled`
- `/extra/audio_stt_provider`
- `/extra/audio_stt_model`
- `/extra/audio_tts_enabled`
- `/extra/audio_tts_provider`
- `/extra/audio_tts_model`
- `/extra/audio_tts_voice`

### 2) Record speech and transcribe

- Focus the **Input** area
- Press **Ctrl+L** to start recording
- Press **Ctrl+L** again to stop and transcribe

Expected behavior:

- Status/footer shows recording activity (`REC` indicator in status bar)
- Transcript is inserted into input when STT completes

### 3) Speak selected/latest assistant message

- Focus the **Chat** area
- Optionally select an assistant message with arrow keys
- Press **Ctrl+P**

Behavior:

- If an assistant message is selected, Zorai speaks that message
- If no message is selected, Zorai speaks the latest assistant message
- Footer shows `🔊 PLAYING` while playback process is alive
- If selected message is not speakable assistant text, Zorai shows a warning

### 4) Stop playback

- Press **Ctrl+S** to stop active audio playback

---

## Desktop app usage (Electron)

In the desktop UI you can:

- Use the composer mic controls for STT capture
- Use speak controls on assistant messages for TTS playback
- Configure STT/TTS provider/model/voice in settings

Settings persist in daemon config the same way as TUI.

---

## Recommended defaults

- `audio_stt_provider`: `openai`
- `audio_stt_model`: `whisper-1`
- `audio_tts_provider`: `openai`
- `audio_tts_model`: `gpt-4o-mini-tts`
- `audio_tts_voice`: `alloy`

---

## Linux runtime dependencies

TUI voice capture/playback shells out to system binaries.

### Recording backends (STT input)

TUI tries:

1. `ffmpeg`
2. `arecord` (fallback)

### Playback backends (TTS output)

TUI tries:

1. `mpv`
2. `paplay` (fallback)

Install the tools available on your distro.

---

## Troubleshooting

### "Voice capture failed" / recorder unavailable

- Install `ffmpeg` and/or `arecord`
- Verify microphone device/permissions
- Retry `Ctrl+L`

### "Audio playback failed" / player unavailable

- Install `mpv` and/or `paplay`
- Retry `Ctrl+P`
- Stop stuck playback with `Ctrl+S`

### STT/TTS tool errors

- Open error viewer with **Ctrl+E** in TUI
- Confirm provider credentials and model availability
- Check `audio_*` fields in settings for typos

### Transcript not inserted

- Ensure `STT Enabled` is true
- Confirm speech result returned text (not only error)

---

## Config snippet example

```json
{
  "extra": {
    "audio_stt_enabled": true,
    "audio_stt_provider": "openai",
    "audio_stt_model": "whisper-1",
    "audio_tts_enabled": true,
    "audio_tts_provider": "openai",
    "audio_tts_model": "gpt-4o-mini-tts",
    "audio_tts_voice": "alloy"
  }
}
```

---

## Quick voice workflow (TUI)

1. `/settings` → Features → Audio (set STT/TTS fields)
2. Go to Input → **Ctrl+L** (start)
3. **Ctrl+L** again (stop + transcribe)
4. Send/edit transcript as needed
5. Go to Chat, select assistant msg → **Ctrl+P** (speak)
6. **Ctrl+S** if you need to stop playback
