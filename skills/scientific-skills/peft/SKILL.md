---
name: peft
description: "Parameter-Efficient Fine-Tuning (PEFT) library. LoRA, QLoRA, AdaLoRA, IA3, Prefix Tuning, P-Tuning, Prompt Tuning. Fine-tune large models with minimal memory overhead. Hugging Face ecosystem integration."
tags: [peft, lora, qlora, fine-tuning, llm, huggingface, parameter-efficient, zorai]
---
## Overview

PEFT (Parameter-Efficient Fine-Tuning) adapts large pretrained models by training only a small subset of parameters. Supports LoRA, QLoRA, AdaLoRA, IA3, Prefix Tuning, P-Tuning, and Prompt Tuning. Reduces GPU memory by 4-16x compared to full fine-tuning.

## Installation

```bash
uv pip install peft
```

## LoRA

```python
from transformers import AutoModelForCausalLM
from peft import LoraConfig, get_peft_model, TaskType

model = AutoModelForCausalLM.from_pretrained("Qwen/Qwen2.5-1.5B-Instruct")
peft_config = LoraConfig(
    r=16, lora_alpha=32,
    target_modules=["q_proj", "k_proj", "v_proj", "o_proj"],
    task_type=TaskType.CAUSAL_LM,
)
model = get_peft_model(model, peft_config)
model.print_trainable_parameters()
```

## Save & Merge

```python
model.save_pretrained("adapter")
from peft import PeftModel
base = AutoModelForCausalLM.from_pretrained("Qwen/Qwen2.5-1.5B-Instruct")
merged = PeftModel.from_pretrained(base, "adapter").merge_and_unload()
```

## QLoRA

```python
from transformers import BitsAndBytesConfig
model = AutoModelForCausalLM.from_pretrained("Qwen/Qwen2.5-1.5B-Instruct",
    quantization_config=BitsAndBytesConfig(load_in_4bit=True), device_map="auto")
model = get_peft_model(model, peft_config)
```

## References
- [PEFT docs](https://huggingface.co/docs/peft)
- [PEFT GitHub](https://github.com/huggingface/peft)