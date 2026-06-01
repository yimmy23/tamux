---
name: openfda-database
description: >
  Long-tail stub for the deepmind `openfda_database` skill. Query, search, and download data from the openFDA API for drugs, devices, foods, tobacco, cosmetics, animal and veterinary products, substances, and transparency data. Use for FDA adverse events, recalls, labeling, approvals, shortages, 510(k) clearances, NDC lookups, and any FDA safety or regulatory data query across all 28 API endpoints.
  For the full workflow, read
  `skills/scientific-skills-gdm/openfda_database/SKILL.md` in the repo.
---

# openfda-database (zorai stub)

This is a **stub** sub-plugin. The canonical workflow and full command
reference live in the deepmind bundle at
`skills/scientific-skills-gdm/openfda_database/SKILL.md`. Read that first.

## Calling this stub

The stub exposes a single `run` command that forwards to the deepmind
Python entry script (`scripts/openfda_query.py`):

```bash
OPENFDA_DATABASE_ARGS="<deepmind-subcommand-and-its-flags>" \
zorai plugin invoke openfda-database run
```

Example (for `pubmed-database`):

```bash
PUBMED_DATABASE_ARGS="search --query 'BRCA1 AND clinsig_pathogenic' --output /tmp/pubmed.json" \
zorai plugin invoke pubmed-database run
```

If the skill has multiple Python scripts and you need a non-default one,
override with `SCRIPT=<other-script.py>` env var; the stub defaults to
`openfda_query.py`.

## Available subcommands (from the deepmind script)

- `count`
- `download`
- `search`

## Auth

See `skills/scientific-skills-gdm/openfda_database/SKILL.md` for the
auth requirements of this skill. The stub itself declares no settings
of its own.

## License

Plugin manifest + this stub: MIT.
Deepmind skill + vendored scripts: Apache 2.0 (see
`skills/scientific-skills-gdm/LICENSE`).
