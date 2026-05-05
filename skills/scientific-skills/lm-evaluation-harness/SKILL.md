---
name: lm-evaluation-harness
description: LLM evaluation framework (EleutherAI lm-evaluation-harness). Unified benchmark evaluation for language models with 200+ tasks, support for HuggingFace transformers, vLLM, SGLang, OpenAI API, GGUF, and custom models. Used by HuggingFace Open LLM Leaderboard. Covers MMLU, HellaSwag, ARC, GSM8K, HumanEval, BBH, TruthfulQA, and more.
license: MIT license
tags: [llm-evaluation, benchmark-suite, leaderboard-eval, few-shot-eval, lm-evaluation-harness]
metadata:
    skill-author: K-Dense Inc.
---|
| `leaderboard` | MMLU, ARC, HellaSwag, TruthfulQA, Winogrande, GSM8K | Open LLM Leaderboard suite |
| `mmlu` | 57 subjects (STEM, humanities, social sciences) | World knowledge + reasoning |
| `gsm8k` | Grade-school math word problems | Mathematical reasoning |
| `hellaswag` | Commonsense NLI | Commonsense reasoning |
| `arc_challenge` | Science exam questions | Scientific reasoning |
| `truthfulqa` | Adversarial questions | Truthfulness/hallucination |
| `humaneval` | Python code generation | Coding ability |
| `bigbench` | 200+ BIG-Bench tasks | Broad capability assessment |
| `ifeval` | Instruction-following | Instruction adherence |

**Run leaderboard suite:**
```bash
lm-eval run \
    --model hf \
    --model_args pretrained=your-model \
    --tasks leaderboard \
    --device cuda:0 \
    --batch_size auto
```

### 4. Python API

```python
from lm_eval import simple_evaluate

results = simple_evaluate(
    model="hf",
    model_args={"pretrained": "meta-llama/Llama-3.2-1B"},
    tasks=["mmlu", "hellaswag", "gsm8k"],
    device="cuda:0",
    batch_size="auto",
    limit=100,  # Optional: limit samples per task
)

# Access results
for task, metrics in results["results"].items():
    print(f"{task}: {metrics}")

# Formatted table
print(results["configs"])
print(results["samples"])  # Per-sample outputs
```

### 5. Evaluating LoRA / PEFT Adapters

```bash
lm-eval run \
    --model hf \
    --model_args pretrained=meta-llama/Llama-3.2-1B,peft=/path/to/lora_adapter \
    --tasks mmlu \
    --device cuda:0
```

In Python:
```python
from lm_eval import simple_evaluate

results = simple_evaluate(
    model="hf",
    model_args={
        "pretrained": "meta-llama/Llama-3.2-1B",
        "peft": "/path/to/lora_adapter",
    },
    tasks=["mmlu"],
)
```

### 6. Multi-GPU Evaluation

**Data-parallel (model fits on single GPU):**
```bash
accelerate launch -m lm_eval \
    --model hf \
    --model_args pretrained=model-name \
    --tasks lambada_openai,arc_easy \
    --batch_size 16
```

**Model-parallel (model too large for one GPU):**
```bash
lm-eval run \
    --model hf \
    --model_args pretrained=model-name,parallelize=True \
    --tasks mmlu \
    --batch_size 8
```

**Both (data + model parallel):**
```bash
accelerate launch --multi_gpu --num_processes 4 \
    -m lm_eval \
    --model hf \
    --model_args pretrained=model-name,parallelize=True \
    --tasks mmlu \
    --batch_size 8
```

### 7. API Model Evaluation

**OpenAI-compatible API:**
```bash
export OPENAI_API_KEY=your-key

lm-eval run \
    --model openai-completions \
    --model_args model=gpt-4o,base_url=https://api.openai.com/v1/completions \
    --tasks mmlu \
    --batch_size 32
```

**Local server (vLLM served):**
```bash
lm-eval run \
    --model local-completions \
    --model_args model=local-model,base_url=http://localhost:8000/v1/completions \
    --tasks mmlu
```

### 8. Custom Tasks (YAML Config)

Create a custom task at `lm_eval/tasks/my_task/my_task.yaml`:
```yaml
task: my_custom_task
dataset_path: my-dataset
dataset_name: default
output_type: multiple_choice
training_split: train
validation_split: validation
doc_to_text: "Question: {{question}}\nA. {{choices[0]}}\nB. {{choices[1]}}\nC. {{choices[2]}}\nD. {{choices[3]}}\nAnswer:"
doc_to_target: "{{answer}}"
doc_to_choice: "{{choices}}"
metric_list:
  - metric: acc
```

Run it:
```bash
lm-eval run --model hf --model_args pretrained=model-name --tasks my_custom_task
```

### 9. Few-Shot Configuration

```bash
lm-eval run \
    --model hf \
    --model_args pretrained=model-name \
    --tasks mmlu \
    --num_fewshot 5 \
    --fewshot_random_seed 42
```

### 10. Logging and Output Formats

```bash
# JSON output
lm-eval run --model hf --model_args pretrained=model-name \
    --tasks mmlu --output_path results/

# W&B logging
lm-eval run --model hf --model_args pretrained=model-name \
    --tasks mmlu --wandb_args project=eval-runs
```

## Key Evaluation Tips

1. **Use `--batch_size auto`** — automatic batch size detection maximizes throughput
2. **Always set a `--seed` for reproducibility** across evaluation runs
3. **Prefer vLLM backend for models >7B** — 5-10x faster than HF
4. **Use `--limit 100` during development** to test task setup quickly
5. **mmlu uses 5-shot by default**, while most tasks are 0-shot
6. **Install model backends separately** — base package is lightweight by design
7. **GGUF eval requires explicit tokenizer path** — skip this and loading may hang

## References

- [Full CLI Reference](https://github.com/EleutherAI/lm-evaluation-harness/blob/main/docs/interface.md)
- [Configuration Guide](https://github.com/EleutherAI/lm-evaluation-harness/blob/main/docs/config_files.md)
- [Python API Documentation](https://github.com/EleutherAI/lm-evaluation-harness/blob/main/docs/python-api.md)
- [Task Guide](https://github.com/EleutherAI/lm-evaluation-harness/tree/main/lm_eval/tasks)
- [Open LLM Leaderboard](https://huggingface.co/spaces/HuggingFaceH4/open_llm_leaderboard)
