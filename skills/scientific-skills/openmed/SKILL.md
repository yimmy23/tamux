---
name: openmed
description: "Production-ready medical NLP toolkit (maziyarpanahi/openmed). Entity extraction, assertion detection, medical reasoning, de-identification, and structured insight generation from clinical text. Zero-shot disease/disorder coding, ICD-10-CM mapping."
tags: [medical-nlp, clinical-text, entity-extraction, deidentification, icd10, healthcare, zorai]
---
## Overview

OpenMed is a production-ready medical NLP toolkit focused on extracting structured clinical information from free text. It supports entity extraction, assertion handling, de-identification, and model-specific workflows through a simple Python API, batch processing, and a REST service.

## Installation

From a local checkout of the repo:

```bash
uv pip install -e ".[hf]"
# add service dependencies if needed
uv pip install -e ".[hf,service]"
```

Apple Silicon / MLX path:

```bash
uv pip install -e ".[mlx]"
```

## Single-text analysis

```python
from openmed import analyze_text

result = analyze_text(
    "Patient started on imatinib for chronic myeloid leukemia.",
    model_name="disease_detection_superclinical",
)

for entity in result.entities:
    print(entity.label, entity.text, round(entity.confidence, 3))
```

Expected use cases include:
- disease / diagnosis extraction
- drug extraction
- anatomy / procedure extraction
- assertion-aware clinical interpretation

## Batch processing

```python
from openmed import BatchProcessor

processor = BatchProcessor(
    model_name="disease_detection_superclinical",
    confidence_threshold=0.55,
    group_entities=True,
)

result = processor.process_texts([
    "Patient started metformin for type 2 diabetes.",
    "Imatinib started for chronic myeloid leukemia.",
])

for doc in result:
    print(doc.entities)
```

## REST API service

```bash
uvicorn openmed.service.app:app --host 0.0.0.0 --port 8080
```

Then call it from your app backend instead of embedding model logic everywhere.

## De-identification / privacy workflow

OpenMed also exposes privacy-oriented workflows. Use those when clinical text may contain PHI/PII and you need redaction before storage, review, or downstream model calls.

## Practical workflow

1. Pick the right model family for the extraction task.
2. Start with `analyze_text()` on a handful of examples.
3. Inspect confidence and false positives before scaling.
4. Switch to `BatchProcessor` for document sets.
5. Use the REST service when integrating into larger systems.
6. Add privacy filtering before persistence or external delivery.

## Common failure modes

- using the wrong `model_name` for the task
- treating confidence as a calibrated probability without validation
- skipping de-identification in clinical workflows
- batch-processing without checking entity grouping/threshold behavior first
