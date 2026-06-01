---
name: ensembl
description: >
  Query the Ensembl database to resolve gene, transcript, and protein IDs,
  fetch genomic or protein sequences, retrieve gene structures (exons), and
  get variant consequence and effect predictions (VEP). Use this skill as a
  primary ID translator, genomic sequence database, and variant effect
  prediction tool. Backed by the public Ensembl REST API
  (https://rest.ensembl.org/) — no auth.
---

# Ensembl Plugin

Use the **ensembl** plugin for ID translation, sequence retrieval, and
variant effect prediction. The plugin wraps the deepmind
`ensembl_api.py` script.

## Auth

None. Ensembl REST is anonymous-public with a soft rate limit
(~15 req/s per IP). The script's `http_client` enforces backoff on 429s.

## Common env vars

Most commands accept:

- `ENS_OUTPUT` — output file path. Optional for most; required for some
  large-sequence returns.
- `ENS_ASSEMBLY` — genome assembly (`GRCh38` default, `GRCh37` for
  legacy references).

ID-flavored commands take a single ID via `ENS_ENSG_ID` / `ENS_ENST_ID` /
`ENS_ID` / `ENS_QUERY` depending on the subcommand.

## Commands

### `/ensembl.resolve-gene`

Resolve a gene symbol / alias / RefSeq ID to one or more Ensembl gene
IDs (ENSG). Useful for normalizing input from papers / ClinVar into
canonical Ensembl IDs.

Required env: `ENS_QUERY`. Optional: `ENS_OUTPUT`, `ENS_ASSEMBLY`.

Example:

```bash
ENS_QUERY=TP53 ENS_OUTPUT=./tp53_ensembl.json \
zorai plugin invoke ensembl resolve-gene
```

### `/ensembl.map-id`

Cross-reference an Ensembl ID (ENSG/ENST/ENSP) to external databases
(RefSeq, UniProt, HGNC, NCBI, etc.). The primary tool for ID translation
across databases.

Required env: `ENS_ID`. Optional: `ENS_OUTPUT`, `ENS_ASSEMBLY`.

Example:

```bash
ENS_ID=ENSG00000141510 ENS_OUTPUT=./tp53_xrefs.json \
zorai plugin invoke ensembl map-id
```

### `/ensembl.get-sequence`

Fetch raw genomic DNA for a coordinate window. Two input modes:
- `--region chr1:1000-2000`
- `--chrom chr1 --start 1000 --end 2000`

Required env: `ENS_REGION` OR (`ENS_CHROM` + `ENS_START` + `ENS_END`).
Optional: `ENS_OUTPUT`, `ENS_ASSEMBLY`.

### `/ensembl.gene-summary`

High-level metadata for a gene: symbol, biotype, location, description.

Required env: `ENS_ENSG_ID` (e.g. `ENSG00000141510`).
Optional: `ENS_OUTPUT`, `ENS_ASSEMBLY`.

### `/ensembl.transcripts`

List transcripts for a gene, with optional MANE/canonical filter.

Required env: `ENS_ENSG_ID`. Optional: `ENS_OUTPUT`, `ENS_ASSEMBLY`.

### `/ensembl.canonical-tss`

TSS coordinate for the canonical transcript of a gene.

Required env: `ENS_GENE` (symbol or ENSG ID).
Optional: `ENS_OUTPUT`, `ENS_ASSEMBLY`.

### `/ensembl.transcript-structure`

Exon, CDS, and UTR layout for a transcript.

Required env: `ENS_ENST_ID` (e.g. `ENST00000269305`).
Optional: `ENS_OUTPUT`, `ENS_ASSEMBLY`.

### `/ensembl.protein-info`

ENSP ID and sequence length for a transcript.

Required env: `ENS_ENST_ID`. Optional: `ENS_OUTPUT`, `ENS_ASSEMBLY`.

### `/ensembl.protein-sequence`

Amino acid sequence (FASTA) for a transcript or protein ID.

Required env: `ENS_ID` (ENST or ENSP).
Optional: `ENS_OUTPUT`, `ENS_ASSEMBLY`.

### `/ensembl.vep`

Variant Effect Predictor. Annotate a single variant with consequences
on overlapping transcripts, regulatory features, and known variants.
**The VEP endpoint is one of the slowest on Ensembl** (~3-5s per call
on cold path); batch VEP is not exposed here.

Required env: `ENS_VARIANT` (e.g. `chr1 230710048 A G`) OR `ENS_REGION`
(for a region's variants). Optional: `ENS_OUTPUT`, `ENS_ASSEMBLY`.

## Limits

- **15 req/s per IP** for anonymous Ensembl REST. The script enforces
  backoff on 429 responses.
- VEP is rate-limited to **3 calls per second** per the Ensembl docs.
- `protein-sequence` returns can be large; prefer `ENS_OUTPUT` over
  capturing stdout for full-length proteins.

## Cross-references

- **uniprot** plugin: `map-id` returns the UniProt accession, which
  the uniprot plugin can then look up in detail.
- **clinvar** plugin: resolve a ClinVar variant's gene with
  `resolve-gene` before fetching the variant's transcript-level
  consequences with `vep`.
- **openalex** plugin: a typical workflow resolves a paper's gene
  symbol via `resolve-gene`, then fetches the paper list and gene
  metadata in one chain.

## License

Plugin manifest + this skill file: MIT.
Vendored scripts: Apache 2.0.
Ensembl data terms: <https://www.ensembl.org/info/about/legal/index.html>.
