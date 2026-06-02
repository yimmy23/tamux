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

The stub exposes a single `/unibind-database.run` command that forwards to the deepmind
Python entry script (`scripts/unibind_api.py`).

```bash
UNIBIND_DATABASE_ARGS="<deepmind-subcommand-and-its-flags>" \
/unibind-database.run
```

Replace `<deepmind-subcommand-and-its-flags>` with the subcommand and flags
described in the upstream skill.

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
