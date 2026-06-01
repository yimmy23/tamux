---
name: clinical-trials-database
description: >
  Long-tail stub for the deepmind `clinical_trials_database` skill. Query ClinicalTrials.gov via APIv2. Use when you want to search for trials by condition, drug, location, status, or phase; retrieve trial details by NCT ID; check eligibility/inclusion criteria; count trials across conditions or time periods; identify a sponsor's trial portfolio; find recruiting trials for patient matching.
  For the full workflow, read
  `skills/scientific-skills-gdm/clinical_trials_database/SKILL.md` in the repo.
---

# clinical-trials-database (zorai stub)

This is a **stub** sub-plugin. The canonical workflow and full command
reference live in the deepmind bundle at
`skills/scientific-skills-gdm/clinical_trials_database/SKILL.md`. Read that first.

## Calling this stub

The stub exposes a single `run` command that forwards to the deepmind
Python entry script (`scripts/clinical_trials_api.py`):

```bash
CLINICAL_TRIALS_DATABASE_ARGS="<deepmind-subcommand-and-its-flags>" \
zorai plugin invoke clinical-trials-database run
```

Example (for `pubmed-database`):

```bash
PUBMED_DATABASE_ARGS="search --query 'BRCA1 AND clinsig_pathogenic' --output /tmp/pubmed.json" \
zorai plugin invoke pubmed-database run
```

If the skill has multiple Python scripts and you need a non-default one,
override with `SCRIPT=<other-script.py>` env var; the stub defaults to
`clinical_trials_api.py`.

## Available subcommands (from the deepmind script)

- `count`
- `get-eligibility`
- `get-study`
- `raw-query`
- `search`

## Auth

See `skills/scientific-skills-gdm/clinical_trials_database/SKILL.md` for the
auth requirements of this skill. The stub itself declares no settings
of its own.

## License

Plugin manifest + this stub: MIT.
Deepmind skill + vendored scripts: Apache 2.0 (see
`skills/scientific-skills-gdm/LICENSE`).
