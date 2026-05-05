---
name: openmed-multilingual-pii
description: "OpenMed multilingual PII extraction supporting 9 languages: EN, FR, DE, IT, ES, PT, NL, HI, TE. 210+ models in the PII catalog."
tags: [openmed, multilingual, pii, i18n, french, german, spanish, portuguese]
---

# Multilingual PII

OpenMed supports PII extraction and de-identification in 9 languages with language-specialized models.

## Supported Languages

| Language | Code | Models |
|---|---|---|
| English | en | Full family (35 models) |
| French | fr | Full family (35 models) |
| German | de | Full family (35 models) |
| Italian | it | Full family (35 models) |
| Spanish | es | Full family (35 models) |
| Portuguese | pt | 31 public API-visible models |
| Dutch | nl | 1 flagship model |
| Hindi | hi | 1 flagship model |
| Telugu | te | 1 flagship model |

## Examples

```python
from openmed import extract_pii

# Portuguese
result = extract_pii(
    "Paciente: Pedro Almeida, CPF: 123.456.789-09",
    lang="pt",
    model_name="OpenMed/OpenMed-PII-Portuguese-SnowflakeMed-Large-568M-v1",
)

# Dutch
result = extract_pii(
    "Patiënt: Eva de Vries, geboortedatum: 15 januari 1984, BSN: 123456782",
    lang="nl",
    model_name="OpenMed/OpenMed-PII-Dutch-SuperClinical-Large-434M-v1",
)

# Hindi
result = extract_pii(
    "रोगी: अनीता शर्मा, जन्मतिथि: 15 जनवरी 1984",
    lang="hi",
    model_name="OpenMed/OpenMed-PII-Hindi-SuperClinical-Large-434M-v1",
)

# Telugu
result = extract_pii(
    "రోగి: సితా రెడ్డి, జన్మ తేదీ: 15 జనవరి 1984",
    lang="te",
    model_name="OpenMed/OpenMed-PII-Telugu-SuperClinical-Large-434M-v1",
)
```

## Getting Default Models by Language

```python
from openmed.core.pii_i18n import SUPPORTED_LANGUAGES, get_patterns_for_language

print(SUPPORTED_LANGUAGES)
patterns = get_patterns_for_language("fr")  # French PII patterns
```

## References
- docs/pii-i18n (in-repo)
- https://openmed.life/docs/pii
