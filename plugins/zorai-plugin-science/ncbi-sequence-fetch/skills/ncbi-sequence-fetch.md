---
name: ncbi-sequence-fetch
description: >
  Long-tail stub for the deepmind `ncbi_sequence_fetch` skill. Retrieve protein and nucleotide sequences from NCBI databases using E-utilities. Supports direct accession lookup, CDS translation, gene+organism search, locus lookup, PubMed-linked sequences, patent protein extraction, and organism+length fallback search. Use when you need to fetch biological sequences by accession, gene name, locus tag, PubMed ID, or patent number.
  For the full workflow, read
  `skills/scientific-skills-gdm/ncbi_sequence_fetch/SKILL.md` in the repo.
---

# ncbi-sequence-fetch (zorai stub)

This is a **stub** sub-plugin. The canonical workflow and full command
reference live in the deepmind bundle at
`skills/scientific-skills-gdm/ncbi_sequence_fetch/SKILL.md`. Read that first.

## Calling this stub

The stub exposes a single `run` command that forwards to the deepmind
Python entry script (`scripts/ncbi_fetch.py`):

```bash
NCBI_SEQUENCE_FETCH_ARGS="<deepmind-subcommand-and-its-flags>" \
zorai plugin invoke ncbi-sequence-fetch run
```

Example (for `pubmed-database`):

```bash
PUBMED_DATABASE_ARGS="search --query 'BRCA1 AND clinsig_pathogenic' --output /tmp/pubmed.json" \
zorai plugin invoke pubmed-database run
```

If the skill has multiple Python scripts and you need a non-default one,
override with `SCRIPT=<other-script.py>` env var; the stub defaults to
`ncbi_fetch.py`.

## Available subcommands (from the deepmind script)

- `ncbi_fetch`

## Auth

See `skills/scientific-skills-gdm/ncbi_sequence_fetch/SKILL.md` for the
auth requirements of this skill. The stub itself declares no settings
of its own.

## License

Plugin manifest + this stub: MIT.
Deepmind skill + vendored scripts: Apache 2.0 (see
`skills/scientific-skills-gdm/LICENSE`).
