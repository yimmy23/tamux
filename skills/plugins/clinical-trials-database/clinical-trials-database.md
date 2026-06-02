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

The stub exposes a single `/clinical-trials-database.run` command that forwards to the deepmind
Python entry script (`scripts/clinical_trials_api.py`).

```bash
CLINICAL_TRIALS_DATABASE_ARGS="<deepmind-subcommand-and-its-flags>" \
/clinical-trials-database.run
```

Replace `<deepmind-subcommand-and-its-flags>` with the subcommand and flags
described in the upstream skill.

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
