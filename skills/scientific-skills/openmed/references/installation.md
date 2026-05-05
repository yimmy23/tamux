---
name: openmed-installation
description: "OpenMed installation guide: PyPI, editable installs, extras (HF, service, MLX), Docker, and Swift Package Manager for Apple platforms."
tags: [openmed, installation, setup, docker, mlx, swift]
---

# Installation

OpenMed supports multiple installation paths depending on your use case and platform.

## From Source (Recommended)

```bash
git clone https://github.com/maziyarpanahi/openmed.git
cd openmed

# Basic installation (Hugging Face backend)
uv pip install -e ".[hf]"

# With REST API service dependencies
uv pip install -e ".[hf,service]"

# Apple Silicon acceleration
uv pip install -e ".[mlx]"
```

## Published Release

```bash
uv pip install "openmed[hf]"
uv pip install "openmed[hf,service]"
uv pip install "openmed[mlx]"
```

## Docker

```bash
docker build -t openmed:1.2.0 .
docker run --rm -p 8080:8080 -e OPENMED_PROFILE=prod openmed:1.2.0
```

## Swift (OpenMedKit)

```swift
dependencies: [
    .package(url: "https://github.com/maziyarpanahi/openmed.git", from: "1.2.0"),
]
```

Supports MLX on Apple Silicon and CoreML fallback for PII token classification and Privacy Filter.

## Extras Explained

| Extra | Includes |
|---|---|
| `[hf]` | transformers, torch, tokenizers |
| `[service]` | FastAPI, uvicorn |
| `[mlx]` | mlx, mlx-lm, tiktoken, huggingface-hub |
