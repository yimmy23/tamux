---
name: drug-discovery-task
description: "Use when working with molecular data: drug discovery pipelines, molecular docking, Cheminformatics analysis, ADME prediction, or compound screening."
recommended_skills:
  - rdkit
  - deepchem
  - datamol
  - molecular-dynamics
  - diffdock
  - torchdrug
recommended_guidelines:
  - scientific-data-analysis-task
  - clinical-research-task
---

## Overview

Drug discovery and cheminformatics combines molecular modeling, machine learning, and experimental data to identify and optimize drug candidates.

## Workflow

1. Define the target: protein, pathway, or phenotype. Identify existing assays and known modulators.
2. Use `rdkit` for molecular representation, fingerprinting, and similarity searching.
3. Use `deepchem` for ML-based property prediction and molecular featurization.
4. Use `datamol` for molecular standardization and curation — normalize charges, sanitize, remove salts.
5. For virtual screening: compute molecular descriptors, train or load a predictive model, rank candidates.
6. For molecular docking: use `diffdock` for diffusion-based docking, analyze binding poses.
7. Document assay conditions, model training data, and prediction confidence.

## Quality Gate

Drug discovery analysis is complete when molecular data is curated, predictions are validated, and assumptions are documented.