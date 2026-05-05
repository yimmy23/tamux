---
name: wandb
description: "Weights & Biases — ML experiment tracking and visualization. Log metrics, hyperparameters, model checkpoints, and artifacts. Collaborative dashboards, sweep hyperparameter search, and model registry."
tags: [experiment-tracking, hyperparameter-sweeps, model-observability, training-visualization, wandb]
---
## Overview

Weights & Biases (wandb) tracks ML experiments with rich visualizations, hyperparameter sweeps, dataset versioning, model registry, and collaborative dashboards. Industry standard for experiment tracking across ML teams.

## Installation

```bash
uv pip install wandb
wandb login  # authenticate with API key
```

## Experiment Tracking

```python
import wandb

wandb.init(project="my_project", config={
    "learning_rate": 0.001,
    "batch_size": 32,
    "architecture": "transformer",
})
for epoch in range(10):
    loss = train_one_epoch()
    wandb.log({"train_loss": loss, "val_loss": val_loss, "epoch": epoch})
wandb.finish()
```

## Hyperparameter Sweep

```python
sweep_config = {
    "method": "bayes",
    "metric": {"name": "val_loss", "goal": "minimize"},
    "parameters": {"lr": {"min": 1e-5, "max": 1e-2}},
}
sweep_id = wandb.sweep(sweep_config, project="my_project")
wandb.agent(sweep_id, function=train_function, count=20)
```

## References
- [W&B docs](https://docs.wandb.ai/)
- [W&B GitHub](https://github.com/wandb/wandb)