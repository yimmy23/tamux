---
name: quickgo-database
description: >
  Long-tail stub for the deepmind `quickgo_database` skill. Query the QuickGO and Evidence & Conclusion Ontology (ECO) REST API. Use this when you need to map genes to biological processes, molecular functions, or cellular components, find genes associated with a specific pathway/GO term, or explore the Gene Ontology hierarchy. Do not use for querying drug targets (use OpenTargets) or mechanistic signaling pathway diagrams (use KEGG).
  For the full workflow, read
  `skills/scientific-skills-gdm/quickgo_database/SKILL.md` in the repo.
---

# quickgo-database (zorai stub)

This is a **stub** sub-plugin. The canonical workflow and full command
reference live in the deepmind bundle at
`skills/scientific-skills-gdm/quickgo_database/SKILL.md`. Read that first.

## Calling this stub

The stub exposes a single `/quickgo-database.run` command that forwards to the deepmind
Python entry script (`scripts/quickgo_tool.py`).

```bash
QUICKGO_DATABASE_ARGS="<deepmind-subcommand-and-its-flags>" \
/quickgo-database.run
```

Replace `<deepmind-subcommand-and-its-flags>` with the subcommand and flags
described in the upstream skill.

## Available subcommands (from the deepmind script)

- `annotation`
- `eco`
- `geneproduct`
- `go`
- `search`
- `slim`
- `terms`

## Auth

See `skills/scientific-skills-gdm/quickgo_database/SKILL.md` for the
auth requirements of this skill. The stub itself declares no settings
of its own.

## License

Plugin manifest + this stub: MIT.
Deepmind skill + vendored scripts: Apache 2.0 (see
`skills/scientific-skills-gdm/LICENSE`).
