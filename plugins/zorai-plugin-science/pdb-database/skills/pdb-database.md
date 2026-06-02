---
name: pdb-database
description: >
  Use when you want to search for or download experimentally-determined 3D
  structures for biomolecules (proteins, nucleic acids, bound ligands).
  Supports searching by sequence similarity, structure similarity, chemical
  and other attributes. Also use to get metadata about biomolecular structure
  experiments. Pairs with the alphafold-database sub-plugin (predicted vs
  experimental structures). Backed by the public RCSB APIs — no auth.
---

# PDB Database Plugin (RCSB)

Use the **pdb-database** plugin for experimental 3D structures from the
[RCSB Protein Data Bank](https://www.rcsb.org/). Pairs naturally with
the **alphafold-database** sub-plugin (predicted vs experimental
structures — most workflows query both and compare).

## Auth

None. RCSB APIs (Search v2, Data API GraphQL, file download) are
anonymous-public. Soft rate limit ~10 req/s per IP; the deepmind scripts
honor `Retry-After` on 429s.

## Common env vars

- `PDB_OUTPUT` — output file path. **Required** for `search`, `fetch-metadata`, `fetch-schema`.
- `PDB_IDS` — comma-separated PDB IDs (e.g. `1A2B,4HHB`). **Required** for `download-coordinates`.
- `PDB_OUTPUT_DIR` — output directory. **Required** for `download-coordinates`.
- `PDB_QUERY` — JSON query for `search` and `fetch-metadata`. The
  deepmind `fetch-schema` command can introspect the schema if you need
  to build a new query.
- `PDB_FORMAT` — `cif` (mmCIF, default) or `pdb` for `download-coordinates`.

## Query format

The RCSB Search API v2 takes a JSON query in the
[POST query syntax](https://search.rcsb.org/structure-search-attributes.html).
Example — find all human hemoglobin structures with resolution < 2.0 Å:

```json
{
  "query": {
    "type": "group",
    "logical_operator": "and",
    "nodes": [
      {
        "type": "terminal",
        "service": "text",
        "parameters": {
          "attribute": "rcsb_entity_source_organism.scientific_name",
          "operator": "exact_match",
          "value": "Homo sapiens"
        }
      },
      {
        "type": "terminal",
        "service": "text",
        "parameters": {
          "attribute": "rcsb_entry_info.resolution_combined",
          "operator": "range",
          "value": {"from": 0.0, "to": 2.0}
        }
      }
    ]
  },
  "return_type": "entry",
  "request_options": {"paginate": {"start": 0, "rows": 25}}
}
```

Pass this as `PDB_QUERY='{"query":..., "return_type": "entry"}'`.

## Commands

### `/pdb-database.search`

Search the PDB. Returns up to `--rows` matching entries (default 25).

Required env: `PDB_QUERY` (JSON), `PDB_OUTPUT`.
Optional: `PDB_RETURN_TYPE` (`entry` default, `polymer_entity`, `assembly`, etc.),
`PDB_SORT_BY`, `PDB_SORT_DIRECTION`, `PDB_PAGE_START`, `PDB_ROWS`,
`PDB_COUNT_ONLY` (returns just the count, faster).

Example — find a specific PDB entry:

```bash
PDB_QUERY='{"query": {"type": "terminal", "service": "text", "parameters": {"attribute": "rcsb_entry_container_identifiers.entry_id", "operator": "exact_match", "value": "1A2B"}}, "return_type": "entry"}' \
PDB_OUTPUT=./pdb_1a2b.json \
/pdb-database.search
```

### `/pdb-database.fetch-metadata`

Fetch rich per-entry metadata via GraphQL. Use when you need resolution,
R-factor, chain composition, bound ligands, citation info, etc.

Required env: `PDB_QUERY` (GraphQL query string), `PDB_OUTPUT`.

Example:

```bash
PDB_QUERY='{ entries(entry_ids: ["1A2B","4HHB"]) { rcsb_entry_info { resolution_combined } rcsb_accession_info { initial_release_date } } }' \
PDB_OUTPUT=./pdb_metadata.json \
/pdb-database.fetch-metadata
```

### `/pdb-database.download-coordinates`

Download coordinate files (mmCIF or PDB format) for one or more entries.
Files are written to `--output_dir` as `<ID>.cif` or `<ID>.pdb`.

Required env: `PDB_IDS` (comma-separated), `PDB_OUTPUT_DIR`.
Optional: `PDB_FORMAT` (`cif` default, or `pdb`).

Example — download 3 structures:

```bash
PDB_IDS=1A2B,4HHB,1CRN PDB_FORMAT=cif PDB_OUTPUT_DIR=./pdbs/ \
/pdb-database.download-coordinates
# -> ./pdbs/1A2B.cif  ./pdbs/4HHB.cif  ./pdbs/1CRN.cif
```

### `/pdb-database.fetch-schema`

Introspect the RCSB Data API GraphQL schema. Use to discover available
fields and types when building new queries.

Required env: `PDB_OUTPUT`. Optional: `PDB_API` (`data` default,
`data` for the GraphQL API; this script mainly supports the data API).

## Cross-references

- **alphafold-database** plugin: for a given protein, fetch the
  experimental structure from PDB and the predicted structure from
  AlphaFold, then compare (RMSD, per-residue confidence overlay).
- **uniprot** plugin: resolve a UniProt accession to a PDB ID via
  UniProt's cross-references, then download the structure here.
- **ensembl** plugin: resolve a gene to its ENST transcript, fetch the
  translated protein sequence, then search PDB for matching structures.

## License

Plugin manifest + this skill file: MIT.
Vendored scripts: Apache 2.0.
PDB data terms: <https://www.rcsb.org/pages/policies>.
