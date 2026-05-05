---
name: sglang
description: "Structured Generation Language for LLM serving. RadixAttention prefix caching, constrained decoding (JSON, grammar), OpenAI-compatible API, and multi-turn optimization. Fast inference with structured output guarantees."
tags: [structured-generation, constrained-decoding, llm-serving, agent-inference, sglang]
---
## Overview

SGLang is a fast LLM inference and serving framework with structured generation (constrained decoding), RadixAttention for prefix caching, and an OpenAI-compatible API. Particularly strong for reasoning models (DeepSeek R1, QwQ) and guided generation with JSON schema, grammar, and regex constraints.

## Installation

```bash
uv pip install sglang[all]
```

## Quick Start

```python
import sglang as sgl

@sgl.function
def multi_turn(s, question):
    s += sgl.system("You are a helpful assistant.")
    s += sgl.user(question)
    s += sgl.assistant()

state = multi_turn.run(question="What is the derivative of x^2?")
print(state["answer"])
```

## Server Mode

```bash
python -m sglang.launch_server --model-path Qwen/Qwen2.5-1.5B-Instruct --port 30000
```

```python
from openai import OpenAI
client = OpenAI(base_url="http://localhost:30000/v1", api_key="none")
```

## Structured Generation

```python
@sgl.function
def json_gen(s):
    s += "Generate a person's info in JSON."
    s += sgl.gen("json_output", max_tokens=128, 
                 schema='{"type": "object", "properties": {"name": {"type": "string"}, "age": {"type": "integer"}}}')

state = json_gen.run()
print(state["json_output"])
```

## References
- [SGLang docs](https://sgl-project.github.io/)
- [SGLang GitHub](https://github.com/sgl-project/sglang)