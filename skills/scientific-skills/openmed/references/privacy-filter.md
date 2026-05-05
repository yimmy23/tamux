---
name: openmed-privacy-filter
description: "OpenAI Privacy Filter and Nemotron-PII fine-tune families. PyTorch and MLX backends, automatic fallback routing, and 8-bit quantization."
tags: [openmed, privacy-filter, openai, nemotron, mlx, pii]
---

# Privacy Filter Family

OpenMed ships two Privacy Filter variants sharing the same architecture with different training data.

## Variants

| Variant | Training Data | PyTorch (CPU/CUDA) | MLX Full | MLX 8-bit |
|---|---|---|---|---|
| OpenAI baseline | OpenAI PII set | `openai/privacy-filter` | `OpenMed/privacy-filter-mlx` | `OpenMed/privacy-filter-mlx-8bit` |
| Nemotron-PII | NVIDIA Nemotron-PII | `OpenMed/privacy-filter-nemotron` | `OpenMed/privacy-filter-nemotron-mlx` | `OpenMed/privacy-filter-nemotron-mlx-8bit` |

## Usage (PyTorch)

```python
from openmed import extract_pii, deidentify

text = "Patient Sarah Connor (DOB: 03/15/1985) at MRN 4471882."

# OpenAI baseline
result = extract_pii(text, model_name="openai/privacy-filter")

# Nemotron fine-tune
result = extract_pii(text, model_name="OpenMed/privacy-filter-nemotron")

# De-identify
masked = deidentify(text, model_name="OpenMed/privacy-filter-nemotron", method="mask")
```

## Usage (MLX — Apple Silicon)

```python
from openmed import extract_pii

# Full precision
extract_pii(text, model_name="OpenMed/privacy-filter-mlx")

# 8-bit quantized
extract_pii(text, model_name="OpenMed/privacy-filter-mlx-8bit")

# Nemotron MLX
extract_pii(text, model_name="OpenMed/privacy-filter-nemotron-mlx")
```

## Cross-Platform Fallback

MLX model names work everywhere. On non-Apple hosts:
- `OpenMed/privacy-filter-mlx*` → falls back to `openai/privacy-filter`
- `OpenMed/privacy-filter-nemotron-mlx*` → falls back to `OpenMed/privacy-filter-nemotron`

## References
- docs/anonymization.md
- examples/privacy_filter_unified.py
- examples/obfuscation_demo.py
