---
name: chembl
description: >
  Query the ChEMBL database for bioactive molecules, drug targets, bioactivity
  data, approved drugs, and chemical structures. Use when the user asks about
  compounds, targets, IC50 / Ki values, drug mechanisms, or structure searches.
  Backed by the public ChEMBL REST API
  (https://www.ebi.ac.uk/chembl/api/data/) — no auth.
---

# ChEMBL Plugin

Use the **chembl** plugin for bioactive-molecule, drug-target, and
bioactivity queries. The plugin wraps the deepmind `chembl_api.py` script,
which exposes **33 endpoint subcommands** + 4 utility subcommands. The
plugin maps the 9 most common endpoints to named zorai commands and
provides a `/chembl.run` catch-all for the rest.

## Auth

None. ChEMBL REST is anonymous-public.

## Common env vars

Most commands accept these:

- `CH_ID` — single-entity lookup
- `CH_SEARCH` — free-text search (only on `searchable` endpoints: activity, assay, chembl_id_lookup, document, molecule, protein_classification, target)
- `CH_SMILES` — SMILES string (for `similarity` / `substructure`)
- `CH_LIMIT` — max results (default varies by endpoint)
- `CH_OFFSET` — pagination offset
- `CH_FORMAT` — `json` (default) | `xml` | `yaml` | `tsv` | `csv`
- `CH_OUTPUT` — output file (for `status`, `image`)

Endpoint-specific:

- `CH_TARGET` / `CH_MOLECULE` — filter `activity` by target/molecule
- `CH_NORMALIZE` — `true` to convert bioactivity values to nM
- `CH_THRESHOLD` — Tanimoto cutoff for `similarity` (default 0.7)
- `CH_DL_FORMAT` — `sdf` | `mol` for `molecule` (requires `CH_ID`)

## Commands

### `/chembl.status`

Check ChEMBL API status and version.

Optional env: `CH_OUTPUT` (default `chembl_status.json`).

### `/chembl.molecule`

Query molecules. Search by SMILES / ChEMBL ID / InChIKey, or fetch a single
molecule by `--id`.

Optional: `CH_ID`, `CH_SMILES`, `CH_SEARCH`, `CH_LIMIT`, `CH_OFFSET`,
`CH_FORMAT`, `CH_DL_FORMAT`.

### `/chembl.target`

Query targets (proteins / nucleic acids / complexes with bioactivity data).

Optional: `CH_ID`, `CH_SEARCH`, `CH_LIMIT`, `CH_OFFSET`, `CH_FORMAT`.

### `/chembl.activity`

Query bioactivity records. Returns IC50 / Ki / etc. for (compound, target)
pairs. Use `CH_NORMALIZE=true` to convert all values to nM.

Optional: `CH_ID`, `CH_MOLECULE`, `CH_TARGET`, `CH_LIMIT`, `CH_OFFSET`,
`CH_FORMAT`, `CH_NORMALIZE`.

### `/chembl.assay`

Query experimental assays.

Optional: `CH_ID`, `CH_SEARCH`, `CH_LIMIT`, `CH_FORMAT`.

### `/chembl.drug`

Query approved drugs (mechanism, indication, trade names).

Optional: `CH_ID`, `CH_SEARCH`, `CH_LIMIT`, `CH_FORMAT`.

### `/chembl.mechanism`

Query drug mechanism-of-action records (drug → target → action type).

Optional: `CH_ID`, `CH_SEARCH`, `CH_LIMIT`, `CH_FORMAT`.

### `/chembl.similarity`

Server-side similarity search by SMILES. Returns molecules with
Tanimoto >= `CH_THRESHOLD` (default 0.7).

Required: `CH_SMILES`. Optional: `CH_THRESHOLD`, `CH_LIMIT`, `CH_FORMAT`.

### `/chembl.substructure`

Server-side substructure search. Returns molecules containing the query
SMILES as a substructure.

Required: `CH_SMILES`. Optional: `CH_LIMIT`, `CH_FORMAT`.

### `/chembl.image`

Download a compound image (SVG by default, or PNG) for a ChEMBL molecule ID.

Required: `CH_ID` (e.g. `CHEMBL25`). Optional: `CH_FORMAT`, `CH_OUTPUT`.

### `/chembl.run`

Generic catch-all. Forwards an arbitrary chembl_api.py subcommand. Use for
endpoints not exposed as named commands:

`assay_class`, `atc_class`, `binding_site`, `biotherapeutic`, `cell_line`,
`chembl_id_lookup`, `chembl_release`, `compound_record`,
`compound_structural_alert`, `document`, `document_similarity`,
`drug_indication`, `drug_warning`, `go_slim`, `metabolism`, `molecule_form`,
`organism`, `protein_classification`, `source`, `target_component`,
`target_relation`, `tissue`, `xref_source`.

Required: `CH_SUBCOMMAND`. Other args pass through.

Example — fetch a ChEMBL release record:

```bash
CH_SUBCOMMAND=chembl_release CH_ID=ChEMBL_33 \
zorai plugin invoke chembl run
```

## Limits

The ChEMBL REST API is anonymous-public. The script's `http_client` handles
rate limiting and pagination. For bulk pulls of >10k records, prefer
paginating with `CH_LIMIT` + `CH_OFFSET` explicitly.

## License

Plugin manifest + this skill file: MIT.
Vendored scripts: Apache 2.0.
ChEMBL data terms: <https://www.ebi.ac.uk/chembl/terms>.
