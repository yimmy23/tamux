---
name: human-protein-atlas-database
description: >
  Long-tail stub for the deepmind `human_protein_atlas_database` skill. Use when you want to retrieve semi-quantitative protein expression and spatial localisation data from the Human Protein Atlas (HPA).
  For the full workflow, read
  `skills/scientific-skills-gdm/human_protein_atlas_database/SKILL.md` in the repo.
---

# human-protein-atlas-database (zorai stub)

This is a **stub** sub-plugin. The canonical workflow and full command
reference live in the deepmind bundle at
`skills/scientific-skills-gdm/human_protein_atlas_database/SKILL.md`. Read that first.

## Calling this stub

The stub exposes a single `run` command that forwards to the deepmind
Python entry script (`scripts/hpa_cli.py`):

```bash
HUMAN_PROTEIN_ATLAS_DATABASE_ARGS="<deepmind-subcommand-and-its-flags>" \
zorai plugin invoke human-protein-atlas-database run
```

Example (for `pubmed-database`):

```bash
PUBMED_DATABASE_ARGS="search --query 'BRCA1 AND clinsig_pathogenic' --output /tmp/pubmed.json" \
zorai plugin invoke pubmed-database run
```

If the skill has multiple Python scripts and you need a non-default one,
override with `SCRIPT=<other-script.py>` env var; the stub defaults to
`hpa_cli.py`.

## Available subcommands (from the deepmind script)

- `hpa_cli`

## Auth

See `skills/scientific-skills-gdm/human_protein_atlas_database/SKILL.md` for the
auth requirements of this skill. The stub itself declares no settings
of its own.

## License

Plugin manifest + this stub: MIT.
Deepmind skill + vendored scripts: Apache 2.0 (see
`skills/scientific-skills-gdm/LICENSE`).
