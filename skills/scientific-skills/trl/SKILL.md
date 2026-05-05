---
name: trl
description: "Transformer Reinforcement Learning library (TRL). Supervised fine-tuning (SFT), reward modeling, PPO, DPO, KTO, GRPO for RLHF. Process reward models and language model alignment."
tags: [rlhf-post-training, dpo-training, ppo-alignment, supervised-finetuning, trl]
---
## Overview

TRL (Transformer Reinforcement Learning) is Hugging Face's library for RLHF — SFT, reward modeling, PPO, DPO, KTO, and GRPO. It's the standard post-training toolkit for aligning language models with human preferences.

## Installation

```bash
uv pip install trl
```

## SFT

```python
from trl import SFTTrainer
from transformers import AutoModelForCausalLM, AutoTokenizer

model = AutoModelForCausalLM.from_pretrained("Qwen/Qwen2.5-1.5B-Instruct")
tokenizer = AutoTokenizer.from_pretrained("Qwen/Qwen2.5-1.5B-Instruct")

trainer = SFTTrainer(
    model=model, tokenizer=tokenizer,
    train_dataset=dataset,
    args=dict(per_device_train_batch_size=4, learning_rate=2e-5, max_seq_length=2048),
)
trainer.train()
```

## DPO

```python
from trl import DPOTrainer

dpo = DPOTrainer(
    model=model, ref_model=ref_model, tokenizer=tokenizer,
    train_dataset=preference_dataset,
    args=dict(per_device_train_batch_size=4, max_length=2048),
)
dpo.train()
```

## References
- [TRL docs](https://huggingface.co/docs/trl)
- [TRL GitHub](https://github.com/huggingface/trl)