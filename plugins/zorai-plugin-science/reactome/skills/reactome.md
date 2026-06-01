---
name: reactome
description: >
  Query the Reactome database (Analysis and Content Services). Use when the
  user asks about pathway analysis, gene list enrichment, retrieving
  results by token, finding unmapped or not-found identifiers, mapping
  identifiers, reaction participants (inputs, outputs), pathway hierarchy
  (including top-level pathways), diagram export, cross-reference mapping,
  or searching the knowledgebase. Backed by the public Reactome REST APIs
  (ContentService + AnalysisService) — no auth.
---

# Reactome Plugin

Use the **reactome** plugin for pathway analysis and gene-list enrichment.
The plugin wraps the deepmind `reactome_analysis.py` script.

## Auth

None. Both Reactome REST services are anonymous-public. AnalysisService
has a soft rate limit (~30 req/s per IP); ContentService is more lenient.

## Common env vars

- `RE_OUTPUT` — **required** for every command (the script writes JSON
  to disk; stdout is a brief summary).
- `RE_TOKEN` — the opaque token returned by `/analyze`. Required for
  every `token-*` follow-up command. Treat as opaque — don't parse it.
- `RE_PATHWAY` — Reactome stable ID (e.g. `R-HSA-69278` for
  "Cell Cycle"). Used to scope token queries to a specific pathway
  hierarchy.
- `RE_FILE` / `RE_IDS` — input mode for `/analyze`. Either a file
  path (one ID per line) or a comma-separated list. **At least one of
  these is required** for `/analyze`.

## Common workflow: gene list → enriched pathways

```text
   ids.txt or RE_IDS="TP53,BRCA1,EGFR,..."
        │
        ▼
   /reactome.analyze              ← returns a token, stored in RE_TOKEN
        │
        ├─→ /reactome.token-result            ← full enrichment result
        ├─→ /reactome.token-found-all         ← which genes hit
        ├─→ /reactome.token-not-found        ← which genes missed (QC)
        └─→ /reactome.token-filter-species   ← re-filter to a species
```

## Commands

### `/reactome.db-name`

Sanity check. Returns "Reactome".

Required: `RE_OUTPUT`.

### `/reactome.db-version`

Returns the Reactome version number (e.g. `89`). Cite this in
publications.

Required: `RE_OUTPUT`.

### `/reactome.analyze`

Submit a gene list for pathway enrichment. Returns a token in
`RE_OUTPUT`'s JSON; pass it to the `token-*` follow-up commands.

Required: `RE_FILE` (path to a one-id-per-line text file) **or**
`RE_IDS` (comma-separated list of IDs). Always `RE_OUTPUT`.
Optional: `RE_PROJECTION` (e.g. `species`, `pathways`), `RE_INTERACTORS`
(include protein-protein interactors).

Example:

```bash
RE_IDS=TP53,BRCA1,EGFR,KRAS,MYC,PTEN \
RE_OUTPUT=./reactome_analysis.json \
zorai plugin invoke reactome analyze
# Then grab the token from the output JSON's "summary" key and use it:
RE_TOKEN=$(jq -r .summary.token < reactome_analysis.json)
```

### `/reactome.token-result`

Full enrichment result for an analysis. The full nested pathway
hierarchy with p-values, found/total counts per pathway, etc.

Required: `RE_TOKEN`, `RE_OUTPUT`. Optional: `RE_PATHWAY` (scope to a
specific pathway hierarchy).

### `/reactome.token-found-all`

The identifiers that mapped to Reactome, with which pathway they hit
and how many interactions. Useful for verifying input quality.

Required: `RE_TOKEN`, `RE_OUTPUT`. Optional: `RE_PATHWAY`, `RE_RESOURCE`.

### `/reactome.token-not-found`

Identifiers that did NOT map to any Reactome pathway. Use this to
catch typos, wrong species, or non-coding IDs.

Required: `RE_TOKEN`, `RE_OUTPUT`.

### `/reactome.token-filter-species`

Re-filter an existing analysis to a single species. Common case:
the user submitted a list with mixed species, and wants only
Homo sapiens hits.

Required: `RE_TOKEN`, `RE_SPECIES` (NCBI taxon ID, e.g. `9606` for
Homo sapiens), `RE_OUTPUT`.

### `/reactome.identifier`

One-step lookup: analyze a single ID and return its pathway hits
directly. Skip the token dance when you only have one ID.

Required: `RE_ID`, `RE_OUTPUT`. Optional: `RE_RESOURCE`.

### `/reactome.search`

Full-text search across the Reactome knowledgebase (pathways,
reactions, proteins, complexes).

Required: `RE_QUERY`, `RE_OUTPUT`.

### `/reactome.top-pathways`

List the top-level Reactome super-pathways (the 20-ish first-level
branches in the pathway hierarchy: "Signal Transduction", "Disease",
"Metabolism", "Cell Cycle", etc.). Useful as a starting point when
the user is exploring a new biological domain.

Required: `RE_OUTPUT`.

### `/reactome.run`

Generic catch-all. Forwards an arbitrary `reactome_analysis.py`
subcommand. Use for the 44+ specialized endpoints not exposed as
named commands (analyze-projection, analyze-form, token-filter-pathways,
token-found-entities, token-found-interactors, download-result,
mapping, mapping-projection, import-json, report, species-comparison,
participants, participating-entities, component-of, event-ancestors,
contained-events, xref-mapping, diagram, reaction-diagram, etc.).

Required: `RE_SUBCOMMAND`, `RE_OUTPUT`. Other args pass through.

## Limits

- AnalysisService soft limit: 30 req/s. The script enforces backoff
  on 429s.
- A single `/analyze` is rate-limited to **1 GB of input per submission**
  (effectively unbounded for any reasonable gene list).
- Tokens expire after **7 days**. Save the token and result
  immediately; don't rely on re-querying weeks later.

## Cross-references

- **uniprot** plugin: submit a UniProt-derived gene list.
- **ensembl** plugin: resolve a gene symbol to ENSG, then submit the
  ENSG list to `/analyze`.
- **chembl** plugin: drug-target analysis — submit a list of
  drug-target ENSG IDs, see which pathways the drug hits.

## License

Plugin manifest + this skill file: MIT.
Vendored scripts: Apache 2.0.
Reactome data terms: <https://reactome.org/license>.
