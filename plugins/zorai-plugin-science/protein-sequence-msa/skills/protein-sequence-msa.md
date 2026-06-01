---
name: protein-sequence-msa
description: >
  Long-tail stub for the deepmind `protein_sequence_msa` skill. Performs multiple sequence alignment of proteins with EBI Clustal Omega. Use when you need to align multiple sequences to assess similarity, domain conservation, or key residue conservation. Supports up to 4000 sequences and a maximum file size of 4 MB. Do not use to search for homologous proteins in a database (use MMseqs2, BLAST), align non-protein sequences (DNA, RNA), perform structural alignment (use Foldseek, PyMOL), or if you only have a single sequence.
  For the full workflow, read
  `skills/scientific-skills-gdm/protein_sequence_msa/SKILL.md` in the repo.
---

# protein-sequence-msa (zorai stub)

This is a **stub** sub-plugin. The canonical workflow and full command
reference live in the deepmind bundle at
`skills/scientific-skills-gdm/protein_sequence_msa/SKILL.md`. Read that first.

## Calling this stub

The stub exposes a single `run` command that forwards to the deepmind
Python entry script (`scripts/msa_align.py`):

```bash
PROTEIN_SEQUENCE_MSA_ARGS="<deepmind-subcommand-and-its-flags>" \
zorai plugin invoke protein-sequence-msa run
```

Example (for `pubmed-database`):

```bash
PUBMED_DATABASE_ARGS="search --query 'BRCA1 AND clinsig_pathogenic' --output /tmp/pubmed.json" \
zorai plugin invoke pubmed-database run
```

If the skill has multiple Python scripts and you need a non-default one,
override with `SCRIPT=<other-script.py>` env var; the stub defaults to
`msa_align.py`.

## Available subcommands (from the deepmind script)

- `msa_align`

## Auth

See `skills/scientific-skills-gdm/protein_sequence_msa/SKILL.md` for the
auth requirements of this skill. The stub itself declares no settings
of its own.

## License

Plugin manifest + this stub: MIT.
Deepmind skill + vendored scripts: Apache 2.0 (see
`skills/scientific-skills-gdm/LICENSE`).
