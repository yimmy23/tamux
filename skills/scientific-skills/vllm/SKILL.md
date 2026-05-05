---
name: vllm
description: "Fast LLM inference engine. PagedAttention, continuous batching, tensor parallelism, speculative decoding, and prefix caching. OpenAI-compatible API server. Supports Llama, Mistral, Qwen, DeepSeek, and hundreds of models."
tags: [llm-serving, paged-attention, openai-compatible-server, high-throughput-inference, vllm]
---
## Overview

vLLM is a high-throughput, memory-efficient LLM inference engine featuring PagedAttention (near-zero memory waste), continuous batching, tensor parallelism, speculative decoding, prefix caching, and an OpenAI-compatible API.

## Installation

```bash
uv pip install vllm
```

## Offline Inference

```python
from vllm import LLM, SamplingParams

llm = LLM(model="Qwen/Qwen2.5-1.5B-Instruct")
params = SamplingParams(temperature=0.7, top_p=0.9, max_tokens=512)

outputs = llm.generate(["What is the capital of France?"], params)
for o in outputs:
    print(o.outputs[0].text)
```

## API Server

```bash
vllm serve Qwen/Qwen2.5-1.5B-Instruct --port 8000
# OpenAI client:
curl http://localhost:8000/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{"model": "Qwen/Qwen2.5-1.5B-Instruct", "messages": [{"role": "user", "content": "Hello!"}]}'
```

## Multi-GPU

```python
llm = LLM(model="meta-llama/Llama-3.1-8B", tensor_parallel_size=2)
```

## References
- [vLLM docs](https://docs.vllm.ai/)
- [vLLM GitHub](https://github.com/vllm-project/vllm)