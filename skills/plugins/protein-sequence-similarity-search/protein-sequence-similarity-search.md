---
name: protein-sequence-similarity-search
description: >
  Long-tail stub for the deepmind `protein_sequence_similarity_search` skill. Searches for homologous protein sequences using MMseqs2 (fast, default) or BLAST (comprehensive, fallback). Trigger this whenever the user provides a protein sequence or FASTA file and asks to find homologues, sequence matches, or wants to infer protein function based on sequence similarity, but not when the user wants to infer protein function based on structural similarity.
  For the full workflow, read
  `skills/scientific-skills-gdm/protein_sequence_similarity_search/SKILL.md` in the repo.
---

# protein-sequence-similarity-search (zorai stub)

This is a **stub** sub-plugin. The canonical workflow and full command
reference live in the deepmind bundle at
`skills/scientific-skills-gdm/protein_sequence_similarity_search/SKILL.md`. Read that first.

## Calling this stub

The stub exposes a single `run` command that forwards to the deepmind
Python entry script (`scripts/mmseqs2_search.py`):

```bash
PROTEIN_SEQUENCE_SIMILARITY_SEARCH_ARGS="<deepmind-subcommand-and-its-flags>" \
zorai plugin invoke protein-sequence-similarity-search run
```

Example (for `pubmed-database`):

```bash
PUBMED_DATABASE_ARGS="search --query 'BRCA1 AND clinsig_pathogenic' --output /tmp/pubmed.json" \
zorai plugin invoke pubmed-database run
```

If the skill has multiple Python scripts and you need a non-default one,
override with `SCRIPT=<other-script.py>` env var; the stub defaults to
`mmseqs2_search.py`.

## Available subcommands (from the deepmind script)

- `mmseqs2_search`
- `uniprot_blast`

## Auth

See `skills/scientific-skills-gdm/protein_sequence_similarity_search/SKILL.md` for the
auth requirements of this skill. The stub itself declares no settings
of its own.

## License

Plugin manifest + this stub: MIT.
Deepmind skill + vendored scripts: Apache 2.0 (see
`skills/scientific-skills-gdm/LICENSE`).
