---
name: scvelo
description: RNA velocity analysis with scVelo. Estimate cell state transitions from unspliced/spliced mRNA dynamics, infer trajectory directions, compute latent time, and identify driver genes in single-cell RNA-seq data. Complements Scanpy/scVI-tools for trajectory inference.
license: BSD-3-Clause
tags: [scientific-skills, scvelo, scanpy, bioinformatics]
metadata:
    skill-author: Kuan-lin Huang
-------|-----|-------------|
| `adata.layers` | `velocity` | RNA velocity per gene per cell |
| `adata.layers` | `fit_t` | Fitted latent time per gene per cell |
| `adata.obsm` | `velocity_umap` | 2D velocity vectors on UMAP |
| `adata.obs` | `velocity_pseudotime` | Pseudotime from velocity |
| `adata.obs` | `latent_time` | Latent time from dynamical model |
| `adata.obs` | `velocity_length` | Speed of each cell |
| `adata.obs` | `velocity_confidence` | Confidence score per cell |
| `adata.var` | `fit_likelihood` | Gene-level model fit quality |
| `adata.var` | `fit_alpha` | Transcription rate |
| `adata.var` | `fit_beta` | Splicing rate |
| `adata.var` | `fit_gamma` | Degradation rate |
| `adata.uns` | `velocity_graph` | Cell-cell transition probability matrix |

## Velocity Models Comparison

| Model | Speed | Accuracy | When to Use |
|-------|-------|----------|-------------|
| `stochastic` | Fast | Moderate | Exploratory; large datasets |
| `deterministic` | Medium | Moderate | Simple linear kinetics |
| `dynamical` | Slow | High | Publication-quality; identifies driver genes |

## Best Practices

- **Start with stochastic mode** for exploration; switch to dynamical for final analysis
- **Need good coverage of unspliced reads**: Short reads (< 100 bp) may miss intron coverage
- **Minimum 2,000 cells**: RNA velocity is noisy with fewer cells
- **Velocity should be coherent**: Arrows should follow known biology; randomness indicates issues
- **k-NN bandwidth matters**: Too few neighbors → noisy velocity; too many → oversmoothed
- **Sanity check**: Root cells (progenitors) should have high unspliced/spliced ratios for marker genes
- **Dynamical model requires distinct kinetic states**: Works best for clear differentiation processes

## Troubleshooting

| Problem | Solution |
|---------|---------|
| Missing unspliced layer | Re-run velocyto or use STARsolo with `--soloFeatures Gene Velocyto` |
| Very few velocity genes | Lower `min_shared_counts`; check sequencing depth |
| Random-looking arrows | Try different `n_neighbors` or velocity model |
| Memory error with dynamical | Set `n_jobs=1`; reduce `n_top_genes` |
| Negative velocity everywhere | Check that spliced/unspliced layers are not swapped |

## Additional Resources

- **scVelo documentation**: https://scvelo.readthedocs.io/
- **Tutorial notebooks**: https://scvelo.readthedocs.io/tutorials/
- **GitHub**: https://github.com/theislab/scvelo
- **Paper**: Bergen V et al. (2020) Nature Biotechnology. PMID: 32747759
- **velocyto** (preprocessing): http://velocyto.org/
- **CellRank** (fate prediction, extends scVelo): https://cellrank.readthedocs.io/
- **dynamo** (metabolic labeling alternative): https://dynamo-release.readthedocs.io/
