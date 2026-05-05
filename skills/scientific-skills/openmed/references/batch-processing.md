---
name: openmed-batch-processing
description: "Process multiple texts or files with OpenMed. BatchProcessor for multi-document workflows with progress tracking and configurable output."
tags: [openmed, batch, processing, multi-document, workflow]
---

# Batch Processing

Process multiple clinical documents efficiently with OpenMed's BatchProcessor.

## Basic Batch

```python
from openmed import BatchProcessor

processor = BatchProcessor(
    model_name="disease_detection_superclinical",
    confidence_threshold=0.55,
    group_entities=True,
)

texts = [
    "Metastatic breast cancer treated with trastuzumab.",
    "Acute lymphoblastic leukemia diagnosed.",
    "Patient on metformin for type 2 diabetes.",
]

result = processor.process_texts(texts)
for doc in result:
    for entity in doc.entities:
        print(f"{entity.text}: {entity.confidence:.2f}")
```

## With Configuration

```python
from openmed import BatchProcessor, OpenMedConfig

config = OpenMedConfig.from_profile("prod")
processor = BatchProcessor(
    model_name="disease_detection_superclinical",
    config=config,
    group_entities=True,
)
result = processor.process_texts(texts)
```

## References
- https://openmed.life/docs/batch-processing
