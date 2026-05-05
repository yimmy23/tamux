---
name: nanogpt
description: "Minimal GPT pretraining and fine-tuning (nanoGPT). The simplest, fastest repository for training medium-sized GPTs with ~300-line model.py and ~300-line train.py. Reproduces GPT-2 (124M) on OpenWebText. Supports DDP multi-GPU/multi-node, character-level training, weight loading from HuggingFace GPT-2 checkpoints, and simple finetuning. Note: superseded by nanochat for new projects; this repo remains valuable as a reference implementation and learning tool."
license: MIT license
tags: [gpt-pretraining, autoregressive-language-modeling, ddp-training, checkpoint-finetuning, nanogpt]
metadata:
    skill-author: K-Dense Inc.
-----|--------|--------|-------|---------|-------------|
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
