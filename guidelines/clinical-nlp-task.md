---
name: clinical-nlp-task
description: "Use when extracting structured information from clinical text: notes, reports, discharge summaries. Covers entity extraction, de-identification, assertion detection, and medical coding."
recommended_skills:
  - openmed
  - medcat
  - clinical-decision-support
  - clinical-reports
recommended_guidelines:
  - clinical-research-task
  - medical-imaging-task
---

## Overview

Clinical NLP extracts structured data from free-text clinical narratives. This guideline ensures extraction is accurate, privacy-preserving, and clinically useful.

## Workflow

1. Identify the text source: clinical notes, radiology reports, discharge summaries, lab results.
2. Determine the extraction task: disease/disorder detection, medication extraction, procedure coding, assertion classification (negation, temporality, certainty).
3. Use `openmed` for entity extraction with curated medical NER models (disease, drug, anatomy, procedure).
4. Use `medcat` for concept annotation against UMLS, ICD-10, SNOMED CT, or RxNorm ontologies.
5. Apply de-identification using HIPAA-compliant privacy filters before storing or sharing.
6. Validate on a held-out set: precision, recall, F1 against a gold standard.
7. Review false positives and false negatives — clinical NLP errors have patient safety implications.

## Quality Gate

Clinical NLP is complete when accuracy is measured against a gold standard, PHI is removed, and model limitations are documented.