---
name: zenml
description: "ZenML — ML pipeline orchestration. Connect ML tools (MLflow, W&B, Airflow, Kubeflow) into portable pipelines. Caching, versioning, and cloud-agnostic stack management for production ML workflows."
tags: [ml-pipeline-orchestration, reproducible-pipelines, stack-management, pipeline-caching, zenml]
---
## Overview

ZenML is an MLOps framework for portable, reproducible ML pipelines. It provides a standardized pipeline abstraction with built-in tracking, caching, artifact management, and integration with major ML and cloud tools.

## Installation

```bash
uv pip install zenml
```

## Basic Pipeline

```python
from zenml import pipeline, step

@step
def load_data() -> dict:
    return {"data": [1, 2, 3], "labels": [0, 1, 0]}

@step
def train_model(data: dict) -> str:
    return f"Trained on {len(data['data'])} samples"

@pipeline
def training_pipeline():
    data = load_data()
    model = train_model(data)

training_pipeline()
```

## Caching

```python
# Steps are automatically cached — rerunning only changes
@step(enable_cache=True)
def preprocess(raw: dict) -> dict:
    return {"features": [x * 2 for x in raw["data"]]}

# Changing parameters invalidates cache
@step
def train_with_params(data: dict, lr: float = 0.01) -> str:
    return f"Trained with lr={lr}"
```

## Stack and Deploy

```bash
zenml stack register my_stack -o default -a default
zenml stack set my_stack
zenml deploy
```

## References
- [ZenML docs](https://docs.zenml.io/)
- [ZenML GitHub](https://github.com/zenml-io/zenml)