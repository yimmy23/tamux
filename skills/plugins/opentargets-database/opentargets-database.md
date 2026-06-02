---
name: opentargets-database
description: >
  Long-tail stub for the deepmind `opentargets_database` skill. Query Open Targets Platform for target-disease associations, drug target discovery, tractability/safety data, genetics/omics evidence, known drugs, for therapeutic target identification.
  For the full workflow, read
  `skills/scientific-skills-gdm/opentargets_database/SKILL.md` in the repo.
---

# opentargets-database (zorai stub)

This is a **stub** sub-plugin. The canonical workflow and full command
reference live in the deepmind bundle at
`skills/scientific-skills-gdm/opentargets_database/SKILL.md`. Read that first.

## Calling this stub

The stub exposes a single `/opentargets-database.run` command that forwards to the deepmind
Python entry script (`scripts/query_opentargets.py`).

```bash
OPENTARGETS_DATABASE_ARGS="<deepmind-subcommand-and-its-flags>" \
/opentargets-database.run
```

Replace `<deepmind-subcommand-and-its-flags>` with the subcommand and flags
described in the upstream skill.

## Available subcommands (from the deepmind script)

- `custom-query`
- `get-associated-diseases`
- `get-associated-targets`
- `get-credible-sets-near-target`
- `get-gwas-studies`
- `get-l2g`
- `get-qtl-credible-sets`
- `get-study-credible-sets`
- `get-target-druggability`
- `search-disease`

## Auth

See `skills/scientific-skills-gdm/opentargets_database/SKILL.md` for the
auth requirements of this skill. The stub itself declares no settings
of its own.

## License

Plugin manifest + this stub: MIT.
Deepmind skill + vendored scripts: Apache 2.0 (see
`skills/scientific-skills-gdm/LICENSE`).
