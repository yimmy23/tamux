---
name: interpro-database
description: >
  Long-tail stub for the deepmind `interpro_database` skill. Identify domains, families, and sites in proteins; find all proteins in a family or sharing a domain; explore species distribution for a domain; annotate genomes with protein families and GO terms. InterPro combines 14 databases (e.g., Pfam, CDD) into one searchable resource. InterPro-N significantly expands annotation and sequence coverage with deep learning. Includes domain architecture (IDA) search.
  For the full workflow, read
  `skills/scientific-skills-gdm/interpro_database/SKILL.md` in the repo.
---

# interpro-database (zorai stub)

This is a **stub** sub-plugin. The canonical workflow and full command
reference live in the deepmind bundle at
`skills/scientific-skills-gdm/interpro_database/SKILL.md`. Read that first.

## Calling this stub

The stub exposes a single `/interpro-database.run` command that forwards to the deepmind
Python entry script (`scripts/interpro_client.py`).

```bash
INTERPRO_DATABASE_ARGS="<deepmind-subcommand-and-its-flags>" \
/interpro-database.run
```

Replace `<deepmind-subcommand-and-its-flags>` with the subcommand and flags
described in the upstream skill.

## Available subcommands (from the deepmind script)

- `count`
- `fetch`

## Auth

See `skills/scientific-skills-gdm/interpro_database/SKILL.md` for the
auth requirements of this skill. The stub itself declares no settings
of its own.

## License

Plugin manifest + this stub: MIT.
Deepmind skill + vendored scripts: Apache 2.0 (see
`skills/scientific-skills-gdm/LICENSE`).
