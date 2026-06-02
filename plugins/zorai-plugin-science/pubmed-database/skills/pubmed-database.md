---
name: pubmed-database
description: >
  Long-tail stub for the deepmind `pubmed_database` skill. Search PubMed for scientific literature, including published clinical trials. Fetch abstracts and full text. Link published research to biological databases (gene, protein, nucleotide, PubChem) to discover associations between papers and specific compounds or genes. Verify medical spelling, match raw citations, and cache result sets for bulk processing. Interfaces NCBI E-utilities and PMC BioC APIs.
  For the full workflow, read
  `skills/scientific-skills-gdm/pubmed_database/SKILL.md` in the repo.
---

# pubmed-database (zorai stub)

This is a **stub** sub-plugin. The canonical workflow and full command
reference live in the deepmind bundle at
`skills/scientific-skills-gdm/pubmed_database/SKILL.md`. Read that first.

## Calling this stub

The stub exposes a single `/pubmed-database.run` command that forwards to the deepmind
Python entry script (`scripts/pubmed_api.py`).

```bash
PUBMED_DATABASE_ARGS="<deepmind-subcommand-and-its-flags>" \
/pubmed-database.run
```

Replace `<deepmind-subcommand-and-its-flags>` with the subcommand and flags
described in the upstream skill.

## Available subcommands (from the deepmind script)

- `pubmed_api`

## Auth

See `skills/scientific-skills-gdm/pubmed_database/SKILL.md` for the
auth requirements of this skill. The stub itself declares no settings
of its own.

## License

Plugin manifest + this stub: MIT.
Deepmind skill + vendored scripts: Apache 2.0 (see
`skills/scientific-skills-gdm/LICENSE`).
