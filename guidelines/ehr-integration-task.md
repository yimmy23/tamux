---
name: ehr-integration-task
description: "Use when working with electronic health record data: FHIR APIs, OMOP CDM, MIMIC database, or healthcare interoperability."
recommended_skills:
  - fhir
  - omop-ohdsi
  - mimic
  - clinical-trials
recommended_guidelines:
  - clinical-research-task
  - data-analysis-task
  - scientific-database-lookup-task
---

## Overview

EHR data integration requires understanding healthcare data models, terminology mappings, and privacy regulations before querying or transforming clinical data.

## Workflow

1. Identify the data source: live FHIR server, OMOP CDM database, MIMIC flat files, or custom EHR export.
2. Use `fhir` for FHIR REST API queries (Patient, Observation, Condition, MedicationRequest).
3. Use `omop-ohdsi` for OMOP Common Data Model — run ACHILLES for data quality, define cohorts, extract features.
4. Use `mimic` for MIMIC-III/IV ICU data — requires PostgreSQL and credentialed access.
5. Map terminologies: ICD-10, SNOMED CT, LOINC, RxNorm. Validate codes against official code sets.
6. Handle PHI: de-identify before exporting, limit queries to authorized data scopes.

## Quality Gate

EHR data integration is complete when the data model is understood, queries are validated, and PHI is protected.