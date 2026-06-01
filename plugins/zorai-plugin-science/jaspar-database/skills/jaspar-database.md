---
name: jaspar-database
description: >
  Long-tail stub for the deepmind `jaspar_database` skill. Query the JASPAR database for Transcription Factor (TF) binding profiles. Use when retrieving Position Frequency Matrices (PFMs) or Position Weight Matrices (PWMs) for specific TFs, resolving gene symbols to JASPAR Matrix IDs, or getting TF metadata. Supports multiple output formats (MEME, TRANSFAC, PFM, JASPAR, YAML).
  For the full workflow, read
  `skills/scientific-skills-gdm/jaspar_database/SKILL.md` in the repo.
---

# jaspar-database (zorai stub)

This is a **stub** sub-plugin. The canonical workflow and full command
reference live in the deepmind bundle at
`skills/scientific-skills-gdm/jaspar_database/SKILL.md`. Read that first.

## Calling this stub

The stub exposes a single `run` command that forwards to the deepmind
Python entry script (`scripts/jaspar_api.py`):

```bash
JASPAR_DATABASE_ARGS="<deepmind-subcommand-and-its-flags>" \
zorai plugin invoke jaspar-database run
```

Example (for `pubmed-database`):

```bash
PUBMED_DATABASE_ARGS="search --query 'BRCA1 AND clinsig_pathogenic' --output /tmp/pubmed.json" \
zorai plugin invoke pubmed-database run
```

If the skill has multiple Python scripts and you need a non-default one,
override with `SCRIPT=<other-script.py>` env var; the stub defaults to
`jaspar_api.py`.

## Available subcommands (from the deepmind script)

- `get_tf_metadata`
- `get_tf_motif`
- `get_tf_pwm`
- `get_tffm`
- `infer_from_sequence`
- `resolve_tf_id`

## Auth

See `skills/scientific-skills-gdm/jaspar_database/SKILL.md` for the
auth requirements of this skill. The stub itself declares no settings
of its own.

## License

Plugin manifest + this stub: MIT.
Deepmind skill + vendored scripts: Apache 2.0 (see
`skills/scientific-skills-gdm/LICENSE`).
