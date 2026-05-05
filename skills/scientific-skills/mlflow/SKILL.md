---
name: mlflow
description: "MLflow — open-source MLOps platform. Experiment tracking, model registry, packaging, deployment, and evaluation. Multi-cloud ML workflows with reproducible runs and artifact logging."
tags: [mlflow, mlops, experiment-tracking, model-registry, deployment, python, zorai]
---
## Overview

MLflow is the leading open-source MLOps platform covering experiment tracking, model registry, packaging (MLflow Models format), and deployment (MLflow Serving). Supports PyTorch, TensorFlow, scikit-learn, ONNX, XGBoost, and custom models across cloud and on-prem.

## Installation

```bash
uv pip install mlflow
```

## Experiment Tracking

```python
import mlflow
mlflow.set_experiment("my_project")
with mlflow.start_run(run_name="experiment_1"):
    mlflow.log_param("learning_rate", 0.01)
    mlflow.log_param("batch_size", 32)
    mlflow.log_metric("accuracy", 0.92)
    mlflow.log_metric("loss", 0.35)
    mlflow.log_artifact("model.pth")
    mlflow.pytorch.log_model(model, "model")
```

## Model Registry & Serving

```python
mlflow.register_model("runs:/<run_id>/model", "MyModel")
```

```bash
mlflow models serve --model-uri models:/MyModel/1 --port 5001
mlflow ui --host 0.0.0.0 --port 5000
```

## References
- [MLflow docs](https://mlflow.org/docs/latest/)
- [MLflow GitHub](https://github.com/mlflow/mlflow)