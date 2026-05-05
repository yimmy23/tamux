---
name: openmed-pii-deidentification
description: "HIPAA-compliant PII detection and de-identification with smart entity merging, multiple redaction methods, and Faker-backed obfuscation."
tags: [openmed, pii, deidentification, hipaa, privacy, anonymization]
---

# PII Detection & De-identification

OpenMed provides HIPAA-compliant PII detection (all 18 Safe Harbor identifiers) with multiple de-identification methods.

## Extract PII

```python
from openmed import extract_pii

result = extract_pii(
    "Patient: John Doe, DOB: 01/15/1970, SSN: 123-45-6789",
    model_name="pii_detection_superclinical",
    use_smart_merging=True,
)
for entity in result.entities:
    print(f"{entity.label:<8} {entity.text:<25} {entity.confidence:.2f}")
```

## De-identify

```python
from openmed import deidentify

text = "Patient John Doe (DOB: 01/15/1970) at MRN 4471882."

# Mask — replace with label tags
masked = deidentify(text, method="mask")
# "[NAME], DOB: [DATE] at MRN [ID]"

# Remove — complete removal
removed = deidentify(text, method="remove")

# Replace — Faker-backed locale-aware fakes
replaced = deidentify(text, method="replace", lang="pt", locale="pt_BR",
                      consistent=True, seed=42)

# Hash — cryptographic hashing
hashed = deidentify(text, method="hash")

# Shift dates — offset by N days
shifted = deidentify(text, method="shift_dates", date_shift_days=180)
```

## Smart Entity Merging

Prevents tokenization fragmentation of dates, SSNs, and multi-word entities:

```python
result = extract_pii(text, use_smart_merging=True)
# Instead of "01" + "/15/1970", produces "01/15/1970"
```

## References
- https://openmed.life/docs/pii
- docs/anonymization.md
- docs/pii-smart-merging.md
