---
name: literature-search-arxiv
description: >
  Long-tail stub for the deepmind `literature_search_arxiv` skill. Search for scientific papers, preprints, and publications on arXiv. Extract metadata, abstracts, and download full-text PDFs or HTML versions of papers. Use when the user asks to find research papers, literature, or specific arXiv IDs.
  For the full workflow, read
  `skills/scientific-skills-gdm/literature_search_arxiv/SKILL.md` in the repo.
---

# literature-search-arxiv (zorai stub)

This is a **stub** sub-plugin. The canonical workflow and full command
reference live in the deepmind bundle at
`skills/scientific-skills-gdm/literature_search_arxiv/SKILL.md`. Read that first.

## Calling this stub

The stub exposes a single `run` command that forwards to the deepmind
Python entry script (`scripts/download_paper.py`):

```bash
LITERATURE_SEARCH_ARXIV_ARGS="<deepmind-subcommand-and-its-flags>" \
zorai plugin invoke literature-search-arxiv run
```

Example (for `pubmed-database`):

```bash
PUBMED_DATABASE_ARGS="search --query 'BRCA1 AND clinsig_pathogenic' --output /tmp/pubmed.json" \
zorai plugin invoke pubmed-database run
```

If the skill has multiple Python scripts and you need a non-default one,
override with `SCRIPT=<other-script.py>` env var; the stub defaults to
`download_paper.py`.

## Available subcommands (from the deepmind script)

- `download_paper`
- `download_paper_source`
- `search_arxiv`

## Auth

See `skills/scientific-skills-gdm/literature_search_arxiv/SKILL.md` for the
auth requirements of this skill. The stub itself declares no settings
of its own.

## License

Plugin manifest + this stub: MIT.
Deepmind skill + vendored scripts: Apache 2.0 (see
`skills/scientific-skills-gdm/LICENSE`).
