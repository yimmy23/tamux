---
name: ncbi-sequence-fetch
description: >
  Retrieve protein and nucleotide sequences from NCBI databases using
  E-utilities. Supports direct accession lookup, CDS translation, gene+organism
  search, locus lookup, PubMed-linked sequences, patent protein extraction, and
  organism+length fallback search. Use when you need to fetch biological
  sequences by accession, gene name, locus tag, PubMed ID, or patent number.
  The central sequence-retrieval tool — pairs with clinvar (variant context),
  pubmed (literature->sequence), uniprot (cross-reference), ensembl (gene->protein).
  Backed by the public NCBI E-utilities — no auth required but optional
  NCBI_API_KEY raises the rate limit from ~3 to ~10 req/s.
---

# NCBI Sequence Fetch Plugin

Use the **ncbi-sequence-fetch** plugin to retrieve biological sequences
from NCBI via [E-utilities](https://eutils.ncbi.nlm.nih.gov/). Pairs
naturally with **clinvar** (NCBI E-utilities, optional API key), **pubmed**
(PubMed ID -> linked proteins), **uniprot** (cross-reference), and **ensembl**
(gene symbol -> protein).

## Auth (optional)

`NCBI_API_KEY` setting is **optional**. Without it, E-utilities allows
~3 req/s. With one, the limit is ~10 req/s. Get a key at
<https://www.ncbi.nlm.nih.gov/account/settings/>. The script loads
the key from `~/.env` via `dotenv` (looks for `NCBI_API_KEY`).

## Common env vars

- `NCBI_OUTPUT` — output JSON file path. **Required** for every command.
- `NCBI_ACCESSION` — accession ID(s), space-separated. **Required** for
  `fetch-protein`, `fetch-nucleotide`.
- `NCBI_QUERY` — Entrez search query. **Required** for `search`.
- `NCBI_DATABASE` — NCBI database to query (default `protein`). Common:
  `nucleotide`, `gene`, `pubmed`, `omim`, `clinvar`, `structure`, `assembly`.
- `NCBI_GENE` + `NCBI_ORGANISM` — for `gene-protein`.
- `NCBI_LOCUS` — for `locus-protein`.
- `NCBI_DB_FROM` + `NCBI_DB_TO` — for `elink`.
- `NCBI_RETMAX` — max results (default 20) for `search`.

## Common workflow: gene symbol -> protein sequence

```
   gene symbol + organism
        |
        v
   /ncbi-sequence-fetch.gene-protein   <-- returns accession(s)
        |
        v
   /ncbi-sequence-fetch.fetch-protein   <-- FASTA to disk
```

## Commands

### `/ncbi-sequence-fetch.fetch-protein`

Fetch protein sequence(s) by NCBI protein accession(s).

Required env: `NCBI_ACCESSION` (one or more, space-separated),
`NCBI_OUTPUT`.

Example:

```bash
NCBI_ACCESSION="NP_000537 P04637" NCBI_OUTPUT=./p53_protein.fasta \
/ncbi-sequence-fetch.fetch-protein
```

### `/ncbi-sequence-fetch.fetch-nucleotide`

Fetch nucleotide sequence(s) by NCBI nucleotide accession(s).

Required env: `NCBI_ACCESSION`, `NCBI_OUTPUT`.

### `/ncbi-sequence-fetch.cds-translate`

Fetch a CDS accession and translate to protein. Use to verify ORFs or
to get a protein from a nucleotide entry without a separate protein
accession.

Required env: `NCBI_CDS_ACCESSION`, `NCBI_OUTPUT`.

### `/ncbi-sequence-fetch.search`

Search any NCBI database by Entrez query. Returns matching IDs and
total count.

Required env: `NCBI_QUERY` (Entrez syntax), `NCBI_OUTPUT`.
Optional: `NCBI_DATABASE` (default `protein`), `NCBI_RETMAX` (default 20).

Example — search PubMed for papers citing p53:

```bash
NCBI_QUERY="p53[Title] AND 2024[PDAT]" \
NCBI_DATABASE=pubmed NCBI_RETMAX=10 \
NCBI_OUTPUT=./pubmed_p53_2024.json \
/ncbi-sequence-fetch.search
```

### `/ncbi-sequence-fetch.elink`

Follow cross-database links from one NCBI database to another.

Required env: `NCBI_ID` (source ID), `NCBI_DB_FROM`, `NCBI_DB_TO`,
`NCBI_OUTPUT`.

Example — get all protein accessions linked to a PubMed article:

```bash
NCBI_ID=35000000 NCBI_DB_FROM=pubmed NCBI_DB_TO=protein \
NCBI_OUTPUT=./pubmed_proteins.json \
/ncbi-sequence-fetch.elink
```

### `/ncbi-sequence-fetch.gene-protein`

Search for a protein sequence by gene name + organism. Useful when
you have a gene symbol but not a direct accession.

Required env: `NCBI_GENE`, `NCBI_ORGANISM`, `NCBI_OUTPUT`.

Example:

```bash
NCBI_GENE=TP53 NCBI_ORGANISM="Homo sapiens" \
NCBI_OUTPUT=./tp53_protein.fasta \
/ncbi-sequence-fetch.gene-protein
```

### `/ncbi-sequence-fetch.locus-protein`

Search for a protein sequence by locus tag. Use for microbial genetics
workflows where you have a locus tag from a genome annotation.

Required env: `NCBI_LOCUS`, `NCBI_OUTPUT`.

### `/ncbi-sequence-fetch.run`

Generic catch-all. Forwards an arbitrary `ncbi_fetch.py` subcommand.
Use for the 3 specialized subcommands not exposed as named
(`pubmed-proteins`, `patent-search`, `organism-length`) or to pass
forward args to the named subcommands without mapping each flag.

Required env: `NCBI_SUBCOMMAND`, `NCBI_OUTPUT`. Other args pass through.

Example — find proteins linked to a patent number:

```bash
NCBI_SUBCOMMAND=patent-search NCBI_ACCESSION=US12345678 \
NCBI_OUTPUT=./patent_proteins.json \
/ncbi-sequence-fetch.run
```

## Limits

- **Anonymous: 3 req/s** with bursts up to ~10. **With key: 10 req/s.**
  The script's internal rate limiter (deepmind default `qps=3.0`) handles
  this automatically.
- For >10k records, the upstream recommends `epost`/`efetch` batching
  with the Entrez history server. This script does direct `esearch` +
  `efetch`; for very large queries, prefer NCBI's web Entrez interface.

## Cross-references

- **clinvar** plugin: pull a ClinVar variant's linked gene, then
  `fetch-protein` for the protein sequence here.
- **pubmed** plugin (long-tail stub): get a PubMed article's linked
  proteins via `elink` from pubmed to protein.
- **uniprot** plugin: cross-reference a UniProt accession against
  NCBI to confirm canonical accession.
- **ensembl** plugin: resolve a gene symbol to ENSG/ENST, then fetch
  the matching NCBI protein here for cross-validation.

## License

Plugin manifest + this skill file: MIT.
Vendored scripts: Apache 2.0.
NCBI data terms: <https://www.ncbi.nlm.nih.gov/home/about/policies/>.
