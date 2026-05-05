---
name: openmed-entity-extraction
description: "Extract medical entities from clinical text: diseases, drugs, anatomy, genes, and procedures. Use with analyze_text() and model selection."
tags: [openmed, ner, entity-extraction, disease-detection, pharma, clinical-nlp]
---

# Entity Extraction

OpenMed provides pre-trained NER models for medical entity extraction from clinical text.

## Basic Extraction

```python
from openmed import analyze_text

text = "Patient with metastatic breast cancer treated with trastuzumab and paclitaxel."
result = analyze_text(text, model_name="disease_detection_superclinical")

for entity in result.entities:
    print(f"{entity.label:<10} {entity.text:<35} {entity.confidence:.2f}")
```

## Available Models

| Model Key | Entities | Best For |
|---|---|---|
| `disease_detection_superclinical` | DISEASE, CONDITION, DIAGNOSIS | General clinical notes |
| `pharma_detection_superclinical` | DRUG, MEDICATION, TREATMENT | Medication lists, prescriptions |
| `anatomy_detection_electramed` | ANATOMY, ORGAN, BODY_PART | Surgical reports, imaging |
| `gene_detection_genecorpus` | GENE, PROTEIN | Genomics, molecular reports |

## Advanced Options

```python
# Confidence threshold
result = analyze_text(text, model_name="disease_detection_superclinical", confidence_threshold=0.55)

# Group adjacent entities of the same label
result = analyze_text(text, model_name="disease_detection_superclinical", group_entities=True)

# Change output format
result = analyze_text(text, model_name="disease_detection_superclinical", output_format="json")

# Model suggestions by text content
from openmed.core.model_registry import get_model_suggestions
suggestions = get_model_suggestions("Metastatic breast cancer on paclitaxel.")
for key, info, reason in suggestions:
    print(key, info.display_name, reason)
```

## Output Formats

- `dict` (default) — structured Python dict
- `json` — JSON string
- `html` — highlighted HTML
- `csv` — CSV rows

## References
- docs: https://openmed.life/docs/analyze-text
- model registry: https://openmed.life/docs/model-registry
