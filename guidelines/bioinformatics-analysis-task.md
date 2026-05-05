---
name: bioinformatics-analysis-task
description: "Use when analyzing genomic, transcriptomic, or multi-omics data: RNA-seq, single-cell RNA-seq, variant calling, or pathway analysis."
recommended_skills:
  - scanpy
  - biopython
  - scvelo
  - scvi-tools
  - geniml
  - phylogenetics
  - polars-bio
recommended_guidelines:
  - scientific-data-analysis-task
  - clinical-research-task
  - evidence-quality-task
---

## Overview

Bioinformatics analysis requires reproducible pipelines, statistical rigor, and biological validation of computational findings.

## Workflow

1. Define the biological question and appropriate data type (bulk RNA-seq, scRNA-seq, ChIP-seq, WGS).
2. Assess data quality: read quality scores, mapping rates, batch effects, doublet detection for single-cell.
3. Use `scanpy` for single-cell RNA-seq analysis: preprocessing, normalization, clustering, marker identification.
4. Use `biopython` for sequence manipulation, BLAST searches, and GenBank/PDB access.
5. Use `scvelo` for RNA velocity analysis and `scvi-tools` for deep probabilistic modeling.
6. Apply multiple testing correction (BH-FDR, Bonferroni) to all statistical tests.
7. Validate findings: replicate in independent dataset, experimental validation, or literature support.

## Quality Gate

Bioinformatics analysis is complete when preprocessing parameters are documented, statistical tests are corrected for multiplicity, and findings are validated or caveated.