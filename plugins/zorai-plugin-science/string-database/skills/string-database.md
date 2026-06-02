---
name: string-database
description: >
  Long-tail stub for the deepmind `string_database` skill. Query the STRING database for protein-protein interactions (PPIs), functional enrichment, and homology. Use when the user asks about interactions between specific proteins, interaction evidence, confidence scores, protein interaction partners, or pathway enrichments.
  For the full workflow, read
  `skills/scientific-skills-gdm/string_database/SKILL.md` in the repo.
---

# string-database (zorai stub)

This is a **stub** sub-plugin. The canonical workflow and full command
reference live in the deepmind bundle at
`skills/scientific-skills-gdm/string_database/SKILL.md`. Read that first.

## Calling this stub

The stub exposes a single `/string-database.run` command that forwards to the deepmind
Python entry script (`scripts/string_cli.py`).

```bash
STRING_DATABASE_ARGS="<deepmind-subcommand-and-its-flags>" \
/string-database.run
```

Replace `<deepmind-subcommand-and-its-flags>` with the subcommand and flags
described in the upstream skill.

## Available subcommands (from the deepmind script)

- `string_cli`

## Auth

See `skills/scientific-skills-gdm/string_database/SKILL.md` for the
auth requirements of this skill. The stub itself declares no settings
of its own.

## License

Plugin manifest + this stub: MIT.
Deepmind skill + vendored scripts: Apache 2.0 (see
`skills/scientific-skills-gdm/LICENSE`).
