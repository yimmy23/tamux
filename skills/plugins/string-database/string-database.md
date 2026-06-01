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

The stub exposes a single `run` command that forwards to the deepmind
Python entry script (`scripts/string_cli.py`):

```bash
STRING_DATABASE_ARGS="<deepmind-subcommand-and-its-flags>" \
zorai plugin invoke string-database run
```

Example (for `pubmed-database`):

```bash
PUBMED_DATABASE_ARGS="search --query 'BRCA1 AND clinsig_pathogenic' --output /tmp/pubmed.json" \
zorai plugin invoke pubmed-database run
```

If the skill has multiple Python scripts and you need a non-default one,
override with `SCRIPT=<other-script.py>` env var; the stub defaults to
`string_cli.py`.

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
