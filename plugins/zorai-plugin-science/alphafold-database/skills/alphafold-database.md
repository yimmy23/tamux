---
name: alphafold-database
description: >
  Retrieve and analyze AlphaFold predicted structures for a protein. Use when
  the user provides a specific UniProt Accession ID and wants structural
  confidence metrics (pLDDT), domain boundary analysis, or disorder
  assessment. Do not use if the user only has a protein name, gene name,
  or amino acid sequence — ask for a UniProt ID first. Backed by the public
  AlphaFold Database REST API at https://alphafold.ebi.ac.uk/ (no auth).
---

# AlphaFold Database Plugin

Use the **alphafold-database** plugin to fetch and analyze AlphaFold
predicted structures. The plugin wraps the deepmind
`alphafold_database_fetch_and_analyze` Python scripts vendored in this
plugin's `scripts/` directory.

## Auth

None. The AlphaFold DB is anonymous-public at 1 req/s (the script enforces
its own rate limit).

## Inputs

A **valid UniProt accession** is required (e.g. `P04637` for human p53,
`A0A1B0GX81` for a fragment). Do **not** pass gene names, protein names,
or amino-acid sequences — the upstream API only resolves accessions.

## Workflow

```
fetch-structure (downloads mmCIF + PAE JSON to a directory)
   ├── analyze-plddt <metadata JSON>   (confidence metrics)
   └── analyze-pae   <PAE JSON>       (sub-domain boundaries)
```

## Commands

### `/alphafold-database.fetch-structure`

Download AlphaFold predicted structure (mmCIF + PAE JSON) for a UniProt ID.

Required env: `AF_UNIPROT_ID`. Optional: `AF_OUTPUT_DIR` (default `./af_output`).

Example:

```bash
AF_UNIPROT_ID=P04637 AF_OUTPUT_DIR=./p53 \
/alphafold-database.fetch-structure
```

Outputs (inside `AF_OUTPUT_DIR`):

- `<ID>.cif` — the predicted structure in mmCIF format
- `<ID>-pae.json` — the PAE matrix JSON (used by `analyze-pae`)
- `<ID>-metadata.json` — the AlphaFold DB entry (used by `analyze-plddt`)

### `/alphafold-database.analyze-plddt`

Analyze pLDDT confidence metrics from a previously-fetched metadata JSON.

Required env: `AF_METADATA_FILE` (path to the `<ID>-metadata.json` produced
by `fetch-structure`).

### `/alphafold-database.analyze-pae`

Analyze Predicted Aligned Error and detect sub-domain boundaries from a PAE
JSON.

Required env: `AF_PAE_FILE` (path to the `<ID>-pae.json` produced by
`fetch-structure`).
Optional: `AF_PAE_DISTANCE_CUTOFF` (default 7.0), `AF_PAE_MIN_DOMAIN_SIZE`
(default 40), `AF_PAE_OUTPUT_FILE`.

## Error patterns

- **404 from `fetch-structure`** — the accession is not in AFDB (typo, or
  no prediction exists for that protein). Surface the error verbatim; do
  not retry.
- **HTTP error from `fetch-structure`** — the upstream AFDB is rate-limited
  at 1 req/s. The script enforces this; if you see sporadic failures, just
  retry the same command.

## Limits & constraints

- This is the **public** AlphaFold Database. The deepmind `SKILL.md` does
  not currently wrap the EBI-hosted API key for higher limits (none
  exists for AFDB).
- The `analyze-plddt` thresholds are encoded in the script
  (CONFIDENT ≥ 0.7, MODERATE ≥ 0.4, etc.). Override only if you understand
  the pLDDT scale.

## License

Plugin manifest + this skill file: MIT.
Vendored scripts: Apache 2.0.
AlphaFold DB terms: <https://alphafold.ebi.ac.uk/>.
