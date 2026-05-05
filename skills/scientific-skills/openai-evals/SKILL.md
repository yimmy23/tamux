---
name: openai-evals
description: LLM evaluation framework and registry (OpenAI Evals). Framework for evaluating LLMs and LLM-based systems with a registry of community-contributed eval templates. Supports model-graded evals, classification, simple completion matching, and custom completion functions. Use for systematic LLM quality testing, regression detection, and prompt engineering validation.
license: MIT license
tags: [model-graded-evals, regression-testing, prompt-validation, eval-registry, openai-evals]
metadata:
    skill-author: K-Dense Inc.
---

# OpenAI Evals

## Overview

OpenAI Evals provides a framework for evaluating LLMs and systems built with LLMs. It includes a community-contributed registry of evals plus the ability to create custom, private evals. Use this skill for model quality testing, regression detection, prompt engineering validation, and building evaluation pipelines for production LLM systems.

## When to Use This Skill

This skill should be used when:
- Testing how different model versions affect your use case
- Building regression tests for LLM-based features
- Validating prompt engineering changes systematically
- Creating model-graded evaluations without writing code
- Contributing evals to the community registry
- Running standardized LLM quality benchmarks

## Core Capabilities

### 1. Installation

```bash
# Clone with Git-LFS support
git lfs install
git clone https://github.com/openai/evals
cd evals
git lfs fetch --all
git lfs pull
pip install -e .
```

Or via pip:
```bash
pip install evals
```

### 2. Running Evals

```bash
# Run a single eval
oaieval gpt-4o mmlu

# Run with custom parameters
oaieval gpt-4o mmlu --max_samples 100

# Run with recording for debugging
oaieval gpt-4o mmlu --record_path /tmp/record.jsonl

# Run model-graded eval
oaieval gpt-4o test-model-graded
```

### 3. Eval Templates (No-Code Evals)

Three templates let you create evals without writing Python:

**Basic Eval Template** (classification):
```yaml
# my_eval.yaml
my-classification-eval:
  id: my-classification-eval.dev.v0
  metrics: [accuracy]
  description: My custom classification eval
my-classification-eval.dev.v0:
  class: evals.elsuite.basic.match:Match
  args:
    samples_jsonl: my_data/samples.jsonl
```

**Model-Graded Eval Template** (LLM-as-judge):
```yaml
my-model-graded-eval:
  id: my-model-graded-eval.dev.v0
  metrics: [accuracy]
  description: Model-graded quality eval
my-model-graded-eval.dev.v0:
  class: evals.elsuite.modelgraded.classify:ModelBasedClassify
  args:
    samples_jsonl: my_data/samples.jsonl
    eval_type: cot_classify
    model graded_spec: fact
```

**Match Eval Template** (exact/fuzzy match):
```yaml
my-match-eval:
  id: my-match-eval.dev.v0
  metrics: [accuracy, f1_score]
my-match-eval.dev.v0:
  class: evals.elsuite.basic.match:Match
  args:
    samples_jsonl: my_data/samples.jsonl
```

### 4. Data Format

```jsonl
{"input": [{"role": "system", "content": "You are a helpful assistant."}, {"role": "user", "content": "What is 2+2?"}], "ideal": "4"}
{"input": [{"role": "user", "content": "Capital of France?"}], "ideal": "Paris"}
```

### 5. Custom Evals (Python)

```python
from evals.api import DummyCompletionFn
from evals.elsuite import Eval
from evals.record import RecorderBase

class MyCustomEval(Eval):
    def __init__(self, samples_jsonl, *args, **kwargs):
        super().__init__(*args, **kwargs)
        self.samples = self.load_samples(samples_jsonl)

    def eval_sample(self, sample, rng):
        # Get model completion
        prompt = sample["input"]
        result = self.completion_fn(prompt)

        # Judge correctness
        expected = sample["ideal"]
        correct = expected in result.get_completions()[0]

        # Record results
        return evals.record.MatchResult(
            correct=correct,
            expected=expected,
            picked=result.get_completions()[0],
        )

    def run(self, recorder: RecorderBase):
        self.eval_all_samples(recorder)
        return {
            "accuracy": sum(r.correct for r in recorder.results) / len(recorder.results)
        }
```

### 6. Completion Functions

```python
# Use different models
oaieval gpt-4o mmlu                    # OpenAI
oaieval gpt-3.5-turbo mmlu             # GPT-3.5
oaieval text-davinci-003 mmlu          # Completion model

# Dummy for testing eval logic
from evals.api import DummyCompletionFn
completion_fn = DummyCompletionFn()

# Custom completion function
from evals.api import CompletionFn
class MyCompletionFn(CompletionFn):
    def __call__(self, prompt, **kwargs):
        # Call your model, API, or pipeline
        return CompletionResult(completions=["response"])
```

### 7. Model-Graded Eval Types

```yaml
# Fact-checking
model_graded_spec: fact

# Closed-ended QA
model_graded_spec: closedqa

# Chain-of-thought classification
eval_type: cot_classify

# Direct classification
eval_type: classify
```

### 8. Logging and Analysis

```bash
# Log to Snowflake
export SNOWFLAKE_ACCOUNT=...
export SNOWFLAKE_DATABASE=...
oaieval gpt-4o mmlu

# Local JSON log
oaieval gpt-4o mmlu --record_path results.jsonl
cat results.jsonl | python analyze.py

# W&B integration
oaieval gpt-4o mmlu --wandb_project my-project
```

### 9. Available Eval Registry (Selected)

| Eval | Type | Domain |
|------|------|--------|
| `mmlu` | Match | Knowledge (57 subjects) |
| `hellaswag` | Match | Commonsense reasoning |
| `truthfulqa` | Model-graded | Truthfulness |
| `gsm8k` | Match | Math reasoning |
| `humaneval` | Custom | Code generation |
| `ifeval` | Model-graded | Instruction following |
| `bbq` | Model-graded | Bias detection |
| `factuality` | Model-graded | Factual accuracy |
| `translation` | Model-graded | Translation quality |

Browse full registry: `evals/registry/evals/`

### 10. Production Eval Pipeline Pattern

```python
# CI/CD eval pipeline
def run_eval_suite(model_name, eval_names):
    results = {}
    for eval_name in eval_names:
        cmd = f"oaieval {model_name} {eval_name} --max_samples 200"
        result = subprocess.run(cmd, shell=True, capture_output=True)
        results[eval_name] = parse_accuracy(result.stdout)
    return results

# Regression test
previous = {"mmlu": 0.86, "gsm8k": 0.92, "hellaswag": 0.85}
current = run_eval_suite("my-finetuned-model", ["mmlu", "gsm8k", "hellaswag"])

for name, score in current.items():
    if score < previous[name] - 0.02:  # 2% regression threshold
        alert(f"Regression in {name}: {previous[name]:.2f} → {score:.2f}")
```

## Key Patterns

1. **Start with templates** — most evals don't need custom Python code
2. **Use model-graded evals** for subjective quality (fluency, helpfulness, safety)
3. **Use match evals** for objective metrics (classification, multiple choice)
4. **`git lfs fetch --all`** is required before running community evals
5. **Custom completion functions** enable testing non-OpenAI models
6. **Record paths** enable debugging individual failures
7. **Private evals** can test proprietary data without exposing it

## References

- [OpenAI Evals Repo](https://github.com/openai/evals)
- [Build an Eval Guide](https://github.com/openai/evals/blob/main/docs/build-eval.md)
- [Run Evals Guide](https://github.com/openai/evals/blob/main/docs/run-evals.md)
- [Eval Templates Reference](https://github.com/openai/evals/blob/main/docs/eval-templates.md)
- [Custom Eval Example](https://github.com/openai/evals/blob/main/docs/custom-eval.md)
- [Completion Functions Guide](https://github.com/openai/evals/blob/main/docs/completion-fns.md)
