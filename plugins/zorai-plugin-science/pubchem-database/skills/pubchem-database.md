---
name: pubchem-database
description: >
  Long-tail stub for the deepmind `pubchem_database` skill. Query PubChem, search by name/CID/SMILES, retrieve properties, similarity/substructure searches, bioactivity, for cheminformatics. Use when a user asks about a specific chemical, drug, or molecule.
  For the full workflow, read
  `skills/scientific-skills-gdm/pubchem_database/SKILL.md` in the repo.
---

# pubchem-database (zorai stub)

This is a **stub** sub-plugin. The canonical workflow and full command
reference live in the deepmind bundle at
`skills/scientific-skills-gdm/pubchem_database/SKILL.md`. Read that first.

## Calling this stub

The stub exposes a single `/pubchem-database.run` command that forwards to the deepmind
Python entry script (`scripts/pubchem_api.py`).

```bash
PUBCHEM_DATABASE_ARGS="<deepmind-subcommand-and-its-flags>" \
/pubchem-database.run
```

Replace `<deepmind-subcommand-and-its-flags>` with the subcommand and flags
described in the upstream skill.

## Available subcommands (from the deepmind script)

- `assays`
- `image`
- `pharmacology`
- `properties`
- `query`
- `range`
- `resolve`
- `safety`
- `similarity`
- `substructure`
- `synonyms`
- `view`
- `xrefs`

## Auth

See `skills/scientific-skills-gdm/pubchem_database/SKILL.md` for the
auth requirements of this skill. The stub itself declares no settings
of its own.

## License

Plugin manifest + this stub: MIT.
Deepmind skill + vendored scripts: Apache 2.0 (see
`skills/scientific-skills-gdm/LICENSE`).
