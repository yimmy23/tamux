---
name: foldseek-structural-search
description: >
  Long-tail stub for the deepmind `foldseek_structural_search` skill. Performs 3D structural searches of proteins against various databases (PDB, AlphaFold, CATH, MGnify, etc.) using the Foldseek API. Use ONLY when the user provides a physical 3D coordinate file (.cif, .mmcif, or .pdb) and wants to find structurally similar proteins. Do NOT use if the user only provides a protein sequence, gene name, or UniProt ID.
  For the full workflow, read
  `skills/scientific-skills-gdm/foldseek_structural_search/SKILL.md` in the repo.
---

# foldseek-structural-search (zorai stub)

This is a **stub** sub-plugin. The canonical workflow and full command
reference live in the deepmind bundle at
`skills/scientific-skills-gdm/foldseek_structural_search/SKILL.md`. Read that first.

## Calling this stub

The stub exposes a single `run` command that forwards to the deepmind
Python entry script (`scripts/search.py`):

```bash
FOLDSEEK_STRUCTURAL_SEARCH_ARGS="<deepmind-subcommand-and-its-flags>" \
zorai plugin invoke foldseek-structural-search run
```

Example (for `pubmed-database`):

```bash
PUBMED_DATABASE_ARGS="search --query 'BRCA1 AND clinsig_pathogenic' --output /tmp/pubmed.json" \
zorai plugin invoke pubmed-database run
```

If the skill has multiple Python scripts and you need a non-default one,
override with `SCRIPT=<other-script.py>` env var; the stub defaults to
`search.py`.

## Available subcommands (from the deepmind script)

- `search`

## Auth

See `skills/scientific-skills-gdm/foldseek_structural_search/SKILL.md` for the
auth requirements of this skill. The stub itself declares no settings
of its own.

## License

Plugin manifest + this stub: MIT.
Deepmind skill + vendored scripts: Apache 2.0 (see
`skills/scientific-skills-gdm/LICENSE`).
