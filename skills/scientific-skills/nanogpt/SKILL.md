---
name: nanogpt
description: Minimal GPT pretraining and fine-tuning (nanoGPT). The simplest, fastest repository for training medium-sized GPTs with ~300-line model.py and ~300-line train.py. Reproduces GPT-2 (124M) on OpenWebText. Supports DDP multi-GPU/multi-node, character-level training, weight loading from HuggingFace GPT-2 checkpoints, and simple finetuning. Note: superseded by nanochat for new projects; this repo remains valuable as a reference implementation and learning tool.
license: MIT license
tags: [gpt-pretraining, autoregressive-language-modeling, ddp-training, checkpoint-finetuning, nanogpt]
metadata:
    skill-author: K-Dense Inc.
---

# nanoGPT

## Overview

nanoGPT is the simplest, fastest repository for training and finetuning medium-sized GPTs. With a ~300-line model definition and ~300-line training loop, it reproduces GPT-2 (124M) on OpenWebText in ~4 days on a single 8×A100 node. Everything is plain, readable PyTorch — no abstractions, no frameworks — making it ideal for learning, hacking, and as a reference for your own training code. **Note: superseded by [nanochat](https://github.com/karpathy/nanochat) for new projects, but remains excellent for understanding GPT internals and as a clean starting point.**

## When to Use This Skill

This skill should be used when:
- Learning how GPT pretraining works from a minimal, readable codebase
- Training small-to-medium GPTs from scratch (character-level to 124M+)
- Finetuning pretrained GPT-2 models on custom datasets
- Understanding the full training pipeline: tokenization → training → sampling
- Hacking on transformer architectures with minimal boilerplate
- Reproducing GPT-2 training results for research/validation
- Bootstrapping a custom LLM training project from a clean reference

## Core Capabilities

### 1. Installation

```bash
pip install torch numpy transformers datasets tiktoken wandb tqdm
```

### 2. Quick Start — Shakespeare Character-Level GPT

**Prepare data:**
```bash
python data/shakespeare_char/prepare.py
# Creates train.bin and val.bin in data/shakespeare_char/
```

**Train (GPU):**
```bash
python train.py config/train_shakespeare_char.py
# ~3 minutes on A100, val loss ~1.47
```

**Sample:**
```bash
python sample.py --out_dir=out-shakespeare-char
```

**Train on CPU / MacBook:**
```bash
python train.py config/train_shakespeare_char.py \
    --device=cpu --compile=False --eval_iters=20 \
    --block_size=64 --batch_size=12 --n_layer=4 \
    --n_head=4 --n_embd=128 --max_iters=2000 \
    --lr_decay_iters=2000 --dropout=0.0
```

**Apple Silicon (MPS):**
```bash
python train.py config/train_shakespeare_char.py --device=mps
```

### 3. Model Architecture (`model.py`)

The core GPT model is ~300 lines of clean PyTorch:

```python
# Key components (simplified):
class GPT(nn.Module):
    def __init__(self, config):
        self.transformer = nn.ModuleDict(dict(
            wte = nn.Embedding(config.vocab_size, config.n_embd),
            wpe = nn.Embedding(config.block_size, config.n_embd),
            h = nn.ModuleList([Block(config) for _ in range(config.n_layer)]),
            ln_f = nn.LayerNorm(config.n_embd),
        ))
        self.lm_head = nn.Linear(config.n_embd, config.vocab_size, bias=False)

class Block(nn.Module):
    # CausalSelfAttention + MLP with residual connections
    # Uses Flash Attention when available

class CausalSelfAttention(nn.Module):
    # Multi-head causal self-attention
    # Supports Flash Attention via torch's scaled_dot_product_attention
```

### 4. Training Loop (`train.py`)

```bash
# Reproduce GPT-2 (124M) — requires 8×A100 40GB
torchrun --standalone --nproc_per_node=8 train.py config/train_gpt2.py
# ~4 days, val loss ~2.85 (matches GPT-2)

# Multi-node training
# Master node:
torchrun --nproc_per_node=8 --nnodes=2 --node_rank=0 \
    --master_addr=123.456.123.456 --master_port=1234 train.py
# Worker node:
torchrun --nproc_per_node=8 --nnodes=2 --node_rank=1 \
    --master_addr=123.456.123.456 --master_port=1234 train.py
```

**Key training features:**
- Gradient accumulation
- Cosine learning rate decay with warmup
- Gradient clipping
- Mixed precision (bfloat16)
- torch.compile support
- DDP for multi-GPU
- W&B logging

### 5. Configuration System

Configs are Python files, not YAML:

```python
# config/train_gpt2.py
out_dir = 'out'
eval_interval = 2000
log_interval = 1
eval_iters = 200
eval_only = False
always_save_checkpoint = True
init_from = 'scratch'  # or 'resume' or 'gpt2*'

# Data
gradient_accumulation_steps = 5 * 8
batch_size = 12
block_size = 1024

# Model
n_layer = 12
n_head = 12
n_embd = 768
dropout = 0.0
bias = True

# Training
learning_rate = 6e-4
max_iters = 600000
weight_decay = 1e-1
beta1 = 0.9
beta2 = 0.95
grad_clip = 1.0

# LR schedule
decay_lr = True
warmup_iters = 2000
lr_decay_iters = 600000
min_lr = 6e-5

# System
device = 'cuda'
dtype = 'bfloat16'
compile = True
```

### 6. Data Preparation

**OpenWebText (GPT-2 reproduction):**
```bash
python data/openwebtext/prepare.py
# Downloads and tokenizes, creates train.bin (~9B tokens) and val.bin
```

**Custom dataset:**
```bash
# 1. Prepare a single text file
# 2. Use prepare.py as template — it:
#    - Reads text
#    - Tokenizes with tiktoken (GPT-2 BPE)
#    - Saves as uint16 numpy to .bin files
# 3. Update config with your data directory
```

**Tokenization format:**
- Raw sequence of uint16 token IDs in .bin files
- Single continuous stream (no document boundaries)
- Uses `tiktoken` for GPT-2 BPE tokenizer

### 7. Finetuning

```bash
# Start from pretrained GPT-2
python train.py config/finetune_shakespeare.py

# Key config changes for finetuning:
init_from = 'gpt2-xl'  # or gpt2, gpt2-medium, gpt2-large
dropout = 0.1           # Add regularization for small datasets
learning_rate = 1e-5    # Much lower LR
max_iters = 100         # Few iterations
```

**Config template for finetuning:**
```python
# config/finetune_shakespeare.py
init_from = 'gpt2-xl'
out_dir = 'out-shakespeare'
eval_interval = 5
eval_iters = 40
log_interval = 1

# Override these from train_gpt2.py
gradient_accumulation_steps = 1
batch_size = 2
block_size = 1024

# Finetune-specific
learning_rate = 1e-5
max_iters = 100
lr_decay_iters = 100
dropout = 0.1
```

### 8. Sampling

```bash
# Sample from trained model
python sample.py --out_dir=out-shakespeare-char

# Key sampling parameters
python sample.py \
    --out_dir=out-shakespeare-char \
    --start="What is the answer to life?" \
    --num_samples=5 \
    --max_new_tokens=500 \
    --temperature=0.8 \
    --top_k=200
```

### 9. Model Scales

| Config | Params | Layers | Heads | Emb Dim | GPU Config |
|--------|--------|--------|-------|---------|-------------|
| Shakespeare char | ~10M | 6 | 6 | 384 | 1 GPU, 3 min |
| GPT-2 small | 124M | 12 | 12 | 768 | 8×A100, 4 days |
| GPT-2 medium | 350M | 24 | 16 | 1024 | Modify config |
| GPT-2 large | 774M | 36 | 20 | 1280 | Modify config |
| GPT-2 XL | 1.5B | 48 | 25 | 1600 | Multi-node |

### 10. Evaluation and Benchmarks

```bash
# Benchmark training speed
python train.py config/train_gpt2.py --eval_only

# Loss tracking
# nanoGPT-reported GPT-2 (124M) on OpenWebText: val loss 2.85
# OpenAI GPT-2 (124M) on WebText: val loss ~3.11
# (domain gap between WebText and OpenWebText accounts for difference)

# Custom benchmarks
python train.py config/train_shakespeare_char.py --eval_iters=200
```

## Key Patterns

1. **Configs are Python files, not YAML** — maximum flexibility, easy to diff
2. **`init_from='scratch'|'resume'|'gpt2*'`** — switch between training modes
3. **Always use `--compile=True`** on GPU for ~2x speedup
4. **Gradient accumulation with `gradient_accumulation_steps`** emulates larger batch sizes
5. **Data is raw uint16 .bin files** — tokenized once, loaded via memmap
6. **Meta device init** for large models — doesn't allocate until needed
7. **Weight tying** between `wte` (embedding) and `lm_head` — standard GPT practice
8. **Use `torchrun` for multi-GPU** — not `python -m torch.distributed.launch`

## References

- [nanoGPT Repository](https://github.com/karpathy/nanoGPT)
- [nchat (successor)](https://github.com/karpathy/nanochat)
- [minGPT (predecessor, more educational)](https://github.com/karpathy/minGPT)
- [GPT-2 Paper](https://d4mucfpksywv.cloudfront.net/better-language-models/language_models_are_unsupervised_multitask_learners.pdf)
- [Attention Is All You Need](https://arxiv.org/abs/1706.03762)
- [Video Walkthrough by Andrej Karpathy](https://www.youtube.com/watch?v=kCc8FmEb1nY)
