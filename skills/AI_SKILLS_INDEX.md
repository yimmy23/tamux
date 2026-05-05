# AI Skills Index

Curated index of local skills in `skills/` that are relevant to AI model training, architecture design, evaluation, RL, and adjacent MLOps workflows.

This index is organized by **workflow need**, not alphabetically, so an agent can choose the right skill quickly.

---

## 1. Resource Planning / Systems Constraints

Use these **before** expensive training or evaluation jobs.

| Skill | License | Best for | Path |
|---|---|---|---|
| `get-available-resources` | MIT | Inspect CPU/GPU/RAM/disk before training or large eval runs | `scientific-skills/get-available-resources/` |
| `optimize-for-gpu` | not specified in frontmatter | Speeding up Python/ML/data pipelines on NVIDIA GPUs | `scientific-skills/optimize-for-gpu/` |

### Recommended choice
- Start with `get-available-resources` when the workload size or machine fit is unclear.
- Use `optimize-for-gpu` when the bottleneck is clearly compute-heavy and GPU acceleration is feasible.

---

## 2. Experiment Configuration / Reproducibility / Sweeps

| Skill | License | Best for | Path |
|---|---|---|---|
| `hydra` | MIT | hierarchical experiment config, CLI overrides, multirun sweeps, reproducible outputs | `scientific-skills/hydra/` |
| `hydra-zen` | MIT | Python-first structured config generation, less Hydra boilerplate | `scientific-skills/hydra-zen/` |
| `optuna` | MIT | hyperparameter optimization, pruning, multi-objective search | `scientific-skills/optuna/` |

### Recommended choice
- Use `hydra` for config composition and reproducible run management.
- Use `hydra-zen` when the project is Python-heavy and YAML should be minimized.
- Use `optuna` for actual search/tuning once the base training pipeline is stable.

---

## 3. General Deep Learning Training

| Skill | License | Best for | Path |
|---|---|---|---|
| `pytorch-lightning` | Apache-2.0 | scalable training loops, callbacks, logging, multi-GPU strategies | `scientific-skills/pytorch-lightning/` |
| `transformers` | Apache-2.0 | pretrained/fine-tuned transformer models for NLP/CV/audio/multimodal | `scientific-skills/transformers/` |
| `nanogpt` | MIT | minimal GPT pretraining / finetuning reference implementation | `scientific-skills/nanogpt/` |

### Recommended choice
- Use `pytorch-lightning` for production-ish or scalable PyTorch training loops.
- Use `transformers` when the task is model-hub / pretrained-model centric.
- Use `nanogpt` when the need is **understanding or hacking raw GPT pretraining** with minimal abstraction.

---

## 4. LLM Evaluation / Benchmarking / Regression Testing

| Skill | License | Best for | Path |
|---|---|---|---|
| `lm-evaluation-harness` | MIT | standard benchmark suites, leaderboard-style eval, few-shot benchmarking | `scientific-skills/lm-evaluation-harness/` |
| `lighteval` | MIT | multi-backend eval, multilingual benchmarks, sample-level analysis | `scientific-skills/lighteval/` |
| `openai-evals` | MIT | custom eval registries, model-graded evals, prompt/system regression testing | `scientific-skills/openai-evals/` |

### Recommended choice
- Use `lm-evaluation-harness` for classic academic benchmark runs and open leaderboard-style comparison.
- Use `lighteval` when backend flexibility or multilingual coverage matters.
- Use `openai-evals` when evaluating **product behavior**, prompts, or model-graded quality on custom datasets.

---

## 5. Reinforcement Learning Algorithms

| Skill | License | Best for | Path |
|---|---|---|---|
| `stable-baselines3` | MIT | standard RL baselines with strong docs and familiar API | `scientific-skills/stable-baselines3/` |
| `cleanrl` | MIT | readable single-file RL implementations for modification/research | `scientific-skills/cleanrl/` |
| `pufferlib` | MIT | high-throughput, parallel, multi-agent RL at scale | `scientific-skills/pufferlib/` |

### Recommended choice
- Use `stable-baselines3` for quick baseline experiments and standard control tasks.
- Use `cleanrl` when you need to inspect or modify the algorithm internals directly.
- Use `pufferlib` when throughput and scale matter more than API simplicity.

---

## 6. Reinforcement Learning Environments

### 6.1 Single-agent / control / classic benchmarks

| Skill | License | Best for | Path |
|---|---|---|---|
| `gymnasium` | MIT | canonical single-agent RL environment API and wrappers | `scientific-skills/gymnasium/` |

### 6.2 Multi-agent RL environments

| Skill | License | Best for | Path |
|---|---|---|---|
| `pettingzoo` | MIT | multi-agent RL environments, AEC + parallel APIs | `scientific-skills/pettingzoo/` |

### 6.3 Robotics / multi-task / meta-RL benchmarks

| Skill | License | Best for | Path |
|---|---|---|---|
| `metaworld` | MIT | robotic manipulation, multi-task RL, meta-RL adaptation benchmarks | `scientific-skills/metaworld/` |

### 6.4 Text-native / LLM RL environments

| Skill | License | Best for | Path |
|---|---|---|---|
| `textarena` | MIT | self-play, strategic reasoning, text-game RL for LLMs | `scientific-skills/textarena/` |

### Recommended choice
- Use `gymnasium` for standard single-agent pipelines.
- Use `pettingzoo` for multi-agent games and MARL.
- Use `metaworld` for robotics-style manipulation benchmarks.
- Use `textarena` for text-native self-play and LLM reasoning environments.

---

## 7. Vision Data Augmentation

| Skill | License | Best for | Path |
|---|---|---|---|
| `albumentations` | MIT | image augmentation across classification, segmentation, detection, keypoints | `scientific-skills/albumentations/` |

### Note
`albumentations` is MIT but the classic repo is no longer actively maintained. Still useful; just treat it as stable/legacy rather than actively evolving.

---

## 8. Common Workflow Compositions

### A. LLM pretraining / minimal GPT experimentation
1. `get-available-resources`
2. `hydra` or `hydra-zen`
3. `nanogpt`
4. `optuna` (only after baseline is stable)
5. `lm-evaluation-harness` or `lighteval`

### B. Transformer finetuning pipeline
1. `get-available-resources`
2. `transformers`
3. `pytorch-lightning` (if custom loop orchestration is needed)
4. `hydra`
5. `optuna`
6. `lm-evaluation-harness` / `openai-evals`

### C. Standard single-agent RL experiment
1. `gymnasium`
2. `stable-baselines3` or `cleanrl`
3. `hydra`
4. `optuna`

### D. Multi-agent RL or self-play
1. `pettingzoo` or `textarena`
2. `cleanrl` or `pufferlib`
3. `hydra`
4. `optuna`

### E. Robotics / transfer / meta-RL
1. `metaworld`
2. `cleanrl` / custom PyTorch loop
3. `hydra`
4. `optuna`
5. `lighteval` / custom analysis as needed

### F. Vision training pipeline
1. `albumentations`
2. `pytorch-lightning` or `transformers`
3. `hydra`
4. `optuna`

---

## 9. MIT Skills Added in This Curation Pass

### Tier 1
- `optuna`
- `lm-evaluation-harness`
- `gymnasium`
- `pettingzoo`
- `cleanrl`
- `hydra`
- `albumentations`

### Tier 2
- `nanogpt`
- `openai-evals`
- `lighteval`
- `metaworld`
- `textarena`
- `hydra-zen`

---

## 10. Important Gaps Still Present Under Strict MIT-Only Policy

These domains are still not fully covered by MIT-only additions because many best-in-class repos are Apache-2.0 or other licenses:

### RLHF / post-training / alignment
Strong non-MIT candidates:
- TRL
- OpenRLHF
- veRL
- alignment-handbook

### LLM serving / inference
Strong non-MIT candidates:
- vLLM
- SGLang
- TGI

### Distributed training systems
Strong non-MIT candidates:
- DeepSpeed
- Ray
- LitGPT

### Data/versioning/MLOps infra
Strong non-MIT candidates:
- DVC
- MLflow ecosystem pieces
- ZenML

If permissive-but-not-MIT licenses are acceptable, the practical skill set can be materially improved by adding these.

---

## 11. Skill Selection Heuristics for Agents

- If the user says **benchmark / leaderboard / MMLU / GSM8K**, start with `lm-evaluation-harness` or `lighteval`.
- If the user says **prompt regressions / model-graded QA / private eval dataset**, start with `openai-evals`.
- If the user says **hyperparameters / pruning / search / study**, start with `optuna`.
- If the user says **configs / sweeps / reproducibility / experiment folders**, start with `hydra`.
- If the user says **Python-first Hydra / no YAML / dataclass configs**, start with `hydra-zen`.
- If the user says **train GPT from scratch / understand GPT internals**, start with `nanogpt`.
- If the user says **single-agent RL**, start with `gymnasium` + `stable-baselines3`.
- If the user says **modify RL algorithm internals**, prefer `cleanrl`.
- If the user says **multi-agent RL**, prefer `pettingzoo`.
- If the user says **robotic manipulation / meta-RL**, prefer `metaworld`.
- If the user says **text self-play / LLM game environments**, prefer `textarena`.
- If the user says **image augmentation / bboxes / masks / keypoints**, prefer `albumentations`.

---

## 12. Maintenance Notes

- New skills added in this pass were written with **agentskills.io-compatible YAML frontmatter** and explicit semantic `tags: [...]` intended to improve skill discovery.
- Several older pre-existing skills in the repo still use broader or noisier tags; they may benefit from a later normalization pass.
