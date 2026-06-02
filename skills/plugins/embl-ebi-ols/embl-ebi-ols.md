---
name: embl-ebi-ols
description: >
  Long-tail stub for the deepmind `embl_ebi_ols` skill. Query and search the EMBL-EBI Ontology Lookup Service (OLS) for biomedical ontology terms, definitions, and hierarchies across 250+ ontologies (e.g., GO, DOID, HP). Use when the user asks to search for terms, retrieve details, navigate hierarchies (parents, children, ancestors), look up properties and individuals, get autocomplete suggestions, or access ontology metadata and statistics.
  For the full workflow, read
  `skills/scientific-skills-gdm/embl_ebi_ols/SKILL.md` in the repo.
---

# embl-ebi-ols (zorai stub)

This is a **stub** sub-plugin. The canonical workflow and full command
reference live in the deepmind bundle at
`skills/scientific-skills-gdm/embl_ebi_ols/SKILL.md`. Read that first.

## Calling this stub

The stub exposes a single `/embl-ebi-ols.run` command that forwards to the deepmind
Python entry script (`scripts/get_individual.py`).

```bash
EMBL_EBI_OLS_ARGS="<deepmind-subcommand-and-its-flags>" \
/embl-ebi-ols.run
```

Replace `<deepmind-subcommand-and-its-flags>` with the subcommand and flags
described in the upstream skill.

## Available subcommands (from the deepmind script)

- `get_individual`
- `get_ontology`
- `get_property`
- `get_stats`
- `get_term`
- `ols_utils`
- `search_ols`
- `suggest_ols`

## Auth

See `skills/scientific-skills-gdm/embl_ebi_ols/SKILL.md` for the
auth requirements of this skill. The stub itself declares no settings
of its own.

## License

Plugin manifest + this stub: MIT.
Deepmind skill + vendored scripts: Apache 2.0 (see
`skills/scientific-skills-gdm/LICENSE`).
