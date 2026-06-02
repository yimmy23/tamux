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

The stub exposes a single `/human-protein-atlas-database.run` command that forwards to the deepmind
Python entry script (`scripts/hpa_cli.py`).

```bash
HUMAN_PROTEIN_ATLAS_DATABASE_ARGS="<deepmind-subcommand-and-its-flags>" \
/human-protein-atlas-database.run
```

Replace `<deepmind-subcommand-and-its-flags>` with the subcommand and flags
described in the upstream skill.

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
