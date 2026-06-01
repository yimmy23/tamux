---
name: literature-search-europepmc
description: >
  Long-tail stub for the deepmind `literature_search_europepmc` skill. Search Europe PMC for scientific literature and download open-access full texts and PDFs. Retrieve full-text XML/plain text by PMCID, get citation lists and bibliography.
  For the full workflow, read
  `skills/scientific-skills-gdm/literature_search_europepmc/SKILL.md` in the repo.
---

# literature-search-europepmc (zorai stub)

This is a **stub** sub-plugin. The canonical workflow and full command
reference live in the deepmind bundle at
`skills/scientific-skills-gdm/literature_search_europepmc/SKILL.md`. Read that first.

## Calling this stub

The stub exposes a single `run` command that forwards to the deepmind
Python entry script (`scripts/europepmc_api.py`):

```bash
LITERATURE_SEARCH_EUROPEPMC_ARGS="<deepmind-subcommand-and-its-flags>" \
zorai plugin invoke literature-search-europepmc run
```

Example (for `pubmed-database`):

```bash
PUBMED_DATABASE_ARGS="search --query 'BRCA1 AND clinsig_pathogenic' --output /tmp/pubmed.json" \
zorai plugin invoke pubmed-database run
```

If the skill has multiple Python scripts and you need a non-default one,
override with `SCRIPT=<other-script.py>` env var; the stub defaults to
`europepmc_api.py`.

## Available subcommands (from the deepmind script)

- `download_pdf`
- `get_citations`
- `get_fulltext`
- `get_references`
- `search`

## Auth

See `skills/scientific-skills-gdm/literature_search_europepmc/SKILL.md` for the
auth requirements of this skill. The stub itself declares no settings
of its own.

## License

Plugin manifest + this stub: MIT.
Deepmind skill + vendored scripts: Apache 2.0 (see
`skills/scientific-skills-gdm/LICENSE`).
