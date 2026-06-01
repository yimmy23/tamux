---
name: literature-search-biorxiv
description: >
  Long-tail stub for the deepmind `literature_search_biorxiv` skill. Browse, filter, and download life sciences, biology, and medical preprints from bioRxiv and medRxiv. Supports fetching paper metadata by DOI, and browsing by date range with category and keyword filters. Keyword filtering is local, so date ranges MUST be narrow (1-4 weeks) with a category to prevent timeouts.
  For the full workflow, read
  `skills/scientific-skills-gdm/literature_search_biorxiv/SKILL.md` in the repo.
---

# literature-search-biorxiv (zorai stub)

This is a **stub** sub-plugin. The canonical workflow and full command
reference live in the deepmind bundle at
`skills/scientific-skills-gdm/literature_search_biorxiv/SKILL.md`. Read that first.

## Calling this stub

The stub exposes a single `run` command that forwards to the deepmind
Python entry script (`scripts/search_by_dates.py`):

```bash
LITERATURE_SEARCH_BIORXIV_ARGS="<deepmind-subcommand-and-its-flags>" \
zorai plugin invoke literature-search-biorxiv run
```

Example (for `pubmed-database`):

```bash
PUBMED_DATABASE_ARGS="search --query 'BRCA1 AND clinsig_pathogenic' --output /tmp/pubmed.json" \
zorai plugin invoke pubmed-database run
```

If the skill has multiple Python scripts and you need a non-default one,
override with `SCRIPT=<other-script.py>` env var; the stub defaults to
`search_by_dates.py`.

## Available subcommands (from the deepmind script)

- `search_by_dates`
- `search_by_doi`

## Auth

See `skills/scientific-skills-gdm/literature_search_biorxiv/SKILL.md` for the
auth requirements of this skill. The stub itself declares no settings
of its own.

## License

Plugin manifest + this stub: MIT.
Deepmind skill + vendored scripts: Apache 2.0 (see
`skills/scientific-skills-gdm/LICENSE`).
