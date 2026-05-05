---
name: axolotl
description: "Streamlined fine-tuning framework for LLMs. Supports full fine-tune, LoRA, QLoRA, FSDP, DeepSpeed, and multi-GPU. YAML config driven. Works with Llama, Mistral, Qwen, DeepSeek, and hundreds of HF models."
tags: [axolotl, fine-tuning, llm, lora, deep-speed, huggingface, zorai]
---
## Overview

Axolotl is a fine-tuning framework supporting SFT, QLoRA, LoRA, full fine-tuning, DPO, and multimodal tuning for 100+ models (Llama, Mistral, Qwen, Gemma, DeepSeek). YAML-driven config avoids boilerplate. Supports multi-GPU, FSDP, DeepSpeed, and flash attention.

## Installation

```bash
git clone https://github.com/OpenAccess-AI-Collective/axolotl
cd axolotl
uv pip install -e .
```

## Basic Config

```yaml
# config.yml
base_model: Qwen/Qwen2.5-1.5B-Instruct
model_type: AutoModelForCausalLM
tokenizer_type: AutoTokenizer
output_dir: ./output

# LoRA
adapter: lora
lora_r: 16
lora_alpha: 32
lora_dropout: 0.05
lora_target_modules:
  - q_proj
  - v_proj

# Training
sequence_len: 2048
micro_batch_size: 2
gradient_accumulation_steps: 4
num_epochs: 3
learning_rate: 2e-5
optimizer: adamw_bnb_8bit
```

## Run

```bash
accelerate launch -m axolotl.cli.train config.yml
```

## Inference

```bash
python -m axolotl.cli.inference --lora_model_dir ./output --base_model Qwen/Qwen2.5-1.5B-Instruct
```

## References
- [Axolotl GitHub](https://github.com/OpenAccess-AI-Collective/axolotl)
- [Axolotl docs](https://axolotl.ziggycrane.com/)