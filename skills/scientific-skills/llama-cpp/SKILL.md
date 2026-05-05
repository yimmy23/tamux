---
name: llama-cpp
description: "LLM inference in C/C++ with Python bindings. GPU acceleration via CUDA/Metal/Vulkan, 2-8 bit quantization (GGUF), KV cache, and grammar-based sampling. Run Llama, Mistral, Gemma, Phi locally."
tags: [llama-cpp, gguf, quantization, local-llm, inference, python, zorai]
---
## Overview

llama.cpp is a C++ inference engine for LLMs optimized for CPU and Apple Silicon. Runs GGUF-format models (Llama, Mistral, Qwen, Gemma, Phi, DeepSeek, etc.) with quantization from Q2 to Q8. Supports GPU offloading, batch inference, and OpenAI-compatible server.

## Installation

```bash
uv pip install llama-cpp-python
# For CUDA: pip install llama-cpp-python --extra-index-url https://abetlen.github.io/llama-cpp-python/whl/cu124
```

## Basic Inference

```python
from llama_cpp import Llama

llm = Llama(model_path="qwen2.5-1.5b-instruct-q4_k_m.gguf")
output = llm("Q: What is machine learning?
A:", max_tokens=128)
print(output["choices"][0]["text"])
```

## Chat Format

```python
llm = Llama(model_path="model.gguf", chat_format="chatml")
response = llm.create_chat_completion(
    messages=[
        {"role": "system", "content": "You are a helpful assistant."},
        {"role": "user", "content": "Explain gradient descent."},
    ]
)
print(response["choices"][0]["message"]["content"])
```

## GPU Offloading

```python
llm = Llama(
    model_path="model.gguf",
    n_gpu_layers=-1,  # offload all layers to GPU
    n_ctx=8192,       # context window
    n_threads=8,
)
```

## References
- [llama.cpp GitHub](https://github.com/ggerganov/llama.cpp)
- [llama-cpp-python docs](https://llama-cpp-python.readthedocs.io/)