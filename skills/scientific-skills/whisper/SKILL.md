---
name: whisper
description: "OpenAI Whisper — general-purpose speech recognition. Multilingual transcription, translation to English, and speaker-agnostic ASR. Models from tiny to large. Robust to noise, accents, and technical vocabulary."
tags: [speech-to-text, multilingual-asr, audio-transcription, translation-asr, whisper]
---
## Overview

OpenAI Whisper is a general-purpose speech recognition model supporting multilingual transcription, translation to English, and speaker-agnostic ASR. Models range from tiny (39M params) to large (1.55B params). Robust to noise, accents, and technical vocabulary.

## Installation

```bash
uv pip install openai-whisper
ffmpeg  # required for audio loading
```

## Basic Transcription

```python
import whisper

model = whisper.load_model("base")
result = model.transcribe("audio.mp3")
print(result["text"])
```

## Multilingual and Translation

```python
# Transcribe in original language
result = model.transcribe("french_audio.mp3", language="fr")

# Translate to English
result = model.transcribe("german_audio.mp3", task="translate")
print(result["text"])  # English output
```

## Word-Level Timestamps

```python
result = model.transcribe("lecture.mp3", word_timestamps=True)
for segment in result["segments"]:
    for word in segment["words"]:
        print(f"{word['word']}: {word['start']:.2f}-{word['end']:.2f}")
```

## Model Size Selection

```python
# tiny (fast, less accurate) → base → small → medium → large (slow, most accurate)
sizes = ["tiny", "base", "small", "medium", "large"]
for s in sizes:
    m = whisper.load_model(s)
    # ~1GB VRAM for base, ~10GB for large
    result = m.transcribe("podcast.mp3")
```

## References
- [OpenAI Whisper GitHub](https://github.com/openai/whisper)
- [Whisper paper - Robust Speech Recognition](https://arxiv.org/abs/2212.04356)