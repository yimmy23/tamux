---
name: encode-ccres-database
description: >
  Long-tail stub for the deepmind `encode_ccres_database` skill. Query the ENCODE Registry of cis-Regulatory Elements (cCREs) via the SCREEN GraphQL API, or make custom queries to the ENCODE Portal REST API for experiments and files (ChIP-seq peaks, etc.). Use when you want to query regulatory annotations or raw experimental data across human cell types.
  For the full workflow, read
  `skills/scientific-skills-gdm/encode_ccres_database/SKILL.md` in the repo.
---

# encode-ccres-database (zorai stub)

This is a **stub** sub-plugin. The canonical workflow and full command
reference live in the deepmind bundle at
`skills/scientific-skills-gdm/encode_ccres_database/SKILL.md`. Read that first.

## Calling this stub

The stub exposes a single `run` command that forwards to the deepmind
Python entry script (`scripts/encode_portal_api.py`):

```bash
ENCODE_CCRES_DATABASE_ARGS="<deepmind-subcommand-and-its-flags>" \
zorai plugin invoke encode-ccres-database run
```

Example (for `pubmed-database`):

```bash
PUBMED_DATABASE_ARGS="search --query 'BRCA1 AND clinsig_pathogenic' --output /tmp/pubmed.json" \
zorai plugin invoke pubmed-database run
```

If the skill has multiple Python scripts and you need a non-default one,
override with `SCRIPT=<other-script.py>` env var; the stub defaults to
`encode_portal_api.py`.

## Available subcommands (from the deepmind script)

- `biosamples`
- `details`
- `entex`
- `gene-expression`
- `gwas`
- `linked-genes`
- `nearby-genes`
- `orthologs`
- `search`

## Auth

See `skills/scientific-skills-gdm/encode_ccres_database/SKILL.md` for the
auth requirements of this skill. The stub itself declares no settings
of its own.

## License

Plugin manifest + this stub: MIT.
Deepmind skill + vendored scripts: Apache 2.0 (see
`skills/scientific-skills-gdm/LICENSE`).
