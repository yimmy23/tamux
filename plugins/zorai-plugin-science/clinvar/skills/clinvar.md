---
name: clinvar
description: >
  Use when needing clinical significance, pathogenicity classifications (e.g.
  Pathogenic, Benign, VUS), clinical evidence rationales, or finding "hard
  positive" benchmark controls for human genomic variants. Backed by NCBI
  ClinVar via E-utilities (https://www.ncbi.nlm.nih.gov/clinvar/). Optional
  NCBI API key raises the rate limit from ~3 req/s to ~10 req/s.
---

# ClinVar Plugin

Use the **clinvar** plugin for clinical-significance lookups and
pathogenicity classifications. The plugin wraps the deepmind
`clinvar_api.py` script.

## Auth (optional)

`NCBI_API_KEY` setting is **optional**. Without it, NCBI E-utilities limits
to ~3 req/s. With one, the limit rises to ~10 req/s. Get a key at
<https://www.ncbi.nlm.nih.gov/account/settings/>.

The script loads the key from `~/.env` via `dotenv` (it picks up
`NCBI_API_KEY` automatically). The plugin **never surfaces the key in
the agent context**.

## Query syntax

ClinVar queries use the NCBI E-utilities query syntax:

- Gene: `BRCA1[gene]`
- Clinical significance: `clinsig_pathogenic`, `clinsig_benign`,
  `clinsig_conflicting`, `clinsig_uncertain`
- Combine: `BRCA1[gene] AND clinsig_pathogenic`
- Coordinates: `NC_000017.11[chr] AND 43094000:43125000[chrpos]`

## Commands

### `/clinvar.count`

Get the total number of variants matching a query (no ID fetch).

Required env: `CV_QUERY`.

Example:

```bash
CV_QUERY="BRCA1[gene] AND clinsig_pathogenic" \
/clinvar.count
```

### `/clinvar.search`

Search for variants. Returns variant IDs.

Required env: `CV_QUERY`. Optional: `CV_RETMAX` (default 20).

### `/clinvar.summary`

Get clinical significance, star rating, and phenotypes for one or more
variant IDs (space-separated).

Required env: `CV_IDS`.

Example:

```bash
CV_IDS="12345 67890 11111" \
/clinvar.summary
```

### `/clinvar.evidence`

Fetch the full clinical evidence record for a single variant ID — review
status, assertions, citations, conditions.

Required env: `CV_VARIANT_ID`.

## Workflow

The typical hard-positive benchmark workflow:

1. `count` — know how many pathogenic variants exist for a gene.
2. `search` — get a batch of variant IDs.
3. `summary` — pull significance + star rating.
4. `evidence` — drill into a single variant for the full review record.

## Rate limits

- **No key** — 3 req/s. The script's `http_client` enforces a soft delay.
  For large pulls, set `NCBI_API_KEY`.
- **With key** — 10 req/s.
- For >10k variants, the upstream recommends `epost`/`efetch` batching,
  which this script does not wrap. Fall back to `ncbi_sequence_fetch`
  in the bundle, or write a one-off script with Biopython's Entrez.

## License

Plugin manifest + this skill file: MIT.
Vendored scripts: Apache 2.0.
ClinVar data terms: <https://www.ncbi.nlm.nih.gov/clinvar/>.
