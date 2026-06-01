---
name: pdb-database
description: >
  Long-tail stub for the deepmind `pdb_database` skill. Use when you want to search for or download experimentally-determined 3D structures for biomolecules (proteins, nucleic acids, bound ligands). Supports searching by sequence similarity, structure similarity, chemical and other attributes. Also use to get metadata about biomolecular structure experiments.
  For the full workflow, read
  `skills/scientific-skills-gdm/pdb_database/SKILL.md` in the repo.
---

# pdb-database (zorai stub)

This is a **stub** sub-plugin. The canonical workflow and full command
reference live in the deepmind bundle at
`skills/scientific-skills-gdm/pdb_database/SKILL.md`. Read that first.

## Calling this stub

The stub exposes a single `run` command that forwards to the deepmind
Python entry script (`scripts/download_coordinate_files.py`):

```bash
PDB_DATABASE_ARGS="<deepmind-subcommand-and-its-flags>" \
zorai plugin invoke pdb-database run
```

Example (for `pubmed-database`):

```bash
PUBMED_DATABASE_ARGS="search --query 'BRCA1 AND clinsig_pathogenic' --output /tmp/pubmed.json" \
zorai plugin invoke pubmed-database run
```

If the skill has multiple Python scripts and you need a non-default one,
override with `SCRIPT=<other-script.py>` env var; the stub defaults to
`download_coordinate_files.py`.

## Available subcommands (from the deepmind script)

- `download_coordinate_files`
- `fetch_pdb_metadata`
- `fetch_schema`
- `search_pdb`

## Auth

See `skills/scientific-skills-gdm/pdb_database/SKILL.md` for the
auth requirements of this skill. The stub itself declares no settings
of its own.

## License

Plugin manifest + this stub: MIT.
Deepmind skill + vendored scripts: Apache 2.0 (see
`skills/scientific-skills-gdm/LICENSE`).
