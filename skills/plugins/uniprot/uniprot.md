---
name: uniprot
description: >
  Access protein metadata, function, taxonomy, and sequences across UniProtKB,
  UniParc, and UniRef. Use when searching for proteins, mapping identifiers, or
  retrieving functional annotations and publications. Don't use for sequence
  alignment, protein folding, or sequence similarity search (use specialized
  skills for those tasks). Backed by the public UniProt REST API
  (https://rest.uniprot.org/) — no auth.
---

# UniProt Plugin

Use the **uniprot** plugin for protein metadata, function, taxonomy, and
sequence retrieval across UniProtKB, UniParc, and UniRef. The plugin wraps
the deepmind `uniprot_tools.py` script (single binary, multiple subcommands).

## Auth

None. UniProt REST is anonymous-public.

## Commands

The plugin exposes one zorai command per UniProt operation.

### `/uniprot.search`

Search proteins in a UniProt dataset with automatic pagination.

Required env: `UP_QUERY` (e.g. `"insulin AND organism_id:9606"`).
Optional: `UP_DATASET` (default `uniprotkb`), `UP_LIMIT`, `UP_FIELDS`
(comma-separated list, e.g. `"accession,id,gene_names,organism_name"`),
`UP_FORMAT` (default `json`).

Example:

```bash
UP_QUERY="gene:TP53 AND organism_id:9606" UP_LIMIT=10 UP_FIELDS="accession,id,gene_names,length" \
/uniprot.search
```

### `/uniprot.get`

Retrieve a single UniProt entry by accession.

Required env: `UP_ACCESSION` (e.g. `P04637`).
Optional: `UP_DATASET`, `UP_FORMAT`.

### `/uniprot.map`

Map IDs between databases (e.g. RefSeq → UniProt, Ensembl → UniProt).
Polls the upstream job to completion.

Required env: `UP_IDS` (comma-separated), `UP_FROM_DB`, `UP_TO_DB`.
Example: `UP_FROM_DB=RefSeq_Protein UP_TO_DB=UniProtKB`.

### `/uniprot.count`

Count hits for a query without retrieving the records. Useful before a
bulk pull to know the size.

Required env: `UP_QUERY`. Optional: `UP_DATASET`.

### `/uniprot.sparql`

Execute a SPARQL query against the UniProt endpoint. For complex graph
queries the SPARQL endpoint is more expressive than the search API.

Required env: `UP_SPARQL` (the full SPARQL query string).

### `/uniprot.stream`

Stream ALL results for a bulk query using `/stream` (up to 10M entries,
no limit). **Use with care — output can be very large.** Prefer
`/uniprot.search` with `UP_LIMIT` unless you genuinely need everything.

Required env: `UP_QUERY`. Optional: `UP_DATASET`.

## Error patterns

- **400 from `search` / `get` / `stream`** — the query string is malformed.
  Check the UniProt query syntax docs.
- **404 from `get`** — the accession does not exist (typo, or a secondary
  accession that is not a primary).
- **`map` polling times out** — the job queue is under load. The script
  surfaces the job URL on failure; you can re-run with the same IDs.

## Limits

The UniProt REST API is anonymous-public with rate limits enforced by the
script's internal `http_client`. No plugin-level rate limiting needed.

## License

Plugin manifest + this skill file: MIT.
Vendored scripts: Apache 2.0.
UniProt data terms: <https://www.uniprot.org/help/license>.
