---
name: unibind-database
description: >
  Long-tail stub for the deepmind `unibind_database` skill. Queries the UniBind database for experimentally validated transcription factor (TF) binding sites. Use when retrieving direct TF-DNA interaction datasets, downloading binding site coordinates (BED/FASTA) for local analysis, or listing available datasets by species, cell line, or TF name. Don't use to query specific intervals, locations, genes, motif models or expression data.
  For the full workflow, read
  `skills/scientific-skills-gdm/unibind_database/SKILL.md` in the repo.
---

# unibind-database (zorai stub)

This is a **stub** sub-plugin. The canonical workflow and full command
reference live in the deepmind bundle at
`skills/scientific-skills-gdm/unibind_database/SKILL.md`. Read that first.

## Calling this stub

The stub exposes a single `run` command that forwards to the deepmind
Python entry script (`scripts/unibind_api.py`):

```bash
UNIBIND_DATABASE_ARGS="<deepmind-subcommand-and-its-flags>" \
zorai plugin invoke unibind-database run
```

Example (for `pubmed-database`):

```bash
PUBMED_DATABASE_ARGS="search --query 'BRCA1 AND clinsig_pathogenic' --output /tmp/pubmed.json" \
zorai plugin invoke pubmed-database run
```

If the skill has multiple Python scripts and you need a non-default one,
override with `SCRIPT=<other-script.py>` env var; the stub defaults to
`unibind_api.py`.

## Available subcommands (from the deepmind script)

- `download_tfbs`
- `get_dataset`
- `list_cell_lines`
- `list_collections`
- `list_datasets`
- `list_species`
- `list_specific_datasets`
- `list_tfs`

## Auth

See `skills/scientific-skills-gdm/unibind_database/SKILL.md` for the
auth requirements of this skill. The stub itself declares no settings
of its own.

## License

Plugin manifest + this stub: MIT.
Deepmind skill + vendored scripts: Apache 2.0 (see
`skills/scientific-skills-gdm/LICENSE`).
