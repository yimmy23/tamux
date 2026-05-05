---
name: hydra
description: Configuration framework for complex applications (Hydra). Dynamic hierarchical configuration by composition and override via CLI, YAML, and structured configs. Use for ML experiment management, multi-environment deployment, hyperparameter sweeps, and reproducible research workflows. Integrates with PyTorch Lightning, Weights & Biases, MLflow, and Optuna.
license: MIT license
tags: [experiment-configuration, config-composition, hyperparameter-sweeps, reproducible-runs, hydra]
metadata:
    skill-author: K-Dense Inc.
---

# Hydra

## Overview

Hydra is a configuration framework that dynamically creates hierarchical configurations through composition and override. It eliminates hardcoded paths and config files scattered across projects. Use this skill for managing complex ML experiment configurations, multi-environment deployments, hyperparameter sweeps, and reproducible research workflows.

## When to Use This Skill

This skill should be used when:
- Managing complex ML experiment configurations across models, datasets, and hardware
- Running hyperparameter sweeps with structured config overrides
- Switching between dev/staging/prod environments without code changes
- Setting up reproducible research with version-controlled configs
- Integrating configuration across PyTorch Lightning, W&B, and other ML tools
- Running multi-run experiments with different parameter combinations
- Organizing large ML codebases with clean separation of config from code

## Core Capabilities

### 1. Installation

```bash
pip install hydra-core --upgrade
```

### 2. Basic Configuration Pattern

**Directory structure:**
```
conf/
  config.yaml
  db/
    mysql.yaml
    postgresql.yaml
my_app.py
```

**conf/config.yaml:**
```yaml
defaults:
  - db: mysql
  - _self_

db:
  driver: mysql
  host: localhost
  port: 3306
  user: root
```

**my_app.py:**
```python
import hydra
from omegaconf import DictConfig, OmegaConf

@hydra.main(version_base=None, config_path="conf", config_name="config")
def my_app(cfg: DictConfig) -> None:
    print(OmegaConf.to_yaml(cfg))
    print(f"Connecting to {cfg.db.host}:{cfg.db.port}")

if __name__ == "__main__":
    my_app()
```

**CLI overrides:**
```bash
python my_app.py                           # Uses mysql
python my_app.py db=postgresql             # Switch to postgresql
python my_app.py db.host=prod-server       # Override specific value
python my_app.py db=postgresql db.port=5432  # Multiple overrides
```

### 3. Structured Configs (Python dataclasses)

```python
from dataclasses import dataclass, field
from typing import List, Optional
import hydra
from hydra.core.config_store import ConfigStore

@dataclass
class ModelConfig:
    name: str = "resnet50"
    pretrained: bool = True
    num_classes: int = 1000

@dataclass
class TrainingConfig:
    learning_rate: float = 0.001
    batch_size: int = 32
    max_epochs: int = 100
    optimizer: str = "adam"

@dataclass
class DataConfig:
    dataset_path: str = "./data"
    num_workers: int = 4
    image_size: int = 224
    augmentations: List[str] = field(default_factory=lambda: ["flip", "rotate"])

@dataclass
class ExperimentConfig:
    model: ModelConfig = ModelConfig()
    training: TrainingConfig = TrainingConfig()
    data: DataConfig = DataConfig()
    seed: int = 42
    experiment_name: str = "baseline"
    tags: List[str] = field(default_factory=list)

# Register config
cs = ConfigStore.instance()
cs.store(name="base_config", node=ExperimentConfig)

@hydra.main(version_base=None, config_path=None, config_name="base_config")
def run_experiment(cfg: ExperimentConfig) -> None:
    print(f"Model: {cfg.model.name}")
    print(f"LR: {cfg.training.learning_rate}")
    print(f"Batch size: {cfg.training.batch_size}")

if __name__ == "__main__":
    run_experiment()
```

**CLI overrides with structured configs:**
```bash
python experiment.py \
    model=resnet101 \
    training.learning_rate=0.0001 \
    training.batch_size=64 \
    data.image_size=256 \
    experiment_name=experiment_1
```

### 4. Config Groups (Modular Configs)

**Directory:**
```
conf/
  config.yaml
  model/
    resnet50.yaml
    vit_base.yaml
    efficientnet.yaml
  optimizer/
    adam.yaml
    adamw.yaml
    sgd.yaml
  dataset/
    imagenet.yaml
    cifar10.yaml
```

**conf/config.yaml:**
```yaml
defaults:
  - model: resnet50
  - optimizer: adamw
  - dataset: imagenet
  - _self_

training:
  epochs: 100
  mixed_precision: true
```

**conf/model/vit_base.yaml:**
```yaml
name: vit_base_patch16_224
pretrained: true
num_classes: 1000
patch_size: 16
hidden_dim: 768
num_heads: 12
num_layers: 12
```

**Usage:**
```bash
python train.py model=vit_base                              # Switch model
python train.py model=efficientnet optimizer=sgd             # Switch both
python train.py model.vit_base.patch_size=32                 # Nested override
```

### 5. Multi-Run (Hyperparameter Sweeps)

```bash
# Grid sweep: try all combinations
python train.py --multirun \
    training.learning_rate=0.001,0.0001,0.00001 \
    training.batch_size=32,64,128

# Specific combinations
python train.py --multirun \
    model=resnet50,vit_base \
    optimizer=adamw,sgd

# Range sweep
python train.py --multirun \
    seed=1,2,3,4,5

# From a sweep config
python train.py --multirun --config-name=sweep_config
```

### 6. Output Management

Hydra automatically creates timestamped output directories:
```
outputs/
  2024-01-15/
    10-30-45/
      .hydra/          # Hydra config metadata
      train.log        # Application logs
      checkpoints/     # Your artifacts
```

**Access output directory in code:**
```python
import hydra
from hydra.utils import get_original_cwd, to_absolute_path

@hydra.main(...)
def my_app(cfg):
    # Hydra changes working directory to output dir
    print(os.getcwd())                    # .../outputs/2024-01-15/10-30-45/
    print(get_original_cwd())             # Original working directory
```

### 7. PyTorch Lightning Integration

**Config:**
```yaml
defaults:
  - model: resnet50
  - trainer: default
  - data: imagenet
  - _self_

seed: 42
```

**Training script:**
```python
@hydra.main(version_base=None, config_path="conf", config_name="config")
def train(cfg: DictConfig):
    pl.seed_everything(cfg.seed)
    model = MyLightningModule(cfg.model)
    datamodule = MyDataModule(cfg.data)
    trainer = pl.Trainer(**cfg.trainer)
    trainer.fit(model, datamodule)
```

### 8. W&B / MLflow Logging Integration

```python
@hydra.main(...)
def train(cfg: DictConfig):
    # W&B
    import wandb
    wandb.init(project=cfg.wandb.project, config=OmegaConf.to_container(cfg))

    # MLflow
    import mlflow
    mlflow.log_params(OmegaConf.to_container(cfg))
```

### 9. Instantiation (hydra.utils.instantiate)

```python
# Config
# model:
#   _target_: torch.optim.AdamW
#   lr: 0.001
#   weight_decay: 0.01

from hydra.utils import instantiate

@hydra.main(...)
def train(cfg):
    optimizer = instantiate(cfg.optimizer)  # Creates AdamW(lr=0.001, weight_decay=0.01)
    model = instantiate(cfg.model)
    scheduler = instantiate(cfg.scheduler, optimizer=optimizer)
```

**Recursive instantiation:**
```yaml
model:
  _target_: mylib.models.ResNetClassifier
  backbone:
    _target_: torchvision.models.resnet50
    pretrained: true
  num_classes: 1000
```

### 10. Resolvers (Dynamic Value Resolution)

```python
# Register a custom resolver
from omegaconf import OmegaConf

OmegaConf.register_new_resolver("sum", lambda x, y: x + y)
OmegaConf.register_new_resolver("eval", eval)

# Use in YAML
# total_steps: ${sum:${train.epochs},${train.warmup_epochs}}
# batch_size_gb: ${eval:'int(${batch_size} * ${image_size}**2 * 3 * 4 / 1e9)'}
```

**Built-in resolvers:**
```yaml
output_dir: ${hydra:runtime.output_dir}
now: ${now:%Y-%m-%d_%H-%M-%S}
# Path relative to config file
data_path: ${oc.env:DATA_PATH,/default/path}
```

## Key Patterns

1. **Separate config from code** — all tunable params go in YAML/dataclasses
2. **Use config groups** for model/dataset/optimizer families — modular swapping
3. **CLI overrides are the source of truth** — YAML provides defaults, CLI finalizes
4. **Use `instantiate()`** for object creation from config — reduces boilerplate
5. **Timestamped output dirs** are automatic — no need to manage manually
6. **Multi-run for sweeps** — `--multirun` plus comma-separated values
7. **Check in config files** — they ARE your experiment documentation

## References

- [Hydra Documentation](https://hydra.cc/docs/intro/)
- [Structured Configs Tutorial](https://hydra.cc/docs/tutorials/structured_config/intro/)
- [OmegaConf Documentation](https://omegaconf.readthedocs.io/)
- [lightning-hydra-template](https://github.com/ashleve/lightning-hydra-template) — full ML template
- [hydra-zen](https://github.com/mit-ll-responsible-ai/hydra-zen) — Pythonic Hydra utilities
