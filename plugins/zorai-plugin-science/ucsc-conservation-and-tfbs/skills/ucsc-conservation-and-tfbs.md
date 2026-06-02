---
name: ucsc-conservation-and-tfbs
description: >
  Long-tail stub for the deepmind `ucsc_conservation_and_tfbs` skill. Fetch Evolutionary Conservation scores (phyloP, phastCons) and Transcription Factor Binding Sites (TFBS) from the UCSC Genome Browser. Use when analyzing whether genomic variants or regions are evolutionarily conserved, functionally important, or bounded by TF regulators across major projects (ENCODE, JASPAR, ReMap).
  For the full workflow, read
  `skills/scientific-skills-gdm/ucsc_conservation_and_tfbs/SKILL.md` in the repo.
---

# ucsc-conservation-and-tfbs (zorai stub)

This is a **stub** sub-plugin. The canonical workflow and full command
reference live in the deepmind bundle at
`skills/scientific-skills-gdm/ucsc_conservation_and_tfbs/SKILL.md`. Read that first.

## Calling this stub

The stub exposes a single `/ucsc-conservation-and-tfbs.run` command that forwards to the deepmind
Python entry script (`scripts/get_conservation.py`).

```bash
UCSC_CONSERVATION_AND_TFBS_ARGS="<deepmind-subcommand-and-its-flags>" \
/ucsc-conservation-and-tfbs.run
```

Replace `<deepmind-subcommand-and-its-flags>` with the subcommand and flags
described in the upstream skill.

## Available subcommands (from the deepmind script)

- `get_conservation`
- `get_tfbs`
- `list_tracks`

## Auth

See `skills/scientific-skills-gdm/ucsc_conservation_and_tfbs/SKILL.md` for the
auth requirements of this skill. The stub itself declares no settings
of its own.

## License

Plugin manifest + this stub: MIT.
Deepmind skill + vendored scripts: Apache 2.0 (see
`skills/scientific-skills-gdm/LICENSE`).
