---
name: ollama
description: "Local LLM runner. One-command setup for Llama, Mistral, Gemma, Qwen, DeepSeek, Phi, and 100+ models. OpenAI-compatible API, model management, GPU acceleration, and custom Modelfile creation."
tags: [ollama, local-llm, llm, inference, api, modelfile, zorai]
---
## Overview

Ollama runs LLMs locally with a single command. Supports Llama 3, Mistral, Gemma, Qwen 2.5, DeepSeek, Phi, and 100+ models with GPU acceleration (CUDA/Metal), OpenAI-compatible API, and custom Modelfiles for configuration.

## Installation

```bash
# macOS / Linux
curl -fsSL https://ollama.com/install.sh | sh
```

## Basic Usage

```bash
ollama pull llama3.1:8b
ollama run llama3.1:8b "Explain quantum computing"
```

## Python API

```python
import openai
client = openai.OpenAI(base_url="http://localhost:11434/v1", api_key="ollama")
resp = client.chat.completions.create(
    model="llama3.1:8b",
    messages=[{"role": "user", "content": "What is ML?"}],
)
print(resp.choices[0].message.content)
```

## Custom Modelfile

```dockerfile
FROM llama3.1:8b
PARAMETER temperature 0.3
SYSTEM "You are a medical coding assistant."
```

```bash
ollama create my-coder -f Modelfile
```

## References
- [Ollama docs](https://github.com/ollama/ollama)
- [Ollama library](https://ollama.com/library)