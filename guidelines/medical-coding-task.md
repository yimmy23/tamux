---
name: medical-coding-task
description: "Use when working with medical codes: ICD-10-CM/PCS, CPT, HCPCS, SNOMED CT, LOINC, RxNorm. Covers code validation, HCC risk adjustment, and reimbursement modeling."
recommended_skills:
  - medical-coding
recommended_guidelines:
  - clinical-research-task
  - ehr-integration-task
  - clinical-nlp-task
---

## Overview

Medical coding is the backbone of healthcare billing, reimbursement, and clinical research. This guideline ensures accurate code assignment, HCC risk adjustment, and cross-terminology mapping.

## Workflow

1. Identify the code set: ICD-10-CM (diagnosis), ICD-10-PCS (procedure), CPT (services), HCPCS (supplies), LOINC (labs), RxNorm (medications).
2. Validate codes against the official code set for the active year — codes change annually.
3. Cross-map between terminologies: ICD-10 to SNOMED for clinical queries, ICD-10 to HCC for risk adjustment.
4. For risk adjustment: identify all documented diagnoses, map to HCC categories, calculate RAF score.
5. Document coding rationale — each code should map to a clinical finding in the record.
6. Check for code specificity: use the most specific code available (e.g., E11.41 vs E11.9).

## Quality Gate

Medical coding is complete when each code maps to a documented clinical finding, codes are validated against the current year's official set, and HCC mapping is documented.