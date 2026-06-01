---
name: pymol
description: >
  Long-tail stub for the deepmind `pymol` skill. Visualize, analyze, and render protein and molecular structures using PyMOL. Use when the user wants to create images of protein structures, perform structural alignments or superposition, measure distances or contacts, highlight binding sites or active site residues, color by B-factor/pLDDT, or analyze protein-ligand interactions. Do not use for docking, molecular dynamics, or sequence-only analysis.
  For the full workflow, read
  `skills/scientific-skills-gdm/pymol/SKILL.md` in the repo.
---

# pymol (zorai stub)

This is a **stub** sub-plugin. The canonical workflow and full command
reference live in the deepmind bundle at
`skills/scientific-skills-gdm/pymol/SKILL.md`. Read that first.

## Calling this stub

The stub exposes a single `run` command that forwards to the deepmind
Python entry script (`scripts/None`):

```bash
PYMOL_ARGS="<deepmind-subcommand-and-its-flags>" \
zorai plugin invoke pymol run
```

Example (for `pubmed-database`):

```bash
PUBMED_DATABASE_ARGS="search --query 'BRCA1 AND clinsig_pathogenic' --output /tmp/pubmed.json" \
zorai plugin invoke pubmed-database run
```

If the skill has multiple Python scripts and you need a non-default one,
override with `SCRIPT=<other-script.py>` env var; the stub defaults to
`None`.

## Available subcommands (from the deepmind script)

_(this skill has no Python subcommands; check the deepmind SKILL.md for the workflow)_

## Auth

See `skills/scientific-skills-gdm/pymol/SKILL.md` for the
auth requirements of this skill. The stub itself declares no settings
of its own.

## License

Plugin manifest + this stub: MIT.
Deepmind skill + vendored scripts: Apache 2.0 (see
`skills/scientific-skills-gdm/LICENSE`).
