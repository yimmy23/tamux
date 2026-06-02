---
name: gnomad
description: >
  Query the Genome Aggregation Database (gnomAD). Use when determining the
  rarity or allele frequency of specific genetic variants, retrieving gene
  constraint metrics (pLI, LOEUF) to assess loss-of-function intolerance,
  finding variants in a genomic region or gene, or querying structural
  variants. Don't use for analyzing individual patient genomes, tracking
  somatic mutations in cancer (use COSMIC), or requesting raw sequencing
  reads (use ENA). Backed by the public gnomAD GraphQL API
  (https://gnomad.broadinstitute.org/api) — no auth.
---

# gnomAD Plugin

Use the **gnomad** plugin for population allele frequencies, gene
constraint metrics, and variant search. The plugin wraps the deepmind
`get_gene_constraint.py` / `get_variant_frequency.py` / `search_variants.py`
scripts (3 separate scripts — the same shape as `alphafold-database`).

## Auth

None. gnomAD GraphQL is anonymous-public with a **strict 10 req/min
rate limit** (the scripts enforce this internally via `qps=0.1666`).

## Common env vars

- `GNMD_OUTPUT` — **required** for every command (output JSON file path).
- `GNMD_DATASET` — gnomAD dataset version. Default `gnomad_r4`. Other
  options: `gnomad_r3`, `gnomad_r2_1`, `gnomad_r2_1_controls`,
  `gnomad_r2_1_non_neuro`, `gnomad_r2_1_neuro`.

## Commands

### `/gnomad.get-gene-constraint`

Get gene-level loss-of-function / missense constraint metrics
(pLI, LOEUF, oe_lof, oe_mis_upper, oe_mis_lower, synonymous Z-scores).
**Required** for assessing whether a gene can tolerate loss of
function.

Required env: `GNMD_GENE` (e.g. `PCSK9`, `BRCA1`, `TP53`), `GNMD_OUTPUT`.

Example:

```bash
GNMD_GENE=PCSK9 GNMD_OUTPUT=./pcsk9_constraint.json \
/gnomad.get-gene-constraint
```

### `/gnomad.get-variant-frequency`

Get allele frequency, allele count, allele number, and population
breakdown for a single variant. Two input modes:
- `--variant_id` in `chrom-pos-ref-alt` format (e.g. `1-55516888-G-T`).
- `--rsid` (e.g. `rs121918506`).

Required env: `GNMD_OUTPUT`. One of `GNMD_VARIANT_ID` or `GNMD_RSID`.
Optional: `GNMD_DATASET`.

Example — a known PCSK9 loss-of-function variant:

```bash
GNMD_RSID=rs562556 GNMD_OUTPUT=./pcsk9_lof_freq.json \
/gnomad.get-variant-frequency
```

### `/gnomad.search-variants`

Search variants by **gene** (single gene) or by **region** (chrom +
start + end), optionally filtered by consequence. Useful for
"what rare pLoF variants exist in this gene?" type questions.

Required env: `GNMD_OUTPUT`. One of:
- `GNMD_GENE` (single-gene search)
- `GNMD_CHROM` + `GNMD_START` + `GNMD_END` (regional search)
- just `GNMD_CONSEQUENCE` (across the whole genome, use sparingly)

Optional: `GNMD_DATASET`.

Example — all pLoF variants in BRCA1:

```bash
GNMD_GENE=BRCA1 GNMD_CONSEQUENCE=pLoF \
GNMD_OUTPUT=./brca1_plof.json \
/gnomad.search-variants
```

## Limits

- **10 req/min per IP** (strict). The scripts enforce a delay; expect
  multi-variant scans to take 6+ seconds per call.
- `search-variants` with no positional filter can return GB of data.
  Always provide `--gene` or a region.

## Cross-references

- **clinvar** plugin: take a pathogenic ClinVar variant, then call
  `get-variant-frequency` to confirm it's rare or common in the
  general population. Common + pathogenic = benign call.
- **ensembl** plugin: resolve a gene symbol to ENSG, then search
  gnomAD for that gene's variants. The combined ENSG + gnomAD
  frequency is the most-cited figure in modern clinical genetics.

## License

Plugin manifest + this skill file: MIT.
Vendored scripts: Apache 2.0.
gnomAD data terms: <https://gnomad.broadinstitute.org/about>.
